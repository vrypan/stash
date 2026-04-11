use clap::{ArgAction, Args};
use std::io::{self, Write};

use crate::display::*;
use crate::store::parse_meta_selection;

#[derive(Args, Debug, Clone)]
pub(crate) struct LsArgs {
    #[arg(
        long,
        default_value = "short",
        help = "ID display: short, full, or pos"
    )]
    id: String,

    #[arg(short = 'a', long = "attr", value_name = "name|+name", action = ArgAction::Append, help = "Show an attribute column, or filter with +name (repeatable)")]
    attr: Vec<String>,

    #[arg(
        short = 'A',
        long = "attrs",
        num_args = 0..=1,
        default_missing_value = "list",
        value_name = "list|count|flag",
        help = "Attribute display: list, count, or flag"
    )]
    attrs: Option<String>,

    #[arg(
        short = 'n',
        long = "number",
        default_value_t = 0,
        help = "Limit number of entries shown (0 = all)"
    )]
    number: usize,

    #[arg(short = 'r', long = "reverse", help = "Show oldest first")]
    reverse: bool,

    #[arg(long, help = "Output listing as rich JSON")]
    json: bool,

    #[arg(long, default_missing_value = "ls", num_args = 0..=1, help = "Include date column: iso, ago, or ls")]
    date: Option<String>,

    #[arg(long, default_missing_value = "human", num_args = 0..=1, help = "Include size column: human or bytes")]
    size: Option<String>,

    #[arg(
        long,
        help = "Include filename (attribute) if available, or else full ULID column"
    )]
    name: bool,

    #[arg(short = 'p', long = "preview", help = "Append compact preview text")]
    preview: bool,

    #[arg(short = 'l', long = "long", help = "Alias for --date --size --name")]
    long: bool,

    #[arg(long, default_value_t = 80, help = "Preview character limit")]
    chars: usize,

    #[arg(long, default_value = "true", help = "Color output: true or false")]
    color: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum AttrsMode {
    List,
    Count,
    Flag,
}

fn parse_attrs_mode(value: Option<&str>) -> io::Result<Option<AttrsMode>> {
    match value {
        None => Ok(None),
        Some("list") => Ok(Some(AttrsMode::List)),
        Some("count") => Ok(Some(AttrsMode::Count)),
        Some("flag") => Ok(Some(AttrsMode::Flag)),
        Some(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--attrs must be list, count, or flag",
        )),
    }
}

