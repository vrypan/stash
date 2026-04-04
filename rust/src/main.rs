mod json;
mod preview;
mod store;

use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufRead, IsTerminal, Write};
use std::mem::MaybeUninit;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use store::Meta;

#[derive(Parser, Debug)]
#[command(
    name = "stash",
    version,
    about = "A local store for pipeline output and ad hoc file snapshots",
    long_about = None,
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
    #[command(about = "Stash stdin or a file")]
    Push(PushArgs),
    #[command(about = "Stream stdin to stdout and stash it at the same time")]
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
    #[command(about = "Print stash paths")]
    Path(PathArgs),
    #[command(about = "Remove entries")]
    Rm(RmArgs),
    #[command(about = "Print the newest entry and remove it")]
    Pop,
    #[command(about = "Print version information")]
    Version,
    #[command(about = "Generate shell completion scripts")]
    Completion(CompletionArgs),
}

#[derive(Args, Debug, Clone, Default)]
struct PushArgs {
    #[arg(short = 'm', long = "meta", value_name = "key=value", action = ArgAction::Append, help = "Metadata key=value (repeatable)")]
    meta: Vec<String>,

    #[arg(help = "Optional file to stash; reads stdin when omitted")]
    file: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
struct TeeArgs {
    #[arg(short = 'm', long = "meta", value_name = "key=value", action = ArgAction::Append, help = "Metadata key=value (repeatable)")]
    meta: Vec<String>,

    #[arg(long, help = "Save a partial entry if the stream is interrupted")]
    partial: bool,
}

#[derive(Args, Debug, Clone, Default)]
struct CatArgs {
    #[arg(help = "Entry reference: id, n, or @n")]
    reference: Option<String>,
}

#[derive(Args, Debug, Clone)]
struct LsArgs {
    #[arg(long, default_value = "short", help = "ID display: short, full, or pos")]
    id: String,

    #[arg(short = 'm', long = "meta", value_name = "tag|@", action = ArgAction::Append, help = "Show metadata tags with @, or filter by tag name (repeatable)")]
    meta: Vec<String>,

    #[arg(short = 'n', long = "number", default_value_t = 0, help = "Limit number of entries shown (0 = all)")]
    number: usize,

    #[arg(short = 'r', long = "reverse", help = "Show oldest first")]
    reverse: bool,

    #[arg(long, default_missing_value = "ls", num_args = 0..=1, help = "Include date column: iso, ago, or ls")]
    date: Option<String>,

    #[arg(long, default_missing_value = "human", num_args = 0..=1, help = "Include size column: human or bytes")]
    size: Option<String>,

    #[arg(long, help = "Include filename or full ULID column")]
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
struct LogArgs {
    #[arg(long, default_value = "full", help = "ID display: short, full, or pos")]
    id: String,

    #[arg(short = 'm', long = "meta", value_name = "tag|@", action = ArgAction::Append, help = "Show metadata tags with @, or filter by tag name (repeatable)")]
    meta: Vec<String>,

    #[arg(short = 'n', long = "number", default_value_t = 0, help = "Limit number of entries shown (0 = all)")]
    number: usize,

    #[arg(short = 'r', long = "reverse", help = "Show oldest first")]
    reverse: bool,

    #[arg(long, help = "Output verbose entry history as JSON")]
    json: bool,

    #[arg(long, help = "Template for custom log output")]
    format: Option<String>,

    #[arg(long, default_value = "iso", help = "Date format: iso or ago")]
    date: String,

    #[arg(long, default_value_t = 80, help = "Preview character limit")]
    chars: usize,

    #[arg(long, help = "Disable color output")]
    no_color: bool,

    #[arg(long, default_value = "true", help = "Color output: true or false")]
    color: String,
}

#[derive(Args, Debug, Clone)]
struct AttrArgs {
    #[arg(help = "Entry reference: id, n, or @n")]
    reference: String,

    #[arg(help = "Attribute key, or set/unset command")]
    key_or_cmd: Option<String>,

    #[arg(help = "Extra arguments for set/unset")]
    rest: Vec<String>,

    #[arg(long, default_value = "\t", help = "Separator used between key and value")]
    separator: String,

