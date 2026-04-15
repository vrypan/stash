use iroh::{EndpointAddr, PublicKey};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub attr_dir: PathBuf,
    pub cache_path: PathBuf,
    pub key_path: PathBuf,
    pub peer_ids: Vec<PublicKey>,
    pub peers: Vec<EndpointAddr>,
    pub allow_peers: Vec<PublicKey>,
    pub show_id: bool,
}
