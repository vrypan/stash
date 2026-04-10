use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use crate::display::*;
use crate::store;
use crate::store::Meta;

#[derive(Parser, Debug)]
#[command(
    name = "stash",
    version,
    about = "A local store for pipeline output and ad hoc file snapshots",
    long_about = "A local store for pipeline output and ad hoc file snapshots.\n\nWhen used without a subcommand, stash picks a mode automatically:\n  - in the middle of a pipeline, it behaves like `stash tee`\n  - otherwise, it behaves like `stash push`",
    after_help = "Links:\n  Docs: https://github.com/vrypan/stash/tree/main/docs\n  Reference: https://github.com/vrypan/stash/blob/main/docs/reference.md\n  Issues: https://github.com/vrypan/stash/issues",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(flatten)]
    push: PushArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(about = "Store and return the entry key")]
    Push(PushArgs),
    #[command(about = "Store and forward to stdout")]
    Tee(TeeArgs),
    #[command(about = "Print an entry's raw data to stdout")]
    Cat(CatArgs),
    #[command(about = "List entries")]
    Ls(LsArgs),
    #[command(alias = "list")]
    #[command(about = "Show detailed entry history")]
    Log(LogArgs),
    #[command(about = "Show or update entry attributes")]
    Attr(AttrArgs),
    #[command(about = "List attribute keys across the stash")]
    Attrs(AttrsArgs),
    #[command(about = "Print stash paths")]
    Path(PathArgs),
    #[command(about = "Remove entries")]
    Rm(RmArgs),
    #[command(about = "Print the newest entry and remove it")]
    Pop,
    #[command(about = "Generate shell completion scripts")]
    Completion(CompletionArgs),
}

#[derive(Args, Debug, Clone, Default)]
pub struct PushArgs {
    #[arg(short = 'a', long = "attr", value_name = "key=value", action = ArgAction::Append, help = "Set attribute key=value (repeatable)")]
    attr: Vec<String>,

    #[arg(long, default_value = "null", help = "Where to print the generated entry ID: stdout, stderr, null, 1, 2, or 0")]
    print: String,

    #[arg(help = "Optional file to stash; reads stdin when omitted")]
    file: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct TeeArgs {
    #[arg(short = 'a', long = "attr", value_name = "key=value", action = ArgAction::Append, help = "Set attribute key=value (repeatable)")]
    attr: Vec<String>,

    #[arg(long, default_value = "null", help = "Where to print the generated entry ID: stdout, stderr, null, 1, 2, or 0")]
    print: String,

    #[arg(long, num_args = 0..=1, default_value_t = true, default_missing_value = "true", help = "Save captured input when an upstream or processing error happens: true or false")]
    save_on_error: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct CatArgs {
    #[arg(help = "Entry reference: id, n, or @n")]
    reference: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct LsArgs {
    #[arg(
        long,
        default_value = "short",
        help = "ID display: short, full, or pos"
    )]
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

#[derive(Args, Debug, Clone)]
pub struct LogArgs {
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

#[derive(Args, Debug, Clone)]
pub struct AttrArgs {
    #[arg(help = "Entry reference: id, n, or @n")]
    reference: String,

    #[arg(
        value_name = "KEY|KEY=VALUE",
        help = "Attribute keys to read, or key=value pairs to write"
    )]
    items: Vec<String>,

    #[arg(
        long,
        default_value = "\t",
        help = "Separator used between key and value"
    )]
    separator: String,

    #[arg(long = "unset", value_name = "KEY", action = ArgAction::Append, help = "Remove attribute key (repeatable)")]
    unset: Vec<String>,

    #[arg(long, help = "Output attributes as JSON")]
    json: bool,

    #[arg(
        short = 'p',
        long = "preview",
        help = "Include preview pseudo-property when available"
    )]
    preview: bool,
}

#[derive(Args, Debug, Clone)]
pub struct AttrsArgs {
    #[arg(long, help = "Include entry count")]
    count: bool,
}

