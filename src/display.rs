use crate::store::{self, Meta, MetaSelection};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::io::{self, IsTerminal, Write};
use std::mem::MaybeUninit;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Decorated entries (display-ready representations)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub(crate) struct DecoratedEntry {
    pub(crate) id: String,
    pub(crate) size_bytes: String,
    pub(crate) size_human: String,
    pub(crate) date: String,
    pub(crate) preview: String,
    pub(crate) filename: Option<String>,
    pub(crate) meta_vals: Vec<String>,
    pub(crate) meta_inline: String,
    pub(crate) log_attr_lines: Vec<(String, String)>,
}

pub(crate) fn decorate_entries(
    items: &[Meta],
    id_mode: &str,
    date_mode: &str,
    preview_chars: usize,
    meta_sel: &MetaSelection,
) -> Vec<DecoratedEntry> {
    items
        .iter()
        .enumerate()
        .map(|(idx, item)| decorate_entry(item, idx, id_mode, date_mode, preview_chars, meta_sel))
        .collect()
}

fn decorate_entry(
    item: &Meta,
    idx: usize,
    id_mode: &str,
    date_mode: &str,
    preview_chars: usize,
    meta_sel: &MetaSelection,
) -> DecoratedEntry {
    let filename = item.attrs.get("filename").cloned();
    let meta_vals = if !meta_sel.display_tags.is_empty() {
        meta_sel
            .display_tags
            .iter()
            .map(|tag| {
                item.attrs
                    .get(tag)
                    .map(|value| escape_attr_output(value))
                    .unwrap_or_else(|| " ".into())
            })
            .collect()
    } else {
        Vec::new()
    };
    let meta_inline = if meta_sel.show_all && !item.attrs.is_empty() {
        item.attrs
            .values()
            .map(|value| escape_attr_output(value))
            .collect::<Vec<_>>()
            .join("  ")
    } else {
        String::new()
    };
    let log_attr_lines = if meta_sel.show_all || !meta_sel.display_tags.is_empty() {
        if meta_sel.show_all {
            item.attrs
                .iter()
                .map(|(k, v)| (k.clone(), escape_attr_output(v)))
                .collect()
        } else {
            meta_sel
                .display_tags
                .iter()
                .filter_map(|tag| {
                    item.attrs
                        .get(tag)
                        .map(|v| (tag.clone(), escape_attr_output(v)))
                })
                .collect()
        }
    } else {
        Vec::new()
    };
    let preview = if item.preview.is_empty() {
        String::new()
    } else {
        preview_snippet(&item.preview, preview_chars)
    };
    DecoratedEntry {
        id: display_id(item, idx, id_mode),
        size_bytes: item.size.to_string(),
        size_human: store::human_size(item.size),
        date: format_date(&item.ts, date_mode),
        preview,
        filename,
        meta_vals,
        meta_inline,
        log_attr_lines,
    }
}

// ---------------------------------------------------------------------------
// ID / escape helpers
// ---------------------------------------------------------------------------

pub(crate) fn display_id(item: &Meta, idx: usize, mode: &str) -> String {
    match mode {
        "full" => item.display_id(),
        "pos" => (idx + 1).to_string(),
        _ => item.short_id(),
    }
}

// Escapes a value for human-readable display output.
// '=' is intentionally NOT escaped here (unlike escape_attr in store.rs)
// because it has no special meaning outside the storage format.
pub(crate) fn escape_attr_output(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

pub(crate) fn preview_snippet(preview: &str, chars: usize) -> String {
    if chars == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut it = preview.chars();
    for _ in 0..chars {
        match it.next() {
            Some(ch) => out.push(ch),
            None => return out,
        }
    }
    if it.next().is_some() && chars > 3 {
        out.push_str("...");
    }
    out
}

pub(crate) fn is_writable_attr_key(key: &str) -> bool {
    match key {
        "id" | "ts" | "size" | "preview" => false,
        _ => {
            if key.is_empty() || key.starts_with('-') || key.ends_with('-') {
                return false;
            }
            let mut prev_dash = false;
            for ch in key.chars() {
                let ok = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
                if !ok {
                    return false;
                }
                if ch == '-' {
                    if prev_dash {
                        return false;
                    }
                    prev_dash = true;
                } else {
                    prev_dash = false;
                }
            }
            true
        }
    }
}

pub(crate) fn attr_value(meta: &Meta, key: &str, with_preview: bool) -> Option<String> {
    match key {
        "id" => Some(meta.display_id()),
        "ts" => Some(meta.ts.clone()),
        "size" => Some(meta.size.to_string()),
        "preview" if with_preview || !meta.preview.is_empty() => {
            (!meta.preview.is_empty()).then(|| meta.preview.clone())
        }
        _ => meta.attrs.get(key).cloned(),
    }
}

// ---------------------------------------------------------------------------
// Terminal / color / padding
// ---------------------------------------------------------------------------

pub(crate) fn color_enabled(value: &str) -> io::Result<bool> {
    match value {
        "true" => Ok(io::stdout().is_terminal()),
        "false" => Ok(false),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--color must be true or false",
        )),
    }
}

