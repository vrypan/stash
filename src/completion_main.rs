use clap_complete::Shell;
use std::io;
use std::str::FromStr;

fn main() {
    let shell_name = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: stash-completion <bash|fish|zsh>");
        std::process::exit(1);
    });
    let shell = Shell::from_str(&shell_name).unwrap_or_else(|_| {
        eprintln!("Unknown shell: {shell_name}. Supported: bash, fish, zsh");
        std::process::exit(1);
    });
    stash::cmd::write_completions(shell, &mut io::stdout());
}
