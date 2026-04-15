fn main() {
    if let Err(err) = stash_cli::stashd::run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