    #[arg(long, help = "Output attributes as JSON")]
    json: bool,

    #[arg(short = 'p', long = "preview", help = "Include preview pseudo-property when available")]
    preview: bool,
}

#[derive(Args, Debug, Clone)]
struct PathArgs {
    #[arg(short = 'd', long = "dir", help = "Print entry directories instead of data file paths")]
    dir: bool,

    #[arg(help = "Entry refs read from arguments or stdin")]
    refs: Vec<String>,
}

#[derive(Args, Debug, Clone)]
struct RmArgs {
    #[arg(help = "Entry reference to remove")]
    reference: Option<String>,

    #[arg(long, help = "Remove entries older than the referenced entry")]
    before: Option<String>,

    #[arg(short = 'f', long = "force", help = "Do not prompt for confirmation")]
    force: bool,
}

#[derive(Args, Debug, Clone)]
struct CompletionArgs {
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

fn main() {
    if let Err(err) = run() {
        if err
            .get_ref()
            .and_then(|e| e.downcast_ref::<store::PartialSavedError>())
            .is_some()
        {
            eprintln!("error: {err}");
            std::process::exit(4);
        }
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Push(args)) => push_command(args),
        Some(Command::Tee(args)) => tee_command(args),
        Some(Command::Cat(args)) => cat_command(args),
        Some(Command::Ls(args)) => ls_command(args),
        Some(Command::Log(args)) => log_command(args),
        Some(Command::Attr(args)) => attr_command(args),
        Some(Command::Path(args)) => path_command(args),
        Some(Command::Rm(args)) => rm_command(args),
        Some(Command::Pop) => pop_command(),
        Some(Command::Version) => {
            println!("stash-rs {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(Command::Completion(args)) => completion_command(args),
        None => push_command(cli.push),
    }
}

fn parse_meta_flags(values: &[String]) -> io::Result<BTreeMap<String, String>> {
    let mut attrs = BTreeMap::new();
    for value in values {
        let Some((k, v)) = value.split_once('=') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "metadata must be key=value",
            ));
        };
        attrs.insert(k.to_string(), v.to_string());
    }
    Ok(attrs)
}

#[derive(Clone, Debug, Default)]
struct MetaSelection {
    show_all: bool,
    tags: Vec<String>,
}

fn parse_meta_selection(values: &[String]) -> io::Result<MetaSelection> {
    let mut out = MetaSelection::default();
    for value in values {
        if value == "@" {
            out.show_all = true;
        } else if value.contains(',') || value.contains('=') || value.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--meta accepts tag names or @ and is repeatable",
            ));
        } else if !out.tags.contains(value) {
            out.tags.push(value.clone());
        }
    }
    Ok(out)
}

fn matches_meta(attrs: &BTreeMap<String, String>, sel: &MetaSelection) -> bool {
    if sel.show_all || sel.tags.is_empty() {
        return true;
    }
    sel.tags.iter().any(|tag| attrs.contains_key(tag))
}

fn push_command(args: PushArgs) -> io::Result<()> {
    let mut attrs = parse_meta_flags(&args.meta)?;
    let id = if let Some(path) = args.file {
        let mut file = File::open(&path)?;
        store::add_filename_attr(&path, &mut attrs);
        store::push_from_reader(&mut file, attrs)?
    } else {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        store::push_from_reader(&mut input, attrs)?
    };
    println!("{id}");
    Ok(())
}

