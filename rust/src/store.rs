use crate::json::{JsonValue, escape_string, parse};
use crate::preview::build_preview_data;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SHORT_ID_LEN: usize = 8;
pub const MIN_ID_LEN: usize = 6;

#[derive(Debug)]
pub struct PartialSavedError {
    pub id: String,
    pub cause: io::Error,
}

impl std::fmt::Display for PartialSavedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "partial entry saved as \"{}\": {}", self.id.to_ascii_lowercase(), self.cause)
    }
}

impl StdError for PartialSavedError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.cause)
    }
}

#[derive(Clone, Debug)]
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

    pub fn to_json_pretty(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(&format!("  \"id\": \"{}\",\n", escape_string(&self.id)));
        out.push_str(&format!("  \"ts\": \"{}\",\n", escape_string(&self.ts)));
        out.push_str(&format!("  \"size\": {}", self.size));
        if !self.preview.is_empty() {
            out.push_str(&format!(",\n  \"preview\": \"{}\"", escape_string(&self.preview)));
        }
        if !self.attrs.is_empty() {
            out.push_str(",\n  \"meta\": {\n");
            let mut first = true;
            for (k, v) in &self.attrs {
                if !first {
                    out.push_str(",\n");
                }
                first = false;
                out.push_str(&format!(
                    "    \"{}\": \"{}\"",
                    escape_string(k),
                    escape_string(v)
                ));
            }
            out.push_str("\n  }");
        }
        out.push_str("\n}\n");
        out
    }

    pub fn to_json_compact(&self) -> String {
        let mut out = String::new();
        out.push('{');
        out.push_str(&format!("\"id\":\"{}\"", escape_string(&self.id)));
        out.push_str(&format!(",\"ts\":\"{}\"", escape_string(&self.ts)));
        out.push_str(&format!(",\"size\":{}", self.size));
        if !self.preview.is_empty() {
            out.push_str(&format!(",\"preview\":\"{}\"", escape_string(&self.preview)));
        }
        if !self.attrs.is_empty() {
            out.push_str(",\"meta\":{");
            let mut first = true;
            for (k, v) in &self.attrs {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&format!("\"{}\":\"{}\"", escape_string(k), escape_string(v)));
            }
            out.push('}');
        }
        out.push('}');
        out
    }

    pub fn from_json_str(input: &str) -> Result<Self, String> {
        let root = parse(input)?;
        let JsonValue::Object(obj) = root else {
            return Err("meta.json root must be an object".into());
        };
        let id = get_string(&obj, "id")?;
        let ts = get_string(&obj, "ts")?;
        let size = get_number(&obj, "size")?;
        let preview = get_optional_string(&obj, "preview").unwrap_or_default();
        let attrs = match obj.get("meta") {
            Some(JsonValue::Object(meta_obj)) => {
                let mut out = BTreeMap::new();
                for (k, v) in meta_obj {
                    match v {
                        JsonValue::String(s) => {
                            out.insert(k.clone(), s.clone());
                        }
                        _ => return Err("meta values must be strings".into()),
                    }
                }
                out
            }
            Some(JsonValue::Null) | None => BTreeMap::new(),
            _ => return Err("meta must be an object".into()),
        };
        Ok(Self {
            id,
            ts,
            size,
            preview,
            attrs,
        })
    }
}

fn get_string(obj: &BTreeMap<String, JsonValue>, key: &str) -> Result<String, String> {
    match obj.get(key) {
        Some(JsonValue::String(s)) => Ok(s.clone()),
        _ => Err(format!("missing string field {key}")),
    }
}

