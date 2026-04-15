use crate::snapshot::{HashMapById, SnapshotEntry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

#[derive(
    Debug,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
struct DaemonCacheFile {
    version: u32,
    entries: BTreeMap<String, SnapshotEntry>,
}

pub(crate) fn read_cache_file(path: &Path, version: u32) -> io::Result<Option<HashMapById>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let cache =
        rkyv::from_bytes::<DaemonCacheFile, rkyv::rancor::Error>(&bytes).map_err(io::Error::other)?;
    if cache.version != version {
        return Ok(None);
    }
    Ok(Some(cache.entries.into_iter().collect()))
}

pub(crate) fn write_cache_file(path: &Path, entries: &HashMapById, version: u32) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let cache = DaemonCacheFile {
        version,
        entries: entries.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    };
    let encoded = rkyv::to_bytes::<rkyv::rancor::Error>(&cache).map_err(io::Error::other)?;
    fs::write(path, encoded)
}