#[derive(Args, Debug, Clone)]
pub struct PathArgs {
    #[arg(
        short = 'd',
        long = "dir",
        help = "Print container directories instead of file paths"
    )]
    dir: bool,

    #[arg(
        short = 'a',
        long = "attr",
        help = "Print attribute file paths instead of data file paths"
    )]
    attr: bool,

    #[arg(help = "Entry refs read from arguments or stdin")]
    refs: Vec<String>,
}

#[derive(Args, Debug, Clone)]
pub struct RmArgs {
    #[arg(help = "Entry references to remove")]
    refs: Vec<String>,

    #[arg(long, help = "Remove entries older than the referenced entry")]
    before: Option<String>,

    #[arg(short = 'a', long = "attr", value_name = "name|name=value", action = ArgAction::Append, help = "Remove entries where an attribute is set, or equals a value (repeatable)")]
    attr: Vec<String>,

    #[arg(short = 'f', long = "force", help = "Do not prompt for confirmation")]
    force: bool,
}

#[derive(Args, Debug, Clone)]
pub struct CompletionArgs {
    #[arg(value_enum, help = "Shell to generate completion for")]
    shell: CompletionShell,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum CompletionShell {
    Bash,
    Fish,
    Zsh,
}

impl From<CompletionShell> for Shell {
    fn from(value: CompletionShell) -> Self {
        match value {
            CompletionShell::Bash => Shell::Bash,
            CompletionShell::Fish => Shell::Fish,
            CompletionShell::Zsh => Shell::Zsh,
        }
    }
}

pub fn main_entry() {
    if let Err(err) = run() {
        if err.kind() == io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        if let Some(partial_err) = err
            .get_ref()
            .and_then(|e| e.downcast_ref::<store::PartialSavedError>())
        {
            let code = match partial_err.signal {
                Some(SIGINT) => 130,
                Some(SIGTERM) => 143,
                _ => 1,
            };
            std::process::exit(code);
        }
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

pub fn run() -> io::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Push(args)) => push_command(args),
        Some(Command::Tee(args)) => tee_command(args),
        Some(Command::Cat(args)) => cat_command(args),
        Some(Command::Ls(args)) => ls_command(args),
        Some(Command::Log(args)) => log_command(args),
        Some(Command::Attr(args)) => attr_command(args),
        Some(Command::Attrs(args)) => attrs_command(args),
        Some(Command::Path(args)) => path_command(args),
        Some(Command::Rm(args)) => rm_command(args),
        Some(Command::Pop) => pop_command(),
        Some(Command::Completion(args)) => completion_command(args),
        None => {
            if smart_mode_uses_tee(&cli.push) {
                tee_command(TeeArgs {
                    attr: cli.push.attr,
                    print: cli.push.print,
                    save_on_error: true,
                })
            } else {
                push_command(cli.push)
            }
        }
    }
}

fn smart_mode_uses_tee(args: &PushArgs) -> bool {
    args.file.is_none() && !io::stdin().is_terminal() && !io::stdout().is_terminal()
}

fn parse_meta_flags(values: &[String]) -> io::Result<BTreeMap<String, String>> {
    let mut attrs = BTreeMap::new();
    for value in values {
        let Some((k, v)) = value.split_once('=') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "attribute must be key=value",
            ));
        };
        attrs.insert(k.to_string(), v.to_string());
    }
    Ok(attrs)
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PrintTarget {
    Stdout,
    Stderr,
    None,
}

fn parse_print_target(value: &str) -> io::Result<PrintTarget> {
    match value {
        "stdout" | "1" => Ok(PrintTarget::Stdout),
        "stderr" | "2" => Ok(PrintTarget::Stderr),
        "null" | "0" => Ok(PrintTarget::None),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--print must be stdout, stderr, null, 1, 2, or 0",
        )),
    }
}

fn emit_generated_id(
    target: PrintTarget,
    id: &str,
    stdout: Option<&mut dyn Write>,
) -> io::Result<()> {
    match target {
        PrintTarget::Stdout => {
            if let Some(out) = stdout {
                writeln!(out, "{id}")?;
            } else {
                println!("{id}");
            }
        }
        PrintTarget::Stderr => {
            eprintln!("{id}");
        }
        PrintTarget::None => {}
    }
    Ok(())
}

