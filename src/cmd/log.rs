use clap::{ArgAction, Args};
use std::io::{self, Write};

use crate::display::*;
use crate::store::parse_meta_selection;

#[derive(Args, Debug, Clone)]
pub(crate) struct LogArgs {
    #[arg(long, default_value = "full", help = "ID display: short, full, or pos")]
    id: String,

    #[arg(short = 'a', long = "attr", value_name = "name", action = ArgAction::Append, help = "Filter by attribute name (repeatable)")]
    attr: Vec<String>,

    #[arg(
        short = 'A',
        long = "attrs",
        help = "Show all attributes where available"
    )]
    attrs: bool,

    #[arg(
        short = 'n',
        long = "number",
        default_value_t = 0,
        help = "Limit number of entries shown (0 = all)"
    )]
    number: usize,

    #[arg(short = 'r', long = "reverse", help = "Show oldest first")]
    reverse: bool,

    #[arg(long, help = "Output verbose entry history as JSON")]
    json: bool,

    #[arg(long, help = "Format string with {field} and {attr:key} placeholders")]
    format: Option<String>,

    #[arg(long, default_value = "iso", help = "Date format: iso or ago")]
    date: String,

    #[arg(long, default_value_t = 80, help = "Preview character limit")]
    chars: usize,

    #[arg(long, default_value = "true", help = "Color output: true or false")]
    color: String,
}

pub(super) fn log_command(args: LogArgs) -> io::Result<()> {
    let color = color_enabled(&args.color)?;
    let date_mode = normalize_date_mode(&args.date)?;
    let meta_sel = parse_meta_selection(&args.attr, args.attrs)?;
    let items = super::collect_entries(&meta_sel, args.reverse, args.number)?;
    if args.json {
        print_log_json(&items, date_mode, args.chars);
        return Ok(());
    }
    if let Some(format) = args.format.as_deref() {
        return print_log_template(&items, date_mode, args.chars, format, color);
    }

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let rows = decorate_entries(&items, &args.id, date_mode, args.chars, &meta_sel);
    for (idx, row) in rows.into_iter().enumerate() {
        if idx > 0 {
            writeln!(out)?;
        }
        write!(out, "entry ")?;
        write_colored(&mut out, &row.id, "1;33", color)?;
        writeln!(out, " ({})", row.size_human)?;
        write_colored(&mut out, "date: ", "1", color)?;
        writeln!(out, "{}", row.date)?;
        if !row.log_attr_lines.is_empty() {
            for (key, value) in row.log_attr_lines {
                write_colored(&mut out, &format!("{key}: "), "1", color)?;
                write_colored(&mut out, &value, "35", color)?;
                writeln!(out)?;
            }
        }
        if !row.preview.is_empty() {
            writeln!(out)?;
            writeln!(out, "    {}", row.preview)?;
        }
    }
    Ok(())
}
