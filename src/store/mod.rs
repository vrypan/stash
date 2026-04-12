use crate::preview::build_preview_data;
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod attr;
mod cache;
mod push;

pub use push::{push_from_reader, tee_from_reader_partial};

pub const SHORT_ID_LEN: usize = 8;
pub const MIN_ID_LEN: usize = 6;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct PartialSavedError {
    pub id: String,
    pub cause: std::io::Error,
    pub signal: Option<i32>,
}

impl std::fmt::Display for PartialSavedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "partial entry saved as \"{}\": {}",
            self.id.to_ascii_lowercase(),
            self.cause
        )
    }
}

impl StdError for PartialSavedError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.cause)
    }
}

pub struct UtcDateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
}

#[derive(
    Clone,
    Debug,
    SerdeSerialize,
    SerdeDeserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Meta {
    pub id: String,
    pub ts: String,
    pub size: i64,
    pub preview: String,
    pub attrs: BTreeMap<String, String>,
}

impl Meta {
    pub fn short_id(&self) -> String {
        self.id[self.id.len().saturating_sub(SHORT_ID_LEN)..].to_ascii_lowercase()
    }

    pub fn display_id(&self) -> String {
        self.id.to_ascii_lowercase()
    }

    pub fn to_json_value(&self, include_preview: bool) -> Value {
        let capacity =
            3 + self.attrs.len() + usize::from(include_preview && !self.preview.is_empty());
        let mut map = serde_json::Map::with_capacity(capacity);
        map.insert("id".into(), Value::String(self.id.clone()));
        map.insert("ts".into(), Value::String(self.ts.clone()));
        map.insert("size".into(), Value::Number(self.size.into()));
        for (k, v) in &self.attrs {
            map.insert(k.clone(), Value::String(v.clone()));
        }
        if include_preview && !self.preview.is_empty() {
            map.insert("preview".into(), Value::String(self.preview.clone()));
        }
        Value::Object(map)
    }
}

// ---------------------------------------------------------------------------
// Entry selection / filtering
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct MetaSelection {
    pub show_all: bool,
    pub display_tags: Vec<String>,
    pub filter_tags: Vec<String>,
}

pub fn parse_meta_selection(values: &[String], show_all: bool) -> io::Result<MetaSelection> {
    let mut out = MetaSelection {
        show_all,
        display_tags: Vec::with_capacity(values.len()),
        filter_tags: Vec::with_capacity(values.len()),
    };
    let mut seen_display = std::collections::HashSet::with_capacity(values.len());
    let mut seen_filter = std::collections::HashSet::with_capacity(values.len());
    for value in values {
        if value.contains(',') || value.contains('=') || value.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--attr accepts name, +name, or ++name and is repeatable",
            ));
        }
        if let Some(key) = value.strip_prefix("++") {
            if key.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--attr filter+display must be ++name",
                ));
            }
            if seen_display.insert(key.to_string()) {
                out.display_tags.push(key.to_string());
            }
            if seen_filter.insert(key.to_string()) {
                out.filter_tags.push(key.to_string());
            }
        } else if let Some(key) = value.strip_prefix('+') {
            if key.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--attr filter must be +name",
                ));
            }
            if seen_filter.insert(key.to_string()) {
                out.filter_tags.push(key.to_string());
            }
        } else if seen_display.insert(value.to_string()) {
            out.display_tags.push(value.clone());
        }
    }
    Ok(out)
}

pub fn matches_meta(attrs: &BTreeMap<String, String>, sel: &MetaSelection) -> bool {
    if sel.filter_tags.is_empty() {
        return true;
    }
    sel.filter_tags.iter().all(|tag| attrs.contains_key(tag))
}

// ---------------------------------------------------------------------------
// Directory / path helpers
// ---------------------------------------------------------------------------

fn cached_base_dir() -> &'static PathBuf {
    use std::sync::OnceLock;
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    BASE.get_or_init(|| match std::env::var("STASH_DIR") {
        Ok(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
        _ => {
            let home = std::env::var("HOME")
                .map(PathBuf::from)
                .expect("HOME not set");
            home.join(".stash")
        }
    })
}

pub fn base_dir() -> io::Result<PathBuf> {
    Ok(cached_base_dir().clone())
}

pub fn data_dir() -> io::Result<PathBuf> {
    Ok(cached_base_dir().join("data"))
}

pub fn attr_dir() -> io::Result<PathBuf> {
    Ok(cached_base_dir().join("attr"))
}

fn cache_dir() -> io::Result<PathBuf> {
    Ok(cached_base_dir().join("cache"))
}

fn list_cache_path() -> io::Result<PathBuf> {
    Ok(cache_dir()?.join("list.cache"))
}

pub fn entry_dir(id: &str) -> io::Result<PathBuf> {
    Ok(cached_base_dir().join(id))
}