fn get_optional_string(obj: &BTreeMap<String, JsonValue>, key: &str) -> Option<String> {
    match obj.get(key) {
        Some(JsonValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn get_number(obj: &BTreeMap<String, JsonValue>, key: &str) -> Result<i64, String> {
    match obj.get(key) {
        Some(JsonValue::Number(n)) => Ok(*n),
        _ => Err(format!("missing numeric field {key}")),
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

pub fn entries_dir() -> io::Result<PathBuf> {
    Ok(base_dir()?.join("entries"))
}

fn cache_dir() -> io::Result<PathBuf> {
    Ok(base_dir()?.join("cache"))
}

fn list_cache_path() -> io::Result<PathBuf> {
    Ok(cache_dir()?.join("list.cache"))
}

pub fn entry_dir(id: &str) -> io::Result<PathBuf> {
    Ok(entries_dir()?.join(id))
}

pub fn entry_data_path(id: &str) -> io::Result<PathBuf> {
    Ok(entry_dir(id)?.join("data"))
}

fn tmp_dir() -> io::Result<PathBuf> {
    Ok(base_dir()?.join("tmp"))
}

pub fn init() -> io::Result<()> {
    let base = base_dir()?;
    fs::create_dir_all(base.join("entries"))?;
    fs::create_dir_all(base.join("tmp"))?;
    fs::create_dir_all(base.join("cache"))?;
    Ok(())
}

fn entries_mtime_key() -> io::Result<String> {
    let modified = fs::metadata(entries_dir()?)?.modified()?;
    let duration = modified.duration_since(UNIX_EPOCH).map_err(io::Error::other)?;
    Ok(format!("{}.{:09}", duration.as_secs(), duration.subsec_nanos()))
}

fn invalidate_list_cache() {
    if let Ok(path) = list_cache_path() {
        let _ = fs::remove_file(path);
    }
}

pub fn list_entry_ids() -> io::Result<Vec<String>> {
    let mut ids = Vec::new();
    let entries = entries_dir()?;
    match fs::read_dir(entries) {
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
    let path = list_cache_path()?;
    let data = fs::read_to_string(path)?;
    let mut lines = data.lines();
    let Some(header) = lines.next() else {
        return Err(io::Error::other("empty list cache"));
    };
    let Some(cached_mtime) = header.strip_prefix("mtime=") else {
        return Err(io::Error::other("invalid list cache header"));
    };
    let current_mtime = entries_mtime_key()?;
    if cached_mtime != current_mtime {
        return Err(io::Error::other("stale list cache"));
    }
    let mut items = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let item = Meta::from_json_str(line).map_err(io::Error::other)?;
        items.push(item);
    }
    Ok(items)
}

fn write_list_cache(items: &[Meta]) -> io::Result<()> {
    init()?;
    let path = list_cache_path()?;
    let mut out = String::new();
    out.push_str("mtime=");
    out.push_str(&entries_mtime_key()?);
    out.push('\n');
    for item in items {
        out.push_str(&item.to_json_compact());
        out.push('\n');
    }
    fs::write(path, out)
}

pub fn newest() -> io::Result<Meta> {
    list()?.into_iter().next().ok_or_else(|| io::Error::other("stash is empty"))
}

pub fn nth_newest(n: usize) -> io::Result<Meta> {
    if n == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "n must be >= 1"));
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
        let n = rest.parse::<usize>().map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid stack ref"))?;
        return nth_newest(n).map(|m| m.id);
    }
    let upper = raw.to_ascii_uppercase();
    if upper.chars().all(|c| c.is_ascii_digit()) {
        let n = upper.parse::<usize>().map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid index"))?;
        return nth_newest(n).map(|m| m.id);
    }
    if upper.len() < MIN_ID_LEN {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "id too short"));
    }
    let ids = list_entry_ids()?;
    if ids.is_empty() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "stash is empty"));
    }
    if let Some(id) = ids.iter().find(|id| **id == upper) {
        return Ok(id.clone());
    }
    let prefix: Vec<_> = ids.iter().filter(|id| id.starts_with(&upper)).cloned().collect();
    if prefix.len() == 1 {
        return Ok(prefix[0].clone());
    }
    if prefix.len() > 1 {
        return Err(io::Error::other("ambiguous id"));
    }
    let suffix: Vec<_> = ids.iter().filter(|id| id.ends_with(&upper)).cloned().collect();
    if suffix.len() == 1 {
        return Ok(suffix[0].clone());
    }
    if suffix.len() > 1 {
        return Err(io::Error::other("ambiguous id"));
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "entry not found"))
}

pub fn get_meta(id: &str) -> io::Result<Meta> {
    let path = entry_dir(id)?.join("meta.json");
    let data = fs::read_to_string(path)?;
    Meta::from_json_str(&data).map_err(io::Error::other)
}

pub fn write_meta(id: &str, meta: &Meta) -> io::Result<()> {
    let result = fs::write(entry_dir(id)?.join("meta.json"), meta.to_json_pretty());
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
    let result = fs::remove_dir_all(entry_dir(id)?);
    if result.is_ok() {
        invalidate_list_cache();
    }
    result
}

