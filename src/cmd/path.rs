use clap::Args;
use std::io::{self, BufRead, IsTerminal};
use std::path::PathBuf;

use crate::store;

#[derive(Args, Debug, Clone)]
pub(crate) struct PathArgs {
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

fn path_base_dir(dir: bool, attr: bool) -> io::Result<PathBuf> {
    if dir {
        store::base_dir()
    } else if attr {
        store::attr_dir()
    } else {
        store::data_dir()
    }
}

pub(super) fn path_command(args: PathArgs) -> io::Result<()> {
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