pub fn entry_data_path(id: &str) -> io::Result<PathBuf> {
    Ok(data_dir()?.join(id.to_ascii_lowercase()))
}

pub fn entry_attr_path(id: &str) -> io::Result<PathBuf> {
    Ok(attr_dir()?.join(id.to_ascii_lowercase()))
}

fn tmp_dir() -> io::Result<PathBuf> {
    Ok(cached_base_dir().join("tmp"))
}

pub fn init() -> io::Result<()> {
    let base = cached_base_dir();
    fs::create_dir_all(base.join("data"))?;
    fs::create_dir_all(base.join("attr"))?;
    fs::create_dir_all(base.join("tmp"))?;
    fs::create_dir_all(base.join("cache"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Query API
// ---------------------------------------------------------------------------

pub fn list_entry_ids() -> io::Result<Vec<String>> {
    let attrs = attr_dir()?;
    let read_dir = match fs::read_dir(&attrs) {
        Ok(rd) => rd,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    let mut ids: Vec<String> = read_dir
        .filter_map(|item| item.ok())
        .map(|item| item.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort_unstable();
    ids.reverse();
    Ok(ids)
}

pub fn list() -> io::Result<Vec<Meta>> {
    if let Ok(items) = cache::read_list_cache() {
        return Ok(items);
    }
    let entry_ids = list_entry_ids()?;
    let mut out = Vec::with_capacity(entry_ids.len());
    for id in entry_ids {
        if let Ok(meta) = get_meta(&id) {
            out.push(meta);
        }
    }
    cache::write_list_cache(&out)?;
    Ok(out)
}

pub fn all_attr_keys() -> io::Result<Vec<(String, usize)>> {
    if let Ok(keys) = cache::read_attr_keys() {
        return Ok(keys);
    }
    let items = list()?;
    // Fall back to rebuilding from the list. write_list_cache already stored
    // the attr key index, so the next call will hit the cache.
    cache::write_list_cache(&items)?;
    cache::read_attr_keys()
}

pub fn newest() -> io::Result<Meta> {
    list()?
        .into_iter()
        .next()
        .ok_or_else(|| io::Error::other("stash is empty"))
}

pub fn nth_newest(n: usize) -> io::Result<Meta> {
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "n must be >= 1",
        ));
    }
    let items = list()?;
    items
        .into_iter()
        .nth(n - 1)
        .ok_or_else(|| io::Error::other("entry index out of range"))
}

pub fn older_than_ids(id: &str) -> io::Result<Vec<String>> {
    let items = list()?;
    for (idx, item) in items.iter().enumerate() {
        if item.id == id {
            return Ok(items[idx + 1..].iter().map(|m| m.id.clone()).collect());
        }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "entry not found"))
}

pub fn newer_than_ids(id: &str) -> io::Result<Vec<String>> {
    let items = list()?;
    for (idx, item) in items.iter().enumerate() {
        if item.id == id {
            return Ok(items[..idx].iter().map(|m| m.id.clone()).collect());
        }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "entry not found"))
}

pub fn resolve(input: &str) -> io::Result<String> {
    let raw = input.trim();
    if raw.is_empty() {
        return newest().map(|m| m.id);
    }
    if let Some(rest) = raw.strip_prefix('@') {
        let n = rest
            .parse::<usize>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid stack ref"))?;
        return nth_newest(n).map(|m| m.id);
    }
    let lower = raw.to_ascii_lowercase();
    if lower.bytes().all(|c| c.is_ascii_digit()) {
        let n = lower
            .parse::<usize>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid index"))?;
        return nth_newest(n).map(|m| m.id);
    }
    if lower.len() < MIN_ID_LEN {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "id too short"));
    }
    let ids = list_entry_ids()?;
    if ids.is_empty() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "stash is empty"));
    }
    if let Some(id) = ids.iter().find(|id| **id == lower) {
        return Ok(id.clone());
    }
    let mut prefix_match: Option<&String> = None;
    let mut suffix_match: Option<&String> = None;
    let mut prefix_ambig = false;
    let mut suffix_ambig = false;
    for id in &ids {
        if id.starts_with(&lower) {
            if prefix_match.is_some() {
                prefix_ambig = true;
            } else {
                prefix_match = Some(id);
            }
        }
        if id.ends_with(&lower) {
            if suffix_match.is_some() {
                suffix_ambig = true;
            } else {
                suffix_match = Some(id);
            }
        }
    }
    if let Some(id) = prefix_match {
        if !prefix_ambig {
            return Ok(id.clone());
        }
        return Err(io::Error::other("ambiguous id"));
    }
    if let Some(id) = suffix_match {
        if !suffix_ambig {
            return Ok(id.clone());
        }
        return Err(io::Error::other("ambiguous id"));
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "entry not found"))
}