fn tee_command(args: TeeArgs) -> io::Result<()> {
    let attrs = parse_meta_flags(&args.meta)?;
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match store::tee_from_reader_partial(&mut input, &mut out, attrs, args.partial) {
        Ok(id) => {
            eprintln!("{id}");
            Ok(())
        }
        Err(err) => {
            if let Some(partial_err) = err
                .get_ref()
                .and_then(|e| e.downcast_ref::<store::PartialSavedError>())
            {
                eprintln!(
                    "partial stash saved: {}",
                    partial_err.id.to_ascii_lowercase()
                );
            }
            Err(err)
        }
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
    let meta_sel = parse_meta_selection(&args.meta)?;
    let items = collect_entries(&meta_sel, args.reverse, args.number)?;
    let effective_chars = if args.preview && args.chars == 80 {
        auto_ls_preview_chars(
            &items,
            &args.id,
            args.date.as_deref(),
            args.size.as_deref(),
            args.name,
            &meta_sel,
        )
    } else {
        args.chars
    };

    if args.date.is_none()
        && args.size.is_none()
        && !args.name
        && !args.preview
        && !meta_sel.show_all
        && meta_sel.tags.is_empty()
    {
        for (idx, item) in items.iter().enumerate() {
            println!("{}", color_id(&display_id(item, idx, &args.id), color));
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

    let mut rows = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        let id = display_id(item, idx, &args.id);
        let size = args
            .size
            .as_deref()
            .map(|mode| format_size(item.size, mode))
            .unwrap_or_default();
        let date = args
            .date
            .as_deref()
            .map(|mode| format_date(&item.ts, mode))
            .unwrap_or_default();
        let name = if args.name {
            item.attrs
                .get("filename")
                .cloned()
                .unwrap_or_else(|| item.display_id())
        } else {
            String::new()
        };
        let meta_vals = if !meta_sel.tags.is_empty() {
            meta_sel
                .tags
                .iter()
                .map(|tag| item.attrs.get(tag).cloned().unwrap_or_else(|| " ".into()))
                .collect()
        } else {
            Vec::new()
        };
        let meta_inline = if meta_sel.show_all && !item.attrs.is_empty() {
            item.attrs.values().cloned().collect::<Vec<_>>().join("  ")
        } else {
            String::new()
        };
        let preview = if args.preview && !item.preview.is_empty() {
            preview_snippet(&item.preview, effective_chars)
        } else {
            String::new()
        };
        rows.push(LsRow {
            id,
            size,
            date,
            name,
            meta_vals,
            meta_inline,
            preview,
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

    for row in rows {
        let mut parts = vec![color_id(&pad_right(&row.id, max_id), color)];
        if !row.size.is_empty() {
            parts.push(pad_left(&row.size, max_size));
        }
        if !row.date.is_empty() {
            parts.push(pad_left(&row.date, max_date));
        }
        if !row.name.is_empty() {
            let padded = pad_right(&row.name, max_name);
            if row.name == row.id {
                parts.push(padded);
            } else {
                parts.push(color_name(&padded, color));
            }
        }
        for (idx, value) in row.meta_vals.iter().enumerate() {
            parts.push(color_attrs(&pad_right(value, meta_widths[idx]), color));
        }
        if !row.meta_inline.is_empty() {
            parts.push(color_attrs(&pad_right(&row.meta_inline, max_inline_meta), color));
        }
        if !row.preview.is_empty() {
            parts.push(row.preview);
        }
        let mut line = parts.join("  ");
        if let Some(width) = terminal_width() {
            line = trim_ansi_to_width(&line, width);
        }
        println!("{line}");
    }
    Ok(())
}

fn log_command(args: LogArgs) -> io::Result<()> {
    let color = if args.no_color { false } else { color_enabled(&args.color)? };
    let date_mode = normalize_date_mode(&args.date)?;
    let meta_sel = parse_meta_selection(&args.meta)?;
    let items = collect_entries(&meta_sel, args.reverse, args.number)?;
    if args.json {
        print_log_json(&items, date_mode, args.chars);
        return Ok(());
    }
    if let Some(format) = args.format.as_deref() {
        return print_log_template(&items, date_mode, args.chars, format);
    }

    for (idx, item) in items.iter().enumerate() {
        if idx > 0 {
            println!();
        }
        let id = display_id(item, idx, &args.id);
        println!(
            "entry {} ({})",
            color_id(&id, color),
            store::human_size(item.size)
        );
        println!(
            "{}{}",
            color_label("Date: ", color),
            format_date(&item.ts, date_mode)
        );
        if let Some(meta_line) = format_log_meta(&item.attrs, &meta_sel) {
            println!(
                "{}{}",
                color_label("Meta: ", color),
                color_attrs(&meta_line, color)
            );
        }
        if !item.preview.is_empty() {
            println!();
            println!("    {}", preview_snippet(&item.preview, args.chars));
        }
    }
    Ok(())
}

fn attr_command(args: AttrArgs) -> io::Result<()> {
    let id = store::resolve(&args.reference)?;
    if let Some(cmd) = args.key_or_cmd.as_deref() {
        match cmd {
            "set" => {
                let mut attrs = BTreeMap::new();
                for pair in &args.rest {
                    let Some((k, v)) = pair.split_once('=') else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "expected key=value",
                        ));
                    };
                    if !is_writable_attr_key(k) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("only metadata keys are writable: {k:?}"),
                        ));
                    }
                    attrs.insert(k.to_string(), v.to_string());
                }
                return store::set_attrs(&id, &attrs);
            }
            "unset" => {
                for key in &args.rest {
                    if !is_writable_attr_key(key) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("only metadata keys are writable: {key:?}"),
                        ));
                    }
                }
                return store::unset_attrs(&id, &args.rest);
            }
            _ if args.rest.is_empty() => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "attr get accepts exactly one key",
                ));
            }
        }
    }

    let meta = store::get_meta(&id)?;
    if args.json {
        print!("{}", meta.to_json_pretty());
        return Ok(());
    }

    if let Some(key) = args.key_or_cmd.as_deref() {
        if let Some(value) = attr_value(&meta, key, args.preview) {
            println!("{value}");
            return Ok(());
        }
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "attribute not found",
        ));
    }

    println!("id{}{}", args.separator, meta.display_id());
    println!("ts{}{}", args.separator, meta.ts);
    println!("size{}{}", args.separator, meta.size);
    for (k, v) in &meta.attrs {
        println!("meta.{}{}{}", k, args.separator, v);
    }
    if args.preview && !meta.preview.is_empty() {
        println!("preview{}{}", args.separator, meta.preview);
    }
    Ok(())
}

