use clap::{ArgAction, Args, Parser, Subcommand};
#[cfg(feature = "completion")]
use clap::{CommandFactory, ValueEnum};
#[cfg(feature = "completion")]
use clap_complete::{Shell, generate};
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use std::io::{self, IsTerminal};

use crate::store;
use crate::store::Meta;
use crate::store::{MetaSelection, matches_meta};

mod attr;
mod ls;
mod path;
mod push;
mod rm;

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
    push: push::PushArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(about = "Store and return the entry key")]
    Push(push::PushArgs),
    #[command(about = "Store and forward to stdout")]
    Tee(push::TeeArgs),
    #[command(about = "Print an entry's raw data to stdout")]
    Cat(CatArgs),
    #[command(about = "List entries")]
    Ls(ls::LsArgs),
    #[command(about = "Show or update entry attributes")]
    Attr(attr::AttrArgs),
    #[command(about = "List attribute keys across the stash")]
    Attrs(AttrsArgs),
    #[command(about = "Print stash paths")]
    Path(path::PathArgs),
    #[command(about = "Remove entries")]
    Rm(rm::RmArgs),
    #[command(about = "Print the newest entry and remove it")]
    Pop,
}

#[derive(Args, Debug, Clone, Default)]
struct CatArgs {
    #[arg(help = "Entry references: id, n, or @n")]
    refs: Vec<String>,

    #[arg(short = 'a', long = "attr", value_name = "name|name=value", action = ArgAction::Append, help = "Print entries where an attribute is set, or equals a value (repeatable)")]
    attr: Vec<String>,

    #[arg(long = "pocket", value_name = "VALUE", action = ArgAction::Append, help = "Alias for --attr pocket=VALUE (repeatable)")]
    pocket: Vec<String>,

    #[arg(short = 'r', long = "reverse", help = "Print entries in reverse order")]
    reverse: bool,
}

#[derive(Args, Debug, Clone)]
struct AttrsArgs {
    #[arg(long, help = "Include entry count")]
    count: bool,
}

#[cfg(feature = "completion")]
#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum CompletionShell {
    Bash,
    Fish,
    Zsh,
}

#[cfg(feature = "completion")]
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
        Some(Command::Push(args)) => push::push_command(args),
        Some(Command::Tee(args)) => push::tee_command(args),
        Some(Command::Cat(args)) => cat_command(args),
        Some(Command::Ls(args)) => ls::ls_command(args),
        Some(Command::Attr(args)) => attr::attr_command(args),
        Some(Command::Attrs(args)) => attrs_command(args),
        Some(Command::Path(args)) => path::path_command(args),
        Some(Command::Rm(args)) => rm::rm_command(args),
        Some(Command::Pop) => pop_command(),
        None => {
            if smart_mode_uses_tee(&cli.push) {
                push::tee_command(push::TeeArgs {
                    attr: cli.push.attr,
                    pocket: cli.push.pocket,
                    print: cli.push.print,
                    save_on_error: true,
                })
            } else {
                push::push_command(cli.push)
            }
        }
    }
}

fn smart_mode_uses_tee(args: &push::PushArgs) -> bool {
    args.file.is_none() && !io::stdin().is_terminal() && !io::stdout().is_terminal()
}

// Shared by ls and log: filter, order, and limit the entry list.
fn collect_entries(sel: &MetaSelection, reverse: bool, limit: usize) -> io::Result<Vec<Meta>> {
    let mut items = store::visible_list()?
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

fn cat_command(args: CatArgs) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();

    let attr_filters = merged_pocket_filters(&args.attr, &args.pocket);

    if !attr_filters.is_empty() {
        if !args.refs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cat accepts either <ref>... or --attr",
            ));
        }
        let filters = parse_attr_match_filters(&attr_filters)?;
        let mut items = store::visible_list()?
            .into_iter()
            .filter(|meta| matches_attr_match_filters(&meta.attrs, &filters))
            .collect::<Vec<_>>();
        if args.reverse {
            items.reverse();
        }
        for item in items {
            store::cat_to_writer(&item.id, &mut out)?;
        }
        return Ok(());
    }

    if args.refs.is_empty() {
        return store::cat_to_writer(&store::newest()?.id, &mut out);
    }

    let refs: Vec<&String> = if args.reverse {
        args.refs.iter().rev().collect()
    } else {
        args.refs.iter().collect()
    };

    for reference in refs {
        let id = store::resolve(reference)?;
        store::cat_to_writer(&id, &mut out)?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct AttrMatchFilter {
    key: String,
    value: Option<String>,
}

fn parse_attr_match_filters(values: &[String]) -> io::Result<Vec<AttrMatchFilter>> {
    let mut filters = Vec::with_capacity(values.len());
    for value in values {
        if value.trim().is_empty() || value.contains(',') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--attr accepts name or name=value and is repeatable",
            ));
        }
        if let Some((key, attr_value)) = value.split_once('=') {
            filters.push(AttrMatchFilter {
                key: key.to_string(),
                value: Some(attr_value.to_string()),
            });
        } else {
            filters.push(AttrMatchFilter {
                key: value.to_string(),
                value: None,
            });
        }
    }
    Ok(filters)
}

fn merged_pocket_filters(attrs: &[String], pockets: &[String]) -> Vec<String> {
    let mut merged = attrs.to_vec();
    merged.extend(
        pockets
            .iter()
            .map(|value| format!("{}={value}", store::POCKET_ATTR)),
    );
    merged
}

fn matches_attr_match_filters(
    attrs: &std::collections::BTreeMap<String, String>,
    filters: &[AttrMatchFilter],
) -> bool {
    filters.iter().all(|filter| match &filter.value {
        Some(value) => attrs.get(&filter.key) == Some(value),
        None => attrs.contains_key(&filter.key),
    })
}

fn pop_command() -> io::Result<()> {
    let newest = store::newest()?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    store::cat_to_writer(&newest.id, &mut out)?;
    store::remove(&newest.id)
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

/// Generate shell completions for `stash` into `writer`.
/// Called by the standalone `stash-completion` binary.
#[cfg(feature = "completion")]
pub fn write_completions(shell: Shell, writer: &mut dyn io::Write) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, writer);
}