pub fn get_meta(id: &str) -> io::Result<Meta> {
    let path = entry_attr_path(id)?;
    let data = fs::read_to_string(path)?;
    attr::parse_attr_file(&data).map_err(io::Error::other)
}

pub fn write_meta(id: &str, meta: &Meta) -> io::Result<()> {
    let result = fs::write(entry_attr_path(id)?, attr::encode_attr(meta));
    if result.is_ok() {
        cache::invalidate_list_cache();
    }
    result
}

pub fn set_attrs(id: &str, attrs: &BTreeMap<String, String>) -> io::Result<()> {
    let mut meta = get_meta(id)?;
    for (k, v) in attrs {
        meta.attrs.insert(k.clone(), v.clone());
    }
    write_meta(id, &meta)
}

pub fn unset_attrs(id: &str, keys: &[String]) -> io::Result<()> {
    let mut meta = get_meta(id)?;
    for key in keys {
        meta.attrs.remove(key);
    }
    write_meta(id, &meta)
}

pub fn cat_to_writer<W: Write>(id: &str, mut writer: W) -> io::Result<()> {
    let mut file = File::open(entry_data_path(id)?)?;
    std::io::copy(&mut file, &mut writer)?;
    Ok(())
}

pub fn remove(id: &str) -> io::Result<()> {
    let data_result = fs::remove_file(entry_data_path(id)?);
    if let Err(ref e) = data_result {
        if e.kind() != io::ErrorKind::NotFound {
            return data_result;
        }
    }
    let attr_result = fs::remove_file(entry_attr_path(id)?);
    if let Err(ref e) = attr_result {
        if e.kind() != io::ErrorKind::NotFound {
            return attr_result;
        }
    }
    cache::invalidate_list_cache();
    Ok(())
}

// Called by io::run_read_loop and io::save_or_abort_partial via super::
fn finalize_saved_entry(
    id: String,
    data_path: PathBuf,
    sample: &[u8],
    total: i64,
    attrs: BTreeMap<String, String>,
) -> io::Result<String> {
    let meta = Meta {
        id: id.clone(),
        ts: now_rfc3339ish()?,
        size: total,
        preview: build_preview_data(sample, sample.len()),
        attrs,
    };
    let tmp = tmp_dir()?;
    let attr_path = tmp.join(format!("{id}.attr"));
    fs::write(&attr_path, attr::encode_attr(&meta))?;
    fs::rename(&data_path, entry_data_path(&id)?)?;
    fs::rename(&attr_path, entry_attr_path(&id)?)?;
    cache::invalidate_list_cache();
    Ok(id)
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

pub fn human_size(n: i64) -> String {
    match n {
        n if n < 1024 => format!("{n}B"),
        n if n < 1024 * 1024 => format!("{:.1}K", n as f64 / 1024.0),
        n if n < 1024 * 1024 * 1024 => format!("{:.1}M", n as f64 / (1024.0 * 1024.0)),
        n => format!("{:.1}G", n as f64 / (1024.0 * 1024.0 * 1024.0)),
    }
}

fn now_rfc3339ish() -> io::Result<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    let secs = now.as_secs() as i64;
    let nanos = now.subsec_nanos();
    let dt = unix_to_utc(secs);
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{nanos:09}Z",
        dt.year, dt.month, dt.day, dt.hour, dt.min, dt.sec,
    ))
}

pub fn unix_to_utc(secs: i64) -> UtcDateTime {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    UtcDateTime {
        year,
        month,
        day,
        hour: (rem / 3600) as u32,
        min: ((rem % 3600) / 60) as u32,
        sec: (rem % 60) as u32,
    }
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u32, d as u32)
}

fn new_ulid() -> io::Result<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_millis() as u64;
    let mut bytes = [0u8; 16];
    for (i, byte) in bytes.iter_mut().enumerate().take(6) {
        *byte = ((now >> (8 * (5 - i))) & 0xff) as u8;
    }
    let mut rand = File::open("/dev/urandom")?;
    rand.read_exact(&mut bytes[6..])?;
    Ok(encode_ulid(bytes).to_ascii_lowercase())
}

fn encode_ulid(bytes: [u8; 16]) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut value = 0u128;
    for byte in bytes {
        value = (value << 8) | byte as u128;
    }
    let mut out = [0u8; 26];
    for i in (0..26).rev() {
        out[i] = ALPHABET[(value & 0x1f) as usize];
        value >>= 5;
    }
    // SAFETY: out contains only bytes from ALPHABET, which is ASCII-only
    unsafe { String::from_utf8_unchecked(out.to_vec()) }
}

pub fn add_filename_attr(path: &Path, attrs: &mut BTreeMap<String, String>) {
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
        attrs.insert("filename".into(), name.into());
    }
}