pub fn push_from_reader<R: Read>(reader: &mut R, attrs: BTreeMap<String, String>) -> io::Result<String> {
    init()?;
    let id = new_ulid()?;
    let tmp = tmp_dir()?.join(&id);
    fs::create_dir_all(&tmp)?;
    let data_path = tmp.join("data");
    let mut data = File::create(&data_path)?;
    let mut sample = Vec::new();
    let mut total = 0i64;
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        if sample.len() < 512 {
            let need = (512 - sample.len()).min(n);
            sample.extend_from_slice(&buf[..need]);
        }
        data.write_all(&buf[..n])?;
        total += n as i64;
    }
    drop(data);

    let meta = Meta {
        id: id.clone(),
        ts: now_rfc3339ish()?,
        size: total,
        preview: build_preview_data(&sample, sample.len()),
        attrs,
    };
    fs::write(tmp.join("meta.json"), meta.to_json_pretty())?;
    fs::rename(tmp, entry_dir(&id)?)?;
    invalidate_list_cache();
    Ok(id)
}

pub fn tee_from_reader_partial<R: Read, W: Write>(
    reader: &mut R,
    stdout: &mut W,
    mut attrs: BTreeMap<String, String>,
    partial: bool,
) -> io::Result<String> {
    init()?;
    let id = new_ulid()?;
    let tmp = tmp_dir()?.join(&id);
    fs::create_dir_all(&tmp)?;
    let mut data = File::create(tmp.join("data"))?;
    let mut sample = Vec::new();
    let mut total = 0i64;
    let mut buf = [0u8; 8192];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(n) => n,
            Err(err) => {
                if !partial || total == 0 {
                    let _ = fs::remove_dir_all(&tmp);
                    return Err(err);
                }
                attrs.insert("partial".into(), "true".into());
                let meta = Meta {
                    id: id.clone(),
                    ts: now_rfc3339ish()?,
                    size: total,
                    preview: build_preview_data(&sample, sample.len()),
                    attrs,
                };
                fs::write(tmp.join("meta.json"), meta.to_json_pretty())?;
                fs::rename(&tmp, entry_dir(&id)?)?;
                invalidate_list_cache();
                return Err(io::Error::other(PartialSavedError { id, cause: err }));
            }
        };
        if n == 0 {
            break;
        }
        if sample.len() < 512 {
            let need = (512 - sample.len()).min(n);
            sample.extend_from_slice(&buf[..need]);
        }
        if let Err(err) = data.write_all(&buf[..n]) {
            let _ = fs::remove_dir_all(&tmp);
            return Err(err);
        }
        if let Err(err) = stdout.write_all(&buf[..n]) {
            if !partial || total == 0 {
                let _ = fs::remove_dir_all(&tmp);
                return Err(err);
            }
            attrs.insert("partial".into(), "true".into());
            let meta = Meta {
                id: id.clone(),
                ts: now_rfc3339ish()?,
                size: total,
                preview: build_preview_data(&sample, sample.len()),
                attrs,
            };
            fs::write(tmp.join("meta.json"), meta.to_json_pretty())?;
            fs::rename(&tmp, entry_dir(&id)?)?;
            invalidate_list_cache();
            return Err(io::Error::other(PartialSavedError { id, cause: err }));
        }
        total += n as i64;
    }
    drop(data);

    let meta = Meta {
        id: id.clone(),
        ts: now_rfc3339ish()?,
        size: total,
        preview: build_preview_data(&sample, sample.len()),
        attrs,
    };
    fs::write(tmp.join("meta.json"), meta.to_json_pretty())?;
    fs::rename(tmp, entry_dir(&id)?)?;
    invalidate_list_cache();
    Ok(id)
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
    let (year, month, day, hour, min, sec) = unix_to_utc(secs);
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}.{nanos:09}Z"
    ))
}

fn unix_to_utc(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = (rem / 3600) as u32;
    let min = ((rem % 3600) / 60) as u32;
    let sec = (rem % 60) as u32;
    let (year, month, day) = civil_from_days(days);
    (year, month, day, hour, min, sec)
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
    for i in 0..6 {
        bytes[i] = ((now >> (8 * (5 - i))) & 0xff) as u8;
    }
    let mut rand = File::open("/dev/urandom")?;
    rand.read_exact(&mut bytes[6..])?;
    Ok(encode_ulid(bytes))
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