pub(crate) fn push_colorized(buf: &mut String, s: &str, code: &str, enabled: bool) {
    if enabled && !s.is_empty() {
        let _ = write!(buf, "\x1b[{code}m{s}\x1b[0m");
    } else {
        buf.push_str(s);
    }
}

pub(crate) fn write_colored<W: Write>(
    out: &mut W,
    s: &str,
    code: &str,
    enabled: bool,
) -> io::Result<()> {
    if enabled && !s.is_empty() {
        write!(out, "\x1b[{code}m{s}\x1b[0m")
    } else {
        write!(out, "{s}")
    }
}

pub(crate) fn pad_right(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - len))
    }
}

pub(crate) fn pad_left(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.to_string()
    } else {
        format!("{}{}", " ".repeat(width - len), s)
    }
}

pub(crate) fn terminal_width() -> Option<usize> {
    if !io::stdout().is_terminal() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;

        #[repr(C)]
        struct WinSize {
            ws_row: u16,
            ws_col: u16,
            ws_xpixel: u16,
            ws_ypixel: u16,
        }

        unsafe extern "C" {
            fn ioctl(fd: i32, request: u64, ...) -> i32;
        }

        const TIOCGWINSZ: u64 = 0x40087468;
        let fd = io::stdout().as_raw_fd();
        let mut ws = MaybeUninit::<WinSize>::uninit();
        // SAFETY: ws points to writable memory for ioctl to populate.
        let rc = unsafe { ioctl(fd, TIOCGWINSZ, ws.as_mut_ptr()) };
        if rc == 0 {
            // SAFETY: ioctl succeeded and initialized ws.
            let ws = unsafe { ws.assume_init() };
            if ws.ws_col > 0 {
                return Some(ws.ws_col as usize);
            }
        }
    }
    None
}

pub(crate) fn trim_ansi_to_width(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let bytes = s.as_bytes();
    let mut out = String::new();
    let mut visible = 0usize;
    let mut chars = s.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch == '\x1b' && bytes.get(i + 1) == Some(&b'[') {
            let end = bytes[i + 2..]
                .iter()
                .position(|&b| (0x40..=0x7e).contains(&b))
                .map(|p| i + 2 + p + 1)
                .unwrap_or(bytes.len());
            out.push_str(&s[i..end]);
            while chars.peek().map(|(j, _)| *j < end).unwrap_or(false) {
                chars.next();
            }
            continue;
        }
        if visible >= width {
            break;
        }
        out.push(ch);
        visible += 1;
    }
    if visible >= width {
        out.push_str("\x1b[0m");
    }
    out
}

// ---------------------------------------------------------------------------
// Date formatting
// ---------------------------------------------------------------------------

pub(crate) fn normalize_date_mode(mode: &str) -> io::Result<&str> {
    match mode {
        "absolute" => Ok("iso"),
        "relative" => Ok("ago"),
        "iso" | "ago" | "ls" => Ok(mode),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--date must be iso, ago, or ls",
        )),
    }
}

pub(crate) fn format_date(ts: &str, mode: &str) -> String {
    match normalize_date_mode(mode).unwrap_or("iso") {
        "ago" => format_relative(ts).unwrap_or_else(|| ts.to_string()),
        "ls" => format_ls_date(ts).unwrap_or_else(|| ts.to_string()),
        _ => ts.to_string(),
    }
}