fn push_command(args: PushArgs) -> io::Result<()> {
    let mut attrs = parse_meta_flags(&args.attr)?;
    let print_target = parse_print_target(&args.print)?;
    let id = if let Some(path) = args.file {
        let mut file = File::open(&path)?;
        store::add_filename_attr(&path, &mut attrs);
        store::push_from_reader(&mut file, attrs)?
    } else {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        store::push_from_reader(&mut input, attrs)?
    };
    emit_generated_id(print_target, &id, None)?;
    Ok(())
}

fn tee_command(args: TeeArgs) -> io::Result<()> {
    let attrs = parse_meta_flags(&args.attr)?;
    let print_target = parse_print_target(&args.print)?;
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match store::tee_from_reader_partial(&mut input, &mut out, attrs, args.save_on_error) {
        Ok(id) => {
            emit_generated_id(print_target, &id, Some(&mut out))?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn cat_command(args: CatArgs) -> io::Result<()> {
    let id = if let Some(reference) = args.reference {
        store::resolve(&reference)?
    } else {
        store::newest()?.id
    };
    let stdout = io::stdout();
    store::cat_to_writer(&id, stdout.lock())
}

fn collect_entries(sel: &MetaSelection, reverse: bool, limit: usize) -> io::Result<Vec<Meta>> {
    let mut items = store::list()?
        .into_iter()
        .filter(|m| matches_meta(&m.attrs, sel))
        .collect::<Vec<_>>();
    if reverse {
        items.reverse();
    }
    if limit > 0 && items.len() > limit {
        items.truncate(limit);
    }
    Ok(items)
}

fn ls_command(mut args: LsArgs) -> io::Result<()> {
    let color = color_enabled(&args.color)?;
    if args.long {
        args.date.get_or_insert("ls".into());
        args.size.get_or_insert("human".into());
        args.name = true;
    }
    if let Some(mode) = args.date.as_deref() {
        args.date = Some(normalize_date_mode(mode)?.to_string());
    }
    let meta_sel = parse_meta_selection(&args.attr, args.attrs)?;
    let items = collect_entries(&meta_sel, args.reverse, args.number)?;
    let ls_date_mode = args.date.as_deref().unwrap_or("ls");
    if args.date.is_none()
        && args.size.is_none()
        && !args.name
        && !args.preview
        && !meta_sel.show_all
        && meta_sel.tags.is_empty()
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
        meta_vals: Vec<String>,
        meta_inline: String,
        preview: String,
    }

    // When auto-computing preview width, build entries with empty previews first;
    // effective_chars will be derived from the column measurement loop below.
    let initial_chars = if args.preview && args.chars == 80 { 0 } else { args.chars };
    let decorated = decorate_entries(&items, &args.id, ls_date_mode, initial_chars, &meta_sel);
    let mut rows = Vec::with_capacity(decorated.len());
    for row in decorated {
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
            meta_vals,
            meta_inline,
            preview: if args.preview { preview } else { String::new() },
        });
    }

    let mut max_id = 0usize;
    let mut max_size = 0usize;
    let mut max_date = 0usize;
    let mut max_name = 0usize;
    let mut max_inline_meta = 0usize;
    let mut meta_widths = vec![0usize; meta_sel.tags.len()];
    for row in &rows {
        max_id = max_id.max(row.id.chars().count());
        max_size = max_size.max(row.size.chars().count());
        max_date = max_date.max(row.date.chars().count());
        max_name = max_name.max(row.name.chars().count());
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

fn log_command(args: LogArgs) -> io::Result<()> {
    let color = color_enabled(&args.color)?;
    let date_mode = normalize_date_mode(&args.date)?;
    let meta_sel = parse_meta_selection(&args.attr, args.attrs)?;
    let items = collect_entries(&meta_sel, args.reverse, args.number)?;
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

fn attr_command(args: AttrArgs) -> io::Result<()> {
    let id = store::resolve(&args.reference)?;
    if !args.unset.is_empty() {
        if !args.items.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot combine --unset with reads or writes",
            ));
        }
        for key in &args.unset {
            if !is_writable_attr_key(key) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("only user-defined attributes are writable: {key:?}"),
                ));
            }
        }
        return store::unset_attrs(&id, &args.unset);
    }

    let has_writes = args.items.iter().any(|item| item.contains('='));
    let has_reads = args.items.iter().any(|item| !item.contains('='));
    if has_writes && has_reads {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot mix attribute reads and writes",
        ));
    }

    if has_writes {
        let mut attrs = BTreeMap::new();
        for pair in &args.items {
            let Some((k, v)) = pair.split_once('=') else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "expected key=value",
                ));
            };
            if !is_writable_attr_key(k) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("only user-defined attributes are writable: {k:?}"),
                ));
            }
            attrs.insert(k.to_string(), v.to_string());
        }
        return store::set_attrs(&id, &attrs);
    }

    let meta = store::get_meta(&id)?;
    if args.json {
        let value = if args.items.is_empty() {
            meta.to_json_value(args.preview)
        } else {
            let mut map = serde_json::Map::new();
            for key in &args.items {
                let Some(value) = attr_value(&meta, key, args.preview) else {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("attribute not found: {key}"),
                    ));
                };
                map.insert(key.clone(), serde_json::Value::String(value));
            }
            serde_json::Value::Object(map)
        };
        serde_json::to_writer_pretty(io::stdout(), &value).map_err(io::Error::other)?;
        println!();
        return Ok(());
    }

    if args.items.len() == 1 {
        let key = &args.items[0];
        if let Some(value) = attr_value(&meta, key, args.preview) {
            println!("{}", escape_attr_output(&value));
            return Ok(());
        }
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "attribute not found",
        ));
    }

    if !args.items.is_empty() {
        for key in &args.items {
            let Some(value) = attr_value(&meta, key, args.preview) else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("attribute not found: {key}"),
                ));
            };
            println!("{}{}{}", key, args.separator, escape_attr_output(&value));
        }
        return Ok(());
    }

    println!("id{}{}", args.separator, meta.display_id());
    println!("ts{}{}", args.separator, meta.ts);
    println!("size{}{}", args.separator, meta.size);
    for (k, v) in &meta.attrs {
        println!("{}{}{}", k, args.separator, escape_attr_output(v));
    }
    if args.preview && !meta.preview.is_empty() {
        println!("preview{}{}", args.separator, escape_attr_output(&meta.preview));
    }
    Ok(())
}