fn path_command(args: PathArgs) -> io::Result<()> {
    let mut refs = args.refs;

    if refs.is_empty() {
        let stdin = io::stdin();
        if stdin.is_terminal() {
            let path = if args.dir {
                store::base_dir()?
            } else {
                store::entries_dir()?
            };
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
            let path = if args.dir {
                store::base_dir()?
            } else {
                store::entries_dir()?
            };
            println!("{}", path.canonicalize().unwrap_or(path).display());
            return Ok(());
        }
    }

    for reference in refs {
        let id = store::resolve(&reference)?;
        let path = if args.dir {
            store::entry_dir(&id)?
        } else {
            store::entry_data_path(&id)?
        };
        println!("{}", path.canonicalize().unwrap_or(path).display());
    }
    Ok(())
}

fn rm_command(args: RmArgs) -> io::Result<()> {
    if let Some(before_ref) = args.before.as_deref() {
        if args.reference.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "rm accepts either <id> or --before",
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

    let Some(reference) = args.reference.as_deref() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "rm requires exactly one ref",
        ));
    };
    let id = store::resolve(reference)?;
    store::remove(&id)
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
    generate(
        Shell::from(args.shell),
        &mut cmd,
        name,
        &mut io::stdout(),
    );
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

fn display_id(item: &Meta, idx: usize, mode: &str) -> String {
    match mode {
        "full" => item.display_id(),
        "pos" => (idx + 1).to_string(),
        _ => item.short_id(),
    }
}

fn color_enabled(value: &str) -> io::Result<bool> {
    match value {
        "true" => Ok(io::stdout().is_terminal()),
        "false" => Ok(false),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--color must be true or false",
        )),
    }
}

fn paint(s: &str, code: &str, enabled: bool) -> String {
    if enabled && !s.is_empty() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

fn color_id(s: &str, enabled: bool) -> String {
    paint(s, "1;33", enabled)
}

fn color_label(s: &str, enabled: bool) -> String {
    paint(s, "1", enabled)
}

fn color_attrs(s: &str, enabled: bool) -> String {
    paint(s, "35", enabled)
}

fn color_name(s: &str, enabled: bool) -> String {
    paint(s, "1;36", enabled)
}

fn pad_right(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - len))
    }
}

fn pad_left(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.to_string()
    } else {
        format!("{}{}", " ".repeat(width - len), s)
    }
}

