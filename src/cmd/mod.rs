use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use std::io::{self, IsTerminal};

use crate::store::{matches_meta, MetaSelection};
use crate::store;
use crate::store::Meta;

mod attr;
mod log;
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
    #[command(alias = "list")]
    #[command(about = "Show detailed entry history")]
    Log(log::LogArgs),
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
    #[command(about = "Generate shell completion scripts")]
    Completion(CompletionArgs),
}

#[derive(Args, Debug, Clone, Default)]
struct CatArgs {
    #[arg(help = "Entry reference: id, n, or @n")]
    reference: Option<String>,
}

#[derive(Args, Debug, Clone)]
struct AttrsArgs {
    #[arg(long, help = "Include entry count")]
    count: bool,
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
        Some(Command::Log(args)) => log::log_command(args),
        Some(Command::Attr(args)) => attr::attr_command(args),
        Some(Command::Attrs(args)) => attrs_command(args),
        Some(Command::Path(args)) => path::path_command(args),
        Some(Command::Rm(args)) => rm::rm_command(args),
        Some(Command::Pop) => pop_command(),
        Some(Command::Completion(args)) => completion_command(args),
        None => {
            if smart_mode_uses_tee(&cli.push) {
                push::tee_command(push::TeeArgs {
                    attr: cli.push.attr,
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

fn cat_command(args: CatArgs) -> io::Result<()> {
    let id = if let Some(reference) = args.reference {
        store::resolve(&reference)?
    } else {
        store::newest()?.id
    };
    let stdout = io::stdout();
    store::cat_to_writer(&id, stdout.lock())
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

fn completion_command(args: CompletionArgs) -> io::Result<()> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(Shell::from(args.shell), &mut cmd, name, &mut io::stdout());
    Ok(())
}
