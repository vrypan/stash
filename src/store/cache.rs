use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use super::Meta;

const LIST_CACHE_VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize)]
struct ListCacheFile {
    version: u32,
    data_mtime: String,
    attr_mtime: String,
    items: Vec<Meta>,
    attr_keys: BTreeMap<String, usize>,
}

fn dir_mtime_key(path: PathBuf) -> io::Result<String> {
    let modified = fs::metadata(path)?.modified()?;
    let duration = modified
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    Ok(format!(
        "{}.{:09}",
        duration.as_secs(),
        duration.subsec_nanos()
    ))
}

pub(super) fn invalidate_list_cache() {
    if let Ok(path) = super::list_cache_path() {
        let _ = fs::remove_file(path);
    }
}

pub(super) fn read_list_cache() -> io::Result<Vec<Meta>> {
    Ok(read_list_cache_file()?.items)
}

pub(super) fn read_attr_keys() -> io::Result<Vec<(String, usize)>> {
    Ok(read_list_cache_file()?.attr_keys.into_iter().collect())
}

fn read_list_cache_file() -> io::Result<ListCacheFile> {
    let path = super::list_cache_path()?;
    let data = fs::read(path)?;
    let cfg = bincode::config::standard();
    let (cache, _): (ListCacheFile, usize) =
        bincode::serde::decode_from_slice(&data, cfg).map_err(io::Error::other)?;
    if cache.version != LIST_CACHE_VERSION {
        return Err(io::Error::other("stale list cache"));
    }
    let current_data = dir_mtime_key(super::data_dir()?)?;
    let current_attr = dir_mtime_key(super::attr_dir()?)?;
    if cache.data_mtime != current_data || cache.attr_mtime != current_attr {
        return Err(io::Error::other("stale list cache"));
    }
    Ok(cache)
}

pub(super) fn write_list_cache(items: &[Meta]) -> io::Result<()> {
    super::init()?;
    let path = super::list_cache_path()?;
    let cache = ListCacheFile {
        version: LIST_CACHE_VERSION,
        data_mtime: dir_mtime_key(super::data_dir()?)?,
        attr_mtime: dir_mtime_key(super::attr_dir()?)?,
        items: items.to_vec(),
        attr_keys: build_attr_key_index(items),
    };
    let cfg = bincode::config::standard();
    let encoded = bincode::serde::encode_to_vec(&cache, cfg).map_err(io::Error::other)?;
    fs::write(path, encoded)
}

fn build_attr_key_index(items: &[Meta]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for item in items {
        for key in item.attrs.keys() {
            *out.entry(key.clone()).or_insert(0) += 1;
        }
    }
    out
}