fn terminal_width() -> Option<usize> {
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

fn trim_ansi_to_width(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let bytes = s.as_bytes();
    let mut out = String::new();
    let mut visible = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let mut j = i + 2;
            while j < bytes.len() {
                let c = bytes[j];
                if (0x40..=0x7e).contains(&c) {
                    j += 1;
                    break;
                }
                j += 1;
            }
            out.push_str(&s[i..j]);
            i = j;
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        if visible >= width {
            break;
        }
        out.push(ch);
        visible += 1;
        i += ch.len_utf8();
    }
    if visible >= width {
        out.push_str("\x1b[0m");
    }
    out
}

fn auto_ls_preview_chars(
    items: &[Meta],
    id_mode: &str,
    date_mode: Option<&str>,
    size_mode: Option<&str>,
    show_name: bool,
    meta_sel: &MetaSelection,
) -> usize {
    let Some(width) = terminal_width() else {
        return 80;
    };

    let mut max_id = 0usize;
    let mut max_size = 0usize;
    let mut max_date = 0usize;
    let mut max_name = 0usize;
    let mut max_inline_meta = 0usize;
    let mut meta_widths = vec![0usize; meta_sel.tags.len()];

    for (idx, item) in items.iter().enumerate() {
        max_id = max_id.max(display_id(item, idx, id_mode).len());
        if let Some(mode) = size_mode {
            max_size = max_size.max(format_size(item.size, mode).len());
        }
        if let Some(mode) = date_mode {
            max_date = max_date.max(format_date(&item.ts, mode).len());
        }
        if show_name {
            let name = item
                .attrs
                .get("filename")
                .cloned()
                .unwrap_or_else(|| item.display_id());
            max_name = max_name.max(name.len());
        }
        if !meta_sel.tags.is_empty() {
            for (i, tag) in meta_sel.tags.iter().enumerate() {
                let value = item.attrs.get(tag).cloned().unwrap_or_else(|| " ".into());
                meta_widths[i] = meta_widths[i].max(value.len());
            }
        } else if meta_sel.show_all && !item.attrs.is_empty() {
            let joined = item.attrs.values().cloned().collect::<Vec<_>>().join("  ");
            max_inline_meta = max_inline_meta.max(joined.len());
        }
    }

    let mut fixed = max_id;
    if size_mode.is_some() {
        fixed += 2 + max_size;
    }
    if date_mode.is_some() {
        fixed += 2 + max_date;
    }
    if show_name {
        fixed += 2 + max_name;
    }
    for meta_width in meta_widths {
        fixed += 2 + meta_width;
    }
    if max_inline_meta > 0 {
        fixed += 2 + max_inline_meta;
    }
    let chars = width.saturating_sub(fixed + 2);
    chars.max(20)
}

fn format_size(size: i64, mode: &str) -> String {
    if mode == "bytes" {
        size.to_string()
    } else {
        store::human_size(size)
    }
}