pub(super) fn ls_command(mut args: LsArgs) -> io::Result<()> {
    if args.long {
        args.date.get_or_insert("ls".into());
        args.size.get_or_insert("human".into());
        if args.attrs.is_none() {
            args.attrs = Some("flag".into());
        }
        if !args.attr.iter().any(|value| value == "filename") {
            args.attr.push("filename".into());
        }
    }
    let color = color_enabled(&args.color)?;
    let attrs_mode = parse_attrs_mode(args.attrs.as_deref())?;
    if let Some(mode) = args.date.as_deref() {
        args.date = Some(normalize_date_mode(mode)?.to_string());
    }
    let meta_sel = parse_meta_selection(&args.attr, attrs_mode == Some(AttrsMode::List))?;
    let items = super::collect_entries(&meta_sel, args.reverse, args.number)?;
    let ls_date_mode = args.date.as_deref().unwrap_or("ls");
    if args.json {
        print_entries_json(&items, ls_date_mode, args.chars);
        return Ok(());
    }
    if args.date.is_none()
        && args.size.is_none()
        && !args.name
        && !args.preview
        && attrs_mode != Some(AttrsMode::Count)
        && attrs_mode != Some(AttrsMode::Flag)
        && !meta_sel.show_all
        && meta_sel.display_tags.is_empty()
    {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        let rows = decorate_entries(&items, &args.id, ls_date_mode, args.chars, &meta_sel);
        for row in rows {
            write_colored(&mut out, &row.id, "1;33", color)?;
            writeln!(out)?;
        }
        return Ok(());
    }

    struct LsRow {
        id: String,
        size: String,
        date: String,
        name: String,
        attr_count: String,
        attr_flag: String,
        meta_vals: Vec<String>,
        meta_inline: String,
        preview: String,
    }

    // When auto-computing preview width, build entries with empty previews first;
    // effective_chars will be derived from the column measurement loop below.
    let initial_chars = if args.preview && args.chars == 80 { 0 } else { args.chars };
    let decorated = decorate_entries(&items, &args.id, ls_date_mode, initial_chars, &meta_sel);
    let mut rows = Vec::with_capacity(decorated.len());
    for (row, item) in decorated.into_iter().zip(items.iter()) {
        let DecoratedEntry {
            id,
            size_bytes,
            size_human,
            date,
            preview,
            filename,
            meta_vals,
            meta_inline,
            log_attr_lines: _,
        } = row;
        rows.push(LsRow {
            id: id.clone(),
            size: args
                .size
                .as_deref()
                .map(|mode| {
                    if mode == "bytes" {
                        size_bytes.clone()
                    } else {
                        size_human.clone()
                    }
                })
                .unwrap_or_default(),
            date: if args.date.is_some() {
                date
            } else {
                String::new()
            },
            name: if args.name {
                filename.unwrap_or_else(|| id.clone())
            } else {
                String::new()
            },
            attr_count: if attrs_mode == Some(AttrsMode::Count) {
                item.attrs.len().to_string()
            } else {
                String::new()
            },
            attr_flag: if attrs_mode == Some(AttrsMode::Flag) && !item.attrs.is_empty() {
                "*".to_string()
            } else {
                String::new()
            },
            meta_vals,
            meta_inline,
            preview: if args.preview { preview } else { String::new() },
        });
    }

    let mut max_id = 0usize;
    let mut max_size = 0usize;
    let mut max_date = 0usize;
    let mut max_name = 0usize;
    let mut max_attr_count = 0usize;
    let mut max_attr_flag = 0usize;
    let mut max_inline_meta = 0usize;
    let mut meta_widths = vec![0usize; meta_sel.display_tags.len()];
    for row in &rows {
        max_id = max_id.max(row.id.chars().count());
        max_size = max_size.max(row.size.chars().count());
        max_date = max_date.max(row.date.chars().count());
        max_name = max_name.max(row.name.chars().count());
        max_attr_count = max_attr_count.max(row.attr_count.chars().count());
        max_attr_flag = max_attr_flag.max(row.attr_flag.chars().count());
        max_inline_meta = max_inline_meta.max(row.meta_inline.chars().count());
        for (idx, value) in row.meta_vals.iter().enumerate() {
            meta_widths[idx] = meta_widths[idx].max(value.chars().count());
        }
    }

    let width = terminal_width();
    if args.preview && args.chars == 80 {
        let term_width = width.unwrap_or(80);
        let mut fixed = max_id;
        if max_size > 0 { fixed += 2 + max_size; }
        if max_date > 0 { fixed += 2 + max_date; }
        if max_name > 0 { fixed += 2 + max_name; }
        if max_attr_count > 0 { fixed += 2 + max_attr_count; }
        if max_attr_flag > 0 { fixed += 2 + max_attr_flag; }
        for &mw in &meta_widths { fixed += 2 + mw; }
        if max_inline_meta > 0 { fixed += 2 + max_inline_meta; }
        let effective_chars = term_width.saturating_sub(fixed + 2).max(20);
        for (row, item) in rows.iter_mut().zip(items.iter()) {
            if !item.preview.is_empty() {
                row.preview = preview_snippet(&item.preview, effective_chars);
            }
        }
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    for row in rows {
        let mut line = String::new();
        push_colorized(&mut line, &pad_right(&row.id, max_id), "1;33", color);
        if !row.size.is_empty() {
            line.push_str("  ");
            line.push_str(&pad_left(&row.size, max_size));
        }
        if !row.date.is_empty() {
            line.push_str("  ");
            line.push_str(&pad_left(&row.date, max_date));
        }
        if !row.name.is_empty() {
            line.push_str("  ");
            let padded = pad_right(&row.name, max_name);
            if row.name == row.id {
                line.push_str(&padded);
            } else {
                push_colorized(&mut line, &padded, "1;36", color);
            }
        }
        if !row.attr_count.is_empty() {
            line.push_str("  ");
            push_colorized(&mut line, &pad_left(&row.attr_count, max_attr_count), "35", color);
        }
        if max_attr_flag > 0 {
            line.push_str("  ");
            push_colorized(&mut line, &pad_left(&row.attr_flag, max_attr_flag), "35", color);
        }
        for (idx, value) in row.meta_vals.iter().enumerate() {
            line.push_str("  ");
            push_colorized(&mut line, &pad_right(value, meta_widths[idx]), "35", color);
        }
        if !row.meta_inline.is_empty() {
            line.push_str("  ");
            push_colorized(
                &mut line,
                &pad_right(&row.meta_inline, max_inline_meta),
                "35",
                color,
            );
        }
        if !row.preview.is_empty() {
            line.push_str("  ");
            line.push_str(&row.preview);
        }
        let rendered = if let Some(width) = width {
            trim_ansi_to_width(&line, width)
        } else {
            line
        };
        writeln!(out, "{rendered}")?;
    }
    Ok(())
}
