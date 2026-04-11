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

    #[arg(long, help = "Print a header row for tabular output")]
    headers: bool,

    #[arg(long, help = "Dim every other output row")]
    stripe: bool,

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

    #[arg(
        short = 'l',
        long = "long",
        help = "Alias for --date --size --attrs=flag --preview"
    )]
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
        args.date.get_or_insert_with(|| "ls".into());
        args.size.get_or_insert_with(|| "human".into());
        args.preview = true;
        if args.attrs.is_none() {
            args.attrs = Some("flag".into());
        }
    }
    let color = color_enabled(&args.color)?;
    let stripe = color && args.stripe;
    let style_color = color && !args.stripe;
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

    let has_size = args.size.is_some();
    let has_date = args.date.is_some();
    let show_count = attrs_mode == Some(AttrsMode::Count);
    let show_flag = attrs_mode == Some(AttrsMode::Flag);
    let show_name = args.name;
    let show_preview = args.preview;
    let has_display_tags = !meta_sel.display_tags.is_empty();
    let show_all_meta = meta_sel.show_all;

    let simple_ids_only = !has_date
        && !has_size
        && !show_name
        && !show_preview
        && !show_count
        && !show_flag
        && !show_all_meta
        && !has_display_tags;
    if simple_ids_only && !args.headers {
        let stdout = io::stdout();
        let mut out = io::BufWriter::new(stdout.lock());
        let rows = decorate_entries(&items, &args.id, ls_date_mode, args.chars, &meta_sel);
        for (idx, row) in rows.into_iter().enumerate() {
            if stripe && idx % 2 == 1 {
                writeln!(out, "{}", dim_ansi_line(&row.id))?;
            } else {
                write_colored(&mut out, &row.id, "1;33", style_color)?;
                writeln!(out)?;
            }
        }
        return Ok(());
    }

    let use_bytes = args.size.as_deref() == Some("bytes");
    let num_tags = meta_sel.display_tags.len();

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
    let auto_preview = show_preview && args.chars == 80;
    let initial_chars = if auto_preview { 0 } else { args.chars };
    let decorated = decorate_entries(&items, &args.id, ls_date_mode, initial_chars, &meta_sel);
    let num_items = decorated.len();
    let mut rows = Vec::with_capacity(num_items);

    let mut max_id = 0usize;
    let mut max_size = 0usize;
    let mut max_date = 0usize;
    let mut max_name = 0usize;
    let mut max_attr_count = 0usize;
    let mut max_attr_flag = 0usize;
    let mut max_inline_meta = 0usize;
    let mut meta_widths = vec![0usize; num_tags];

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
        } = row;

        let size_val = if has_size {
            let s = if use_bytes { size_bytes } else { size_human };
            max_size = max_size.max(s.len());
            s
        } else {
            String::new()
        };

        let date_val = if has_date {
            max_date = max_date.max(date.len());
            date
        } else {
            String::new()
        };

        let name_val = if show_name {
            let n = filename.unwrap_or_else(|| id.clone());
            max_name = max_name.max(n.chars().count());
            n
        } else {
            String::new()
        };

        let attr_count_val = if show_count {
            let s = item.attrs.len().to_string();
            max_attr_count = max_attr_count.max(s.len());
            s
        } else {
            String::new()
        };

        let attr_flag_val = if show_flag && !item.attrs.is_empty() {
            max_attr_flag = max_attr_flag.max(1);
            "*".to_string()
        } else {
            String::new()
        };

        max_id = max_id.max(id.len());

        if show_all_meta {
            max_inline_meta = max_inline_meta.max(meta_inline.chars().count());
        }
        for (idx, value) in meta_vals.iter().enumerate() {
            meta_widths[idx] = meta_widths[idx].max(value.chars().count());
        }

        let preview_val = if show_preview { preview } else { String::new() };

        rows.push(LsRow {
            id,
            size: size_val,
            date: date_val,
            name: name_val,
            attr_count: attr_count_val,
            attr_flag: attr_flag_val,
            meta_vals,
            meta_inline,
            preview: preview_val,
        });
    }

    let header_id = "id";
    let header_size = "size";
    let header_date = "date";
    let header_name = "name";
    let header_attrs = "attrs";
    let header_preview = "preview";
    if args.headers {
        max_id = max_id.max(header_id.len());
        if has_size {
            max_size = max_size.max(header_size.len());
        }
        if has_date {
            max_date = max_date.max(header_date.len());
        }
        if show_name {
            max_name = max_name.max(header_name.len());
        }
        if show_count {
            max_attr_count = max_attr_count.max(header_attrs.len());
        }
        if show_flag {
            max_attr_flag = max_attr_flag.max(header_attrs.len());
        }
        if show_all_meta {
            max_inline_meta = max_inline_meta.max(header_attrs.len());
        }
        for (idx, key) in meta_sel.display_tags.iter().enumerate() {
            meta_widths[idx] = meta_widths[idx].max(key.chars().count());
        }
    }

    let width = terminal_width();
    if auto_preview {
        let term_width = width.unwrap_or(80);
        let mut fixed = max_id;
        if max_size > 0 {
            fixed += 2 + max_size;
        }
        if max_date > 0 {
            fixed += 2 + max_date;
        }
        if max_name > 0 {
            fixed += 2 + max_name;
        }
        if max_attr_count > 0 {
            fixed += 2 + max_attr_count;
        }
        if max_attr_flag > 0 {
            fixed += 2 + max_attr_flag;
        }
        for &mw in &meta_widths {
            fixed += 2 + mw;
        }
        if max_inline_meta > 0 {
            fixed += 2 + max_inline_meta;
        }
        let effective_chars = term_width.saturating_sub(fixed + 2).max(20);
        for (row, item) in rows.iter_mut().zip(items.iter()) {
            if !item.preview.is_empty() {
                row.preview = preview_snippet(&item.preview, effective_chars);
            }
        }
    }

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    // Pre-allocate a reusable line buffer to avoid per-row allocations.
    let estimated_line_cap = max_id
        + max_size
        + max_date
        + max_name
        + max_attr_count
        + max_attr_flag
        + max_inline_meta
        + 128;
    let mut line = String::with_capacity(estimated_line_cap);

    if args.headers {
        line.clear();
        push_colorized(&mut line, &pad_right(header_id, max_id), "1", style_color);
        if has_size {
            line.push_str("  ");
            push_colorized(&mut line, &pad_right(header_size, max_size), "1", style_color);
        }
        if has_date {
            line.push_str("  ");
            push_colorized(&mut line, &pad_right(header_date, max_date), "1", style_color);
        }
        if show_count {
            line.push_str("  ");
            push_colorized(
                &mut line,
                &pad_right(header_attrs, max_attr_count),
                "1",
                style_color,
            );
        }
        if show_flag {
            line.push_str("  ");
            push_colorized(
                &mut line,
                &pad_right(header_attrs, max_attr_flag),
                "1",
                style_color,
            );
        }
        if show_name {
            line.push_str("  ");
            push_colorized(&mut line, &pad_right(header_name, max_name), "1", style_color);
        }
        for (idx, key) in meta_sel.display_tags.iter().enumerate() {
            line.push_str("  ");
            push_colorized(&mut line, &pad_right(key, meta_widths[idx]), "1", style_color);
        }
        if show_all_meta {
            line.push_str("  ");
            push_colorized(
                &mut line,
                &pad_right(header_attrs, max_inline_meta),
                "1",
                style_color,
            );
        }
        if show_preview {
            line.push_str("  ");
            push_colorized(&mut line, header_preview, "1", style_color);
        }
        let rendered = if let Some(w) = width {
            trim_ansi_to_width(&line, w)
        } else {
            // Avoid clone by taking ownership; we'll re-allocate on clear below.
            std::mem::take(&mut line)
        };
        writeln!(out, "{rendered}")?;
    }
    for (idx, row) in rows.iter().enumerate() {
        line.clear();
        push_colorized(&mut line, &pad_right(&row.id, max_id), "1;33", style_color);
        if !row.size.is_empty() {
            line.push_str("  ");
            line.push_str(&pad_left(&row.size, max_size));
        }
        if !row.date.is_empty() {
            line.push_str("  ");
            line.push_str(&pad_left(&row.date, max_date));
        }
        if !row.attr_count.is_empty() {
            line.push_str("  ");
            push_colorized(
                &mut line,
                &pad_left(&row.attr_count, max_attr_count),
                "35",
                style_color,
            );
        }
        if max_attr_flag > 0 {
            line.push_str("  ");
            push_colorized(
                &mut line,
                &pad_left(&row.attr_flag, max_attr_flag),
                "1;35",
                style_color,
            );
        }
        if !row.name.is_empty() {
            line.push_str("  ");
            let padded = pad_right(&row.name, max_name);
            if row.name == row.id {
                line.push_str(&padded);
            } else {
                push_colorized(&mut line, &padded, "1;36", style_color);
            }
        }
        for (idx, value) in row.meta_vals.iter().enumerate() {
            line.push_str("  ");
            push_colorized(&mut line, &pad_right(value, meta_widths[idx]), "36", style_color);
        }
        if !row.meta_inline.is_empty() {
            line.push_str("  ");
            push_colorized(
                &mut line,
                &pad_right(&row.meta_inline, max_inline_meta),
                "36",
                style_color,
            );
        }
        if !row.preview.is_empty() {
            line.push_str("  ");
            line.push_str(&row.preview);
        }
        if let Some(w) = width {
            let rendered = trim_ansi_to_width(&line, w);
            if stripe && idx % 2 == 1 {
                writeln!(out, "{}", dim_ansi_line(&rendered))?;
            } else {
                writeln!(out, "{rendered}")?;
            }
        } else {
            if stripe && idx % 2 == 1 {
                writeln!(out, "{}", dim_ansi_line(&line))?;
            } else {
                writeln!(out, "{line}")?;
            }
        }
    }
    Ok(())
}
