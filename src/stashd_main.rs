fn main() {
    if let Err(err) = stash::stashd::run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