fn attrs_command(args: AttrsArgs) -> io::Result<()> {
    for (key, count) in store::all_attr_keys()? {
        if args.count {
            println!("{key}\t{count}");
        } else {
            println!("{key}");
        }
    }
    Ok(())
}

fn path_base_dir(dir: bool, attr: bool) -> io::Result<PathBuf> {
    if dir {
        store::base_dir()
    } else if attr {
        store::attr_dir()
    } else {
        store::data_dir()
    }
}

fn path_command(args: PathArgs) -> io::Result<()> {
    let dir = args.dir;
    let attr = args.attr;
    let mut refs = args.refs;

    if refs.is_empty() {
        let stdin = io::stdin();
        if stdin.is_terminal() {
            let path = path_base_dir(dir, attr)?;
            println!("{}", path.canonicalize().unwrap_or(path).display());
            return Ok(());
        }
        for line in stdin.lock().lines() {
            let line = line?;
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                refs.push(trimmed.to_string());
            }
        }
        if refs.is_empty() {
            let path = path_base_dir(dir, attr)?;
            println!("{}", path.canonicalize().unwrap_or(path).display());
            return Ok(());
        }
    }

    for reference in refs {
        let id = store::resolve(&reference)?;
        let path = if dir {
            if attr {
                store::attr_dir()?
            } else {
                store::data_dir()?
            }
        } else if attr {
            store::entry_attr_path(&id)?
        } else {
            store::entry_data_path(&id)?
        };
        println!("{}", path.canonicalize().unwrap_or(path).display());
    }
    Ok(())
}

