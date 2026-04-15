use clap::Parser;
use iroh::{EndpointAddr, PublicKey};
use stash_core::store;
use std::io;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "stashd", about = "Replicate stash attr snapshots over iroh")]
struct Cli {
    #[arg(
        long = "peer-id",
        value_name = "NODE_ID",
        action = clap::ArgAction::Append,
        help = "Peer node ID to sync with using iroh discovery"
    )]
    peer_ids: Vec<PublicKey>,

    #[arg(
        long = "peer",
        value_name = "ENDPOINT_ADDR_JSON",
        value_parser = parse_endpoint_addr,
        action = clap::ArgAction::Append,
        help = "Static peer EndpointAddr encoded as JSON (advanced)"
    )]
    peers: Vec<EndpointAddr>,

    #[arg(
        long = "allow-peer",
        value_name = "NODE_ID",
        action = clap::ArgAction::Append,
        help = "Additional allowlisted peer node IDs"
    )]
    allow_peers: Vec<PublicKey>,

    #[arg(
        long = "key-file",
        value_name = "PATH",
        help = "Path to the persisted iroh secret key"
    )]
    key_file: Option<PathBuf>,

    #[arg(
        long = "show-id",
        help = "Print the persisted node ID, creating it first if needed, then exit"
    )]
    show_id: bool,
}

pub fn run() -> io::Result<()> {
    let cli = Cli::parse();
    store::init()?;
    let base_dir = store::base_dir()?;
    stash_sync::run(stash_sync::Config {
        attr_dir: store::attr_dir()?,
        cache_path: base_dir.join("cache").join("daemon.cache"),
        key_path: cli
            .key_file
            .unwrap_or_else(|| base_dir.join("cache").join("iroh.key")),
        peer_ids: cli.peer_ids,
        peers: cli.peers,
        allow_peers: cli.allow_peers,
        show_id: cli.show_id,
    })
}

fn parse_endpoint_addr(input: &str) -> Result<EndpointAddr, String> {
    serde_json::from_str(input).map_err(|err| format!("invalid peer JSON: {err}"))
}