fn normalize_date_mode(mode: &str) -> io::Result<&str> {
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

fn format_date(ts: &str, mode: &str) -> String {
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
    let (now_year, _, _, _, _, _) = unix_to_utc(now_secs);
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

fn format_log_meta(attrs: &BTreeMap<String, String>, sel: &MetaSelection) -> Option<String> {
    let mut parts = Vec::new();
    if sel.show_all {
        for (k, v) in attrs {
            parts.push(format!("{k}={v}"));
        }
    } else {
        for tag in &sel.tags {
            if let Some(v) = attrs.get(tag) {
                parts.push(format!("{tag}={v}"));
            }
        }
    }
    (!parts.is_empty()).then(|| parts.join("  "))
}

fn print_log_json(items: &[Meta], date_mode: &str, chars: usize) {
    println!("[");
    for (idx, item) in items.iter().enumerate() {
        if idx > 0 {
            println!(",");
        }
        print!("  {{");
        print!("\"id\":\"{}\",", escape_json(&item.display_id()));
        print!("\"short_id\":\"{}\",", escape_json(&item.short_id()));
        print!("\"stack_ref\":\"{}\",", idx + 1);
        print!("\"ts\":\"{}\",", escape_json(&item.ts));
        print!("\"date\":\"{}\",", escape_json(&format_date(&item.ts, date_mode)));
        print!("\"size\":{},", item.size);
        print!(
            "\"size_human\":\"{}\"",
            escape_json(&store::human_size(item.size))
        );
        if !item.attrs.is_empty() {
            print!(",\"meta\":{{");
            let mut first = true;
            for (k, v) in &item.attrs {
                if !first {
                    print!(",");
                }
                first = false;
                print!("\"{}\":\"{}\"", escape_json(k), escape_json(v));
            }
            print!("}}");
        }
        let preview = preview_snippet(&item.preview, chars);
        if !preview.is_empty() {
            print!(",\"preview\":[\"{}\"]", escape_json(&preview));
        }
        print!("}}");
    }
    println!();
    println!("]");
}

fn preview_snippet(preview: &str, chars: usize) -> String {
    if chars == 0 {
        return String::new();
    }
    let total = preview.chars().count();
    let mut out = String::new();
    for (idx, ch) in preview.chars().enumerate() {
        if idx >= chars {
            break;
        }
        out.push(ch);
    }
    if total > chars && chars > 3 {
        out.push_str("...");
    }
    out
}

fn is_writable_attr_key(key: &str) -> bool {
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

fn log_template_value(item: &Meta, idx: usize, date_mode: &str, chars: usize, key: &str) -> Option<String> {
    match key {
        "ID" => Some(item.display_id()),
        "ShortID" => Some(item.short_id()),
        "StackRef" => Some((idx + 1).to_string()),
        "TS" => Some(item.ts.clone()),
        "Date" => Some(format_date(&item.ts, date_mode)),
        "Size" => Some(item.size.to_string()),
        "SizeHuman" => Some(store::human_size(item.size)),
        "Preview" => {
            let preview = preview_snippet(&item.preview, chars);
            (!preview.is_empty()).then_some(preview)
        }
        _ => None,
    }
}

fn render_log_template(
    item: &Meta,
    idx: usize,
    date_mode: &str,
    chars: usize,
    format: &str,
) -> String {
    let mut out = String::new();
    let mut rest = format;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let expr = after[..end].trim();
        if let Some(key) = expr.strip_prefix('.') {
            if let Some(value) = log_template_value(item, idx, date_mode, chars, key) {
                out.push_str(&value);
            }
        } else if let Some(key) = parse_index_meta_expr(expr) {
            if let Some(value) = item.attrs.get(key) {
                out.push_str(value);
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn parse_index_meta_expr(expr: &str) -> Option<&str> {
    let rest = expr.strip_prefix("index .Meta ")?;
    let rest = rest.trim();
    if !rest.starts_with('"') || !rest.ends_with('"') || rest.len() < 2 {
        return None;
    }
    Some(&rest[1..rest.len() - 1])
}

fn print_log_template(items: &[Meta], date_mode: &str, chars: usize, format: &str) -> io::Result<()> {
    for (idx, item) in items.iter().enumerate() {
        println!("{}", render_log_template(item, idx, date_mode, chars, format));
    }
    Ok(())
}

fn escape_json(s: &str) -> String {
    crate::json::escape_string(s)
}

fn parse_ts_seconds(ts: &str) -> Option<i64> {
    let (year, month, day, hour, minute, second) = parse_ts_parts(ts)?;
    Some(civil_to_days(year, month, day) * 86_400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64)
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

fn unix_to_utc(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = (rem / 3600) as u32;
    let min = ((rem % 3600) / 60) as u32;
    let sec = (rem % 60) as u32;
    let (year, month, day) = civil_from_days(days);
    (year, month, day, hour, min, sec)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u32, d as u32)
}

fn attr_value(meta: &Meta, key: &str, with_preview: bool) -> Option<String> {
    match key {
        "id" => Some(meta.display_id()),
        "ts" => Some(meta.ts.clone()),
        "size" => Some(meta.size.to_string()),
        "preview" if with_preview || !meta.preview.is_empty() => {
            (!meta.preview.is_empty()).then(|| meta.preview.clone())
        }
        _ => key
            .strip_prefix("meta.")
            .and_then(|k| meta.attrs.get(k).cloned()),
    }
}
