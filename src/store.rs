use crate::preview::build_preview_data;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use signal_hook::SigId;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::low_level;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SHORT_ID_LEN: usize = 8;
pub const MIN_ID_LEN: usize = 6;
const LIST_CACHE_VERSION: u32 = 2;

#[derive(Debug)]
pub struct PartialSavedError {
    pub id: String,
    pub cause: io::Error,
    pub signal: Option<i32>,
}

struct PartialSaveOptions {
    save_on_error: bool,
    save_empty: bool,
    signal: Option<i32>,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    pub id: String,
    pub ts: String,
    pub size: i64,
    pub preview: String,
    pub attrs: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ListCacheFile {
    version: u32,
    data_mtime: String,
    attr_mtime: String,
    items: Vec<Meta>,
    attr_keys: BTreeMap<String, usize>,
}

impl Meta {
    pub fn short_id(&self) -> String {
        self.id[self.id.len().saturating_sub(SHORT_ID_LEN)..].to_ascii_lowercase()
    }

    pub fn display_id(&self) -> String {
        self.id.to_ascii_lowercase()
    }

    pub fn to_json_value(&self, include_preview: bool) -> Value {
        let mut map = serde_json::Map::new();
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

pub fn base_dir() -> io::Result<PathBuf> {
    match std::env::var("STASH_DIR") {
        Ok(dir) if !dir.trim().is_empty() => Ok(PathBuf::from(dir)),
        _ => {
            let home = std::env::var("HOME")
                .map(PathBuf::from)
                .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "HOME not set"))?;
            Ok(home.join(".stash"))
        }
    }
}

pub fn data_dir() -> io::Result<PathBuf> {
    Ok(base_dir()?.join("data"))
}

pub fn attr_dir() -> io::Result<PathBuf> {
    Ok(base_dir()?.join("attr"))
}

fn cache_dir() -> io::Result<PathBuf> {
    Ok(base_dir()?.join("cache"))
}

fn list_cache_path() -> io::Result<PathBuf> {
    Ok(cache_dir()?.join("list.cache"))
}

pub fn entry_dir(id: &str) -> io::Result<PathBuf> {
    Ok(base_dir()?.join(id))
}

pub fn entry_data_path(id: &str) -> io::Result<PathBuf> {
    Ok(data_dir()?.join(id.to_ascii_lowercase()))
}

pub fn entry_attr_path(id: &str) -> io::Result<PathBuf> {
    Ok(attr_dir()?.join(id.to_ascii_lowercase()))
}

fn tmp_dir() -> io::Result<PathBuf> {
    Ok(base_dir()?.join("tmp"))
}

pub fn init() -> io::Result<()> {
    let base = base_dir()?;
    fs::create_dir_all(base.join("data"))?;
    fs::create_dir_all(base.join("attr"))?;
    fs::create_dir_all(base.join("tmp"))?;
    fs::create_dir_all(base.join("cache"))?;
    Ok(())
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

fn invalidate_list_cache() {
    if let Ok(path) = list_cache_path() {
        let _ = fs::remove_file(path);
    }
}

pub fn list_entry_ids() -> io::Result<Vec<String>> {
    let mut ids = Vec::new();
    let attrs = attr_dir()?;
    match fs::read_dir(attrs) {
        Ok(read_dir) => {
            for item in read_dir {
                let item = item?;
                ids.push(item.file_name().to_string_lossy().into_owned());
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(ids),
        Err(err) => return Err(err),
    }
    ids.sort();
    ids.reverse();
    Ok(ids)
}

pub fn list() -> io::Result<Vec<Meta>> {
    if let Ok(items) = read_list_cache() {
        return Ok(items);
    }
    let mut out = Vec::new();
    for id in list_entry_ids()? {
        if let Ok(meta) = get_meta(&id) {
            out.push(meta);
        }
    }
    write_list_cache(&out)?;
    Ok(out)
}

fn read_list_cache() -> io::Result<Vec<Meta>> {
    Ok(read_list_cache_file()?.items)
}

fn read_list_cache_file() -> io::Result<ListCacheFile> {
    let path = list_cache_path()?;
    let data = fs::read(path)?;
    let cfg = bincode::config::standard();
    let (cache, _): (ListCacheFile, usize) =
        bincode::serde::decode_from_slice(&data, cfg).map_err(io::Error::other)?;
    if cache.version != LIST_CACHE_VERSION {
        return Err(io::Error::other("stale list cache"));
    }
    let current_data = dir_mtime_key(data_dir()?)?;
    let current_attr = dir_mtime_key(attr_dir()?)?;
    if cache.data_mtime != current_data || cache.attr_mtime != current_attr {
        return Err(io::Error::other("stale list cache"));
    }
    Ok(cache)
}

fn write_list_cache(items: &[Meta]) -> io::Result<()> {
    init()?;
    let path = list_cache_path()?;
    let cache = ListCacheFile {
        version: LIST_CACHE_VERSION,
        data_mtime: dir_mtime_key(data_dir()?)?,
        attr_mtime: dir_mtime_key(attr_dir()?)?,
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

pub fn all_attr_keys() -> io::Result<Vec<(String, usize)>> {
    if let Ok(cache) = read_list_cache_file() {
        return Ok(cache.attr_keys.into_iter().collect());
    }
    let items = list()?;
    Ok(build_attr_key_index(&items).into_iter().collect())
}

pub fn newest() -> io::Result<Meta> {
    let id = list_entry_ids()?
        .into_iter()
        .next()
        .ok_or_else(|| io::Error::other("stash is empty"))?;
    get_meta(&id)
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
    if lower.chars().all(|c| c.is_ascii_digit()) {
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
    let prefix: Vec<_> = ids
        .iter()
        .filter(|id| id.starts_with(&lower))
        .cloned()
        .collect();
    if prefix.len() == 1 {
        return Ok(prefix[0].clone());
    }
    if prefix.len() > 1 {
        return Err(io::Error::other("ambiguous id"));
    }
    let suffix: Vec<_> = ids
        .iter()
        .filter(|id| id.ends_with(&lower))
        .cloned()
        .collect();
    if suffix.len() == 1 {
        return Ok(suffix[0].clone());
    }
    if suffix.len() > 1 {
        return Err(io::Error::other("ambiguous id"));
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "entry not found"))
}

pub fn get_meta(id: &str) -> io::Result<Meta> {
    let path = entry_attr_path(id)?;
    let data = fs::read_to_string(path)?;
    parse_attr_file(&data).map_err(io::Error::other)
}

pub fn write_meta(id: &str, meta: &Meta) -> io::Result<()> {
    let result = fs::write(entry_attr_path(id)?, encode_attr(meta));
    if result.is_ok() {
        invalidate_list_cache();
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
    io::copy(&mut file, &mut writer)?;
    Ok(())
}

pub fn remove(id: &str) -> io::Result<()> {
    let data_result = fs::remove_file(entry_data_path(id)?);
    if data_result.is_err()
        && data_result
            .as_ref()
            .err()
            .is_some_and(|e| e.kind() != io::ErrorKind::NotFound)
    {
        return data_result;
    }
    let attr_result = fs::remove_file(entry_attr_path(id)?);
    if attr_result.is_err()
        && attr_result
            .as_ref()
            .err()
            .is_some_and(|e| e.kind() != io::ErrorKind::NotFound)
    {
        return attr_result;
    }
    invalidate_list_cache();
    Ok(())
}

pub fn push_from_reader<R: Read>(
    reader: &mut R,
    attrs: BTreeMap<String, String>,
) -> io::Result<String> {
    init()?;
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal = Arc::new(AtomicI32::new(0));
    let _signal_guard = SignalGuard::new(&interrupted, &signal)?;
    let id = new_ulid()?;
    let data_path = tmp_dir()?.join(format!("{id}.data"));
    let data = File::create(&data_path)?;
    run_read_loop(reader, None, data, data_path, id, attrs, &interrupted, &signal, true)
}

pub fn tee_from_reader_partial<R: Read, W: Write>(
    reader: &mut R,
    stdout: &mut W,
    attrs: BTreeMap<String, String>,
    save_on_error: bool,
) -> io::Result<String> {
    init()?;
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal = Arc::new(AtomicI32::new(0));
    let _signal_guard = SignalGuard::new(&interrupted, &signal)?;
    let id = new_ulid()?;
    let data_path = tmp_dir()?.join(format!("{id}.data"));
    let data = File::create(&data_path)?;
    run_read_loop(
        reader,
        Some(stdout as &mut dyn Write),
        data,
        data_path,
        id,
        attrs,
        &interrupted,
        &signal,
        save_on_error,
    )
}

fn run_read_loop<R: Read>(
    reader: &mut R,
    mut tee: Option<&mut dyn Write>,
    mut data: File,
    data_path: PathBuf,
    id: String,
    attrs: BTreeMap<String, String>,
    interrupted: &Arc<AtomicBool>,
    signal: &Arc<AtomicI32>,
    save_on_error: bool,
) -> io::Result<String> {
    let mut sample = Vec::new();
    let mut total = 0i64;
    let mut buf = [0u8; 8192];
    loop {
        if interrupted.load(Ordering::Relaxed) {
            return save_or_abort_partial(
                id,
                data_path,
                &sample,
                total,
                attrs,
                signal_error(signal),
                PartialSaveOptions {
                    save_on_error,
                    save_empty: true,
                    signal: Some(signal.load(Ordering::Relaxed)),
                },
            );
        }
        let n = match reader.read(&mut buf) {
            Ok(n) => n,
            Err(err) => {
                return save_or_abort_partial(
                    id,
                    data_path,
                    &sample,
                    total,
                    attrs,
                    err,
                    PartialSaveOptions {
                        save_on_error,
                        save_empty: false,
                        signal: None,
                    },
                );
            }
        };
        if n == 0 {
            if interrupted.load(Ordering::Relaxed) {
                return save_or_abort_partial(
                    id,
                    data_path,
                    &sample,
                    total,
                    attrs,
                    signal_error(signal),
                    PartialSaveOptions {
                        save_on_error,
                        save_empty: true,
                        signal: Some(signal.load(Ordering::Relaxed)),
                    },
                );
            }
            break;
        }
        if sample.len() < 512 {
            let need = (512 - sample.len()).min(n);
            sample.extend_from_slice(&buf[..need]);
        }
        if let Err(err) = data.write_all(&buf[..n]) {
            let _ = fs::remove_file(&data_path);
            return Err(err);
        }
        total += n as i64;
        if let Some(ref mut out) = tee {
            if let Err(err) = out.write_all(&buf[..n]) {
                drop(data);
                if err.kind() == io::ErrorKind::BrokenPipe {
                    return finalize_saved_entry(id, data_path, &sample, total, attrs);
                }
                return save_or_abort_partial(
                    id,
                    data_path,
                    &sample,
                    total,
                    attrs,
                    err,
                    PartialSaveOptions {
                        save_on_error,
                        save_empty: false,
                        signal: None,
                    },
                );
            }
        }
    }
    drop(data);
    finalize_saved_entry(id, data_path, &sample, total, attrs)
}

fn save_or_abort_partial(
    id: String,
    data_path: PathBuf,
    sample: &[u8],
    total: i64,
    mut attrs: BTreeMap<String, String>,
    err: io::Error,
    options: PartialSaveOptions,
) -> io::Result<String> {
    if !options.save_on_error || (total == 0 && !options.save_empty) {
        let _ = fs::remove_file(&data_path);
        return Err(err);
    }
    attrs.insert("partial".into(), "true".into());
    finalize_saved_entry(id.clone(), data_path, sample, total, attrs)?;
    Err(io::Error::other(PartialSavedError {
        id,
        cause: err,
        signal: options.signal,
    }))
}

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
    fs::write(&attr_path, encode_attr(&meta))?;
    fs::rename(&data_path, entry_data_path(&id)?)?;
    fs::rename(&attr_path, entry_attr_path(&id)?)?;
    invalidate_list_cache();
    Ok(id)
}

struct SignalGuard {
    ids: Vec<SigId>,
}

impl SignalGuard {
    fn new(flag: &Arc<AtomicBool>, signal: &Arc<AtomicI32>) -> io::Result<Self> {
        let ids = vec![
            register_signal(SIGINT, flag, signal)?,
            register_signal(SIGTERM, flag, signal)?,
        ];
        Ok(Self { ids })
    }
}

impl Drop for SignalGuard {
    fn drop(&mut self) {
        for id in self.ids.drain(..) {
            low_level::unregister(id);
        }
    }
}

fn register_signal(
    signo: i32,
    flag: &Arc<AtomicBool>,
    signal: &Arc<AtomicI32>,
) -> io::Result<SigId> {
    let flag = Arc::clone(flag);
    let signal = Arc::clone(signal);
    unsafe {
        low_level::register(signo, move || {
            signal.store(signo, Ordering::Relaxed);
            flag.store(true, Ordering::Relaxed);
        })
    }
    .map_err(io::Error::other)
}

fn signal_error(signal: &Arc<AtomicI32>) -> io::Error {
    let signo = signal.load(Ordering::Relaxed);
    let msg = match signo {
        SIGTERM => "terminated by signal",
        _ => "interrupted by signal",
    };
    io::Error::new(io::ErrorKind::Interrupted, msg)
}

pub fn human_size(n: i64) -> String {
    match n {
        n if n < 1024 => format!("{n}B"),
        n if n < 1024 * 1024 => format!("{:.1}K", n as f64 / 1024.0),
        n if n < 1024 * 1024 * 1024 => format!("{:.1}M", n as f64 / 1024.0 / 1024.0),
        n => format!("{:.1}G", n as f64 / 1024.0 / 1024.0 / 1024.0),
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
    let mut out = [b'0'; 26];
    for i in (0..26).rev() {
        out[i] = ALPHABET[(value & 0x1f) as usize];
        value >>= 5;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn add_filename_attr(path: &Path, attrs: &mut BTreeMap<String, String>) {
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
        attrs.insert("filename".into(), name.into());
    }
}

fn encode_attr(meta: &Meta) -> String {
    let mut out = String::new();
    write_attr_line(&mut out, "id", &meta.id);
    write_attr_line(&mut out, "ts", &meta.ts);
    write_attr_line(&mut out, "size", &meta.size.to_string());
    if !meta.preview.trim().is_empty() {
        write_attr_line(&mut out, "preview", &meta.preview);
    }
    for (k, v) in &meta.attrs {
        write_attr_line(&mut out, k, v);
    }
    out
}

fn write_attr_line(out: &mut String, key: &str, value: &str) {
    out.push_str(&escape_attr(key));
    out.push('=');
    out.push_str(&escape_attr(value));
    out.push('\n');
}

fn escape_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '=' => out.push_str("\\="),
            _ => out.push(ch),
        }
    }
    out
}

fn parse_attr_file(input: &str) -> Result<Meta, String> {
    let mut meta = Meta {
        id: String::new(),
        ts: String::new(),
        size: 0,
        preview: String::new(),
        attrs: BTreeMap::new(),
    };
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = split_attr_line(line) else {
            return Err(format!("invalid attr line {line:?}"));
        };
        let key = unescape_attr(key)?;
        let value = unescape_attr(value)?;
        match key.as_str() {
            "id" => meta.id = value,
            "ts" => meta.ts = value,
            "size" => {
                meta.size = value
                    .parse::<i64>()
                    .map_err(|_| format!("invalid size {value:?}"))?
            }
            "preview" => meta.preview = value,
            _ => {
                meta.attrs.insert(key, value);
            }
        }
    }
    Ok(meta)
}

fn split_attr_line(line: &str) -> Option<(&str, &str)> {
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '\\' => escaped = !escaped,
            '=' if !escaped => return Some((&line[..idx], &line[idx + 1..])),
            _ => escaped = false,
        }
    }
    None
}

fn unescape_attr(input: &str) -> Result<String, String> {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(next) = chars.next() else {
            return Err("unterminated attr escape".into());
        };
        match next {
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '=' => out.push('='),
            other => return Err(format!("invalid attr escape \\{other}")),
        }
    }
    Ok(out)
}
