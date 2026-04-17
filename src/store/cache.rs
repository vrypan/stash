use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::time::UNIX_EPOCH;

use super::Meta;

const LIST_CACHE_VERSION: u32 = 3;

#[derive(
    Debug, SerdeSerialize, SerdeDeserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
)]
struct ListCacheFile {
    version: u32,
    data_mtime: String,
    attr_mtime: String,
    items: Vec<Meta>,
    attr_keys: BTreeMap<String, usize>,
}

fn dir_mtime_key(path: &Path) -> io::Result<String> {
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

    // Check mtimes before deserializing the full cache to fail fast
    let current_data = dir_mtime_key(&super::data_dir()?)?;
    let current_attr = dir_mtime_key(&super::attr_dir()?)?;

    let data = fs::read(path)?;
    let cache =
        rkyv::from_bytes::<ListCacheFile, rkyv::rancor::Error>(&data).map_err(io::Error::other)?;
    if cache.version != LIST_CACHE_VERSION {
        return Err(io::Error::other("stale list cache"));
    }
    if cache.data_mtime != current_data || cache.attr_mtime != current_attr {
        return Err(io::Error::other("stale list cache"));
    }
    Ok(cache)
}

pub(super) fn write_list_cache(items: Vec<Meta>) -> io::Result<Vec<Meta>> {
    super::init()?;
    let path = super::list_cache_path()?;
    let attr_keys = build_attr_key_index(&items);
    let cache = ListCacheFile {
        version: LIST_CACHE_VERSION,
        data_mtime: dir_mtime_key(&super::data_dir()?)?,
        attr_mtime: dir_mtime_key(&super::attr_dir()?)?,
        items,
        attr_keys,
    };
    let encoded = rkyv::to_bytes::<rkyv::rancor::Error>(&cache).map_err(io::Error::other)?;
    fs::write(path, encoded)?;
    Ok(cache.items)
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

pub(super) fn attr_key_index_vec(items: &[Meta]) -> Vec<(String, usize)> {
    build_attr_key_index(items).into_iter().collect()
}