fn rm_command(args: RmArgs) -> io::Result<()> {
    if !args.attr.is_empty() {
        if !args.refs.is_empty() || args.before.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "rm accepts either <ref>..., --before, or --attr",
            ));
        }
        let filters = parse_rm_attr_filters(&args.attr)?;
        let matches: Vec<Meta> = store::list()?
            .into_iter()
            .filter(|meta| matches_rm_attr_filters(&meta.attrs, &filters))
            .collect();
        if matches.is_empty() {
            return Ok(());
        }
        if !args.force && !confirm_rm_entries("matching attributes", &matches)? {
            return Ok(());
        }
        for meta in matches {
            store::remove(&meta.id)?;
        }
        return Ok(());
    }

    if let Some(before_ref) = args.before.as_deref() {
        if !args.refs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "rm accepts either <ref>... or --before",
            ));
        }
        let id = store::resolve(before_ref)?;
        let ids = store::older_than_ids(&id)?;
        if ids.is_empty() {
            return Ok(());
        }
        if !args.force && !confirm_rm_before(before_ref, ids.len())? {
            return Ok(());
        }
        for id in ids {
            store::remove(&id)?;
        }
        return Ok(());
    }

    if args.refs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "rm requires at least one ref",
        ));
    }

    let mut seen = BTreeMap::new();
    for reference in &args.refs {
        let id = store::resolve(reference)?;
        seen.entry(id.clone()).or_insert_with(|| reference.clone());
    }
    let ids: Vec<String> = seen.into_keys().collect();
    if ids.len() == 1 {
        return store::remove(&ids[0]);
    }

    let mut entries = Vec::new();
    for id in &ids {
        entries.push(store::get_meta(id)?);
    }
    if !args.force && !confirm_rm_entries("matching refs", &entries)? {
        return Ok(());
    }
    for id in ids {
        store::remove(&id)?;
    }
    Ok(())
}

fn pop_command() -> io::Result<()> {
    let newest = store::newest()?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    store::cat_to_writer(&newest.id, &mut out)?;
    store::remove(&newest.id)
}

fn completion_command(args: CompletionArgs) -> io::Result<()> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(Shell::from(args.shell), &mut cmd, name, &mut io::stdout());
    Ok(())
}

fn confirm_rm_before(reference: &str, count: usize) -> io::Result<bool> {
    if count == 1 {
        eprint!("Remove 1 entry older than {}? [y/N] ", reference);
    } else {
        eprint!("Remove {} entries older than {}? [y/N] ", count, reference);
    }
    io::stderr().flush()?;
    let mut reply = String::new();
    io::stdin().read_line(&mut reply)?;
    let reply = reply.trim().to_ascii_lowercase();
    Ok(reply == "y" || reply == "yes")
}

#[derive(Clone, Debug)]
struct RmAttrFilter {
    key: String,
    value: Option<String>,
}

fn parse_rm_attr_filters(values: &[String]) -> io::Result<Vec<RmAttrFilter>> {
    let mut filters = Vec::new();
    for value in values {
        if value.trim().is_empty() || value.contains(',') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--attr accepts name or name=value and is repeatable",
            ));
        }
        if let Some((key, attr_value)) = value.split_once('=') {
            filters.push(RmAttrFilter {
                key: key.to_string(),
                value: Some(attr_value.to_string()),
            });
        } else {
            filters.push(RmAttrFilter {
                key: value.to_string(),
                value: None,
            });
        }
    }
    Ok(filters)
}

fn matches_rm_attr_filters(attrs: &BTreeMap<String, String>, filters: &[RmAttrFilter]) -> bool {
    filters.iter().all(|filter| match &filter.value {
        Some(value) => attrs.get(&filter.key) == Some(value),
        None => attrs.contains_key(&filter.key),
    })
}

fn confirm_rm_entries(reason: &str, entries: &[Meta]) -> io::Result<bool> {
    eprintln!(
        "Remove {} entr{} {}:",
        entries.len(),
        if entries.len() == 1 { "y" } else { "ies" },
        reason
    );
    for entry in entries {
        if let Some(name) = entry.attrs.get("filename") {
            eprintln!("  {}  {}  {}", entry.short_id(), entry.ts, name);
        } else {
            eprintln!("  {}  {}", entry.short_id(), entry.ts);
        }
    }
    eprint!("Continue? [y/N] ");
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let answer = input.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}