fn format_relative(ts: &str) -> Option<String> {
    let then = parse_ts_seconds(ts)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    let delta = now.saturating_sub(then);
    Some(if delta < 60 {
        format!("{}s ago", delta)
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86_400 {
        format!("{}h ago", delta / 3600)
    } else {
        format!("{}d ago", delta / 86_400)
    })
}

fn format_ls_date(ts: &str) -> Option<String> {
    let (year, month, day, hour, minute, _) = parse_ts_parts(ts)?;
    let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    let now_year = store::unix_to_utc(now_secs).year;
    let mon = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    if year == now_year {
        Some(format!(
            "{} {:>2} {:02}:{:02}",
            mon[(month - 1) as usize],
            day,
            hour,
            minute
        ))
    } else {
        Some(format!(
            "{} {:>2}  {}",
            mon[(month - 1) as usize],
            day,
            year
        ))
    }
}

fn parse_ts_seconds(ts: &str) -> Option<i64> {
    let (year, month, day, hour, minute, second) = parse_ts_parts(ts)?;
    Some(
        civil_to_days(year, month, day) * 86_400
            + hour as i64 * 3600
            + minute as i64 * 60
            + second as i64,
    )
}

fn parse_ts_parts(ts: &str) -> Option<(i32, u32, u32, u32, u32, u32)> {
    let date = ts.get(0..10)?;
    let time = ts.get(11..19)?;
    Some((
        date.get(0..4)?.parse().ok()?,
        date.get(5..7)?.parse().ok()?,
        date.get(8..10)?.parse().ok()?,
        time.get(0..2)?.parse().ok()?,
        time.get(3..5)?.parse().ok()?,
        time.get(6..8)?.parse().ok()?,
    ))
}

fn civil_to_days(year: i32, month: u32, day: u32) -> i64 {
    let mut y = year as i64;
    let m = month as i64;
    let d = day as i64;
    y -= if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

// ---------------------------------------------------------------------------
// Log output
// ---------------------------------------------------------------------------

pub(crate) fn print_entries_json(items: &[Meta], date_mode: &str, chars: usize) {
    #[derive(Serialize)]
    struct LogJsonEntry {
        id: String,
        short_id: String,
        stack_ref: String,
        ts: String,
        date: String,
        size: i64,
        size_human: String,
        #[serde(flatten)]
        attrs: BTreeMap<String, String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        preview: Vec<String>,
    }

    let out: Vec<LogJsonEntry> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let preview = preview_snippet(&item.preview, chars);
            LogJsonEntry {
                id: item.display_id(),
                short_id: item.short_id(),
                stack_ref: (idx + 1).to_string(),
                ts: item.ts.clone(),
                date: format_date(&item.ts, date_mode),
                size: item.size,
                size_human: store::human_size(item.size),
                attrs: item.attrs.clone(),
                preview: if preview.is_empty() {
                    Vec::new()
                } else {
                    vec![preview]
                },
            }
        })
        .collect();

    serde_json::to_writer_pretty(io::stdout(), &out).expect("write log json");
    println!();
}

fn log_template_value(
    item: &Meta,
    idx: usize,
    date_mode: &str,
    chars: usize,
    key: &str,
) -> Option<String> {
    match key {
        "id" => Some(item.display_id()),
        "short_id" => Some(item.short_id()),
        "stack_ref" => Some((idx + 1).to_string()),
        "ts" => Some(item.ts.clone()),
        "date" => Some(format_date(&item.ts, date_mode)),
        "size" => Some(item.size.to_string()),
        "size_human" => Some(store::human_size(item.size)),
        "preview" => {
            let preview = preview_snippet(&item.preview, chars);
            (!preview.is_empty()).then_some(preview)
        }
        _ => None,
    }
}

fn placeholder_color_code(expr: &str) -> Option<&'static str> {
    match expr {
        "id" | "short_id" | "stack_ref" => Some("1;33"),
        s if s.starts_with("attr:") => Some("35"),
        _ => None,
    }
}

fn render_log_template(
    item: &Meta,
    idx: usize,
    date_mode: &str,
    chars: usize,
    format: &str,
    color: bool,
) -> String {
    let mut out = String::new();
    let mut rest = format;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let expr = after[..end].trim();
        let value = if let Some(key) = expr.strip_prefix("attr:") {
            item.attrs.get(key).cloned()
        } else {
            log_template_value(item, idx, date_mode, chars, expr)
        };
        if let Some(value) = value {
            if let Some(code) = placeholder_color_code(expr) {
                push_colorized(&mut out, &value, code, color);
            } else {
                out.push_str(&value);
            }
        } else {
            out.push('{');
            out.push_str(expr);
            out.push('}');
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

pub(crate) fn print_log_template(
    items: &[Meta],
    date_mode: &str,
    chars: usize,
    format: &str,
    color: bool,
) -> io::Result<()> {
    for (idx, item) in items.iter().enumerate() {
        println!(
            "{}",
            render_log_template(item, idx, date_mode, chars, format, color)
        );
    }
    Ok(())
}
