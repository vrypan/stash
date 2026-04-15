use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use signal_hook::consts::signal::SIGTERM;
use signal_hook::flag;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use crate::store;

const DAEMON_CACHE_VERSION: u32 = 1;

pub type HashMapById = HashMap<String, String>;

#[derive(
    Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, serde::Serialize, serde::Deserialize,
)]
struct DaemonCacheFile {
    version: u32,
    entries: BTreeMap<String, String>,
}

pub fn run() -> io::Result<()> {
    store::init()?;
    let attr_dir = store::attr_dir()?;
    let cache_path = daemon_cache_path()?;
    let mut hashes = reconcile_startup_state(&attr_dir, &cache_path, |line| println!("{line}"))?;
    write_cache_file(&cache_path, &hashes)?;

    let terminated = Arc::new(AtomicBool::new(false));
    flag::register(SIGTERM, Arc::clone(&terminated)).map_err(io::Error::other)?;

    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |result| {
            let _ = tx.send(result);
        },
        Config::default(),
    )
    .map_err(io::Error::other)?;

    watcher
        .watch(&attr_dir, RecursiveMode::NonRecursive)
        .map_err(io::Error::other)?;

    loop {
        if terminated.load(Ordering::Relaxed) {
            write_cache_file(&cache_path, &hashes)?;
            return Ok(());
        }

        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(Ok(event)) => {
                if process_event(&attr_dir, &mut hashes, event, |line| println!("{line}"))? {
                    write_cache_file(&cache_path, &hashes)?;
                }
            }
            Ok(Err(err)) => eprintln!("watch error: {err}"),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                write_cache_file(&cache_path, &hashes)?;
                return Err(io::Error::other("watch channel disconnected"));
            }
        }
    }
}

fn reconcile_startup_state<F>(
    attr_dir: &Path,
    cache_path: &Path,
    emit: F,
) -> io::Result<HashMapById>
where
    F: FnMut(&str),
{
    let cached = read_cache_file(cache_path)?;
    let current = build_hash_map(attr_dir)?;
    if let Some(cached) = cached {
        emit_differences(&cached, &current, emit);
    }
    Ok(current)
}

fn daemon_cache_path() -> io::Result<PathBuf> {
    Ok(store::base_dir()?.join("cache").join("daemon.cache"))
}

fn build_hash_map(attr_dir: &Path) -> io::Result<HashMapById> {
    let read_dir = match fs::read_dir(attr_dir) {
        Ok(read_dir) => read_dir,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(err) => return Err(err),
    };

    let mut hashes = HashMap::new();
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        let Some(id) = attr_id_from_path(attr_dir, &path) else {
            continue;
        };
        if let Some(hash) = hash_file_if_present(&path)? {
            hashes.insert(id, hash);
        }
    }
    Ok(hashes)
}

fn should_process_event_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(CreateKind::Any | CreateKind::File)
            | EventKind::Modify(
                ModifyKind::Any
                    | ModifyKind::Data(_)
                    | ModifyKind::Metadata(_)
                    | ModifyKind::Name(_)
            )
            | EventKind::Remove(RemoveKind::Any | RemoveKind::File)
    )
}

fn unique_attr_paths(attr_dir: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        if path.parent() != Some(attr_dir) {
            continue;
        }
        if out.iter().any(|existing| existing == path) {
            continue;
        }
        out.push(path.clone());
    }
    out
}

fn attr_id_from_path(attr_dir: &Path, path: &Path) -> Option<String> {
    if path.parent() != Some(attr_dir) {
        return None;
    }
    let id = path.file_name()?.to_str()?.to_ascii_lowercase();
    is_full_entry_id(&id).then_some(id)
}

fn hash_file_if_present(path: &Path) -> io::Result<Option<String>> {
    match fs::read(path) {
        Ok(data) => Ok(Some(blake3::hash(&data).to_hex().to_string())),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn is_full_entry_id(value: &str) -> bool {
    value.len() == 26
        && value.bytes().all(|b| {
            matches!(b, b'0'..=b'9' | b'a'..=b'z') && !matches!(b, b'i' | b'l' | b'o' | b'u')
        })
}

fn process_event<F>(
    attr_dir: &Path,
    hashes: &mut HashMapById,
    event: Event,
    mut emit: F,
) -> io::Result<bool>
where
    F: FnMut(&str),
{
    if !should_process_event_kind(&event.kind) {
        return Ok(false);
    }

    let mut changed = false;
    for path in unique_attr_paths(attr_dir, &event.paths) {
        let Some(id) = attr_id_from_path(attr_dir, &path) else {
            continue;
        };
        let old = hashes.get(&id).cloned();
        match hash_file_if_present(&path)? {
            Some(new) => {
                if old.as_deref() != Some(new.as_str()) {
                    emit_transition(&id, old.as_deref(), Some(new.as_str()), &mut emit);
                    hashes.insert(id, new);
                    changed = true;
                }
            }
            None => {
                if old.is_some() {
                    emit_transition(&id, old.as_deref(), None, &mut emit);
                    hashes.remove(&id);
                    changed = true;
                }
            }
        }
    }

    Ok(changed)
}

fn emit_differences<F>(old: &HashMapById, new: &HashMapById, mut emit: F)
where
    F: FnMut(&str),
{
    let mut ids = BTreeMap::new();
    for id in old.keys() {
        ids.insert(id.clone(), ());
    }
    for id in new.keys() {
        ids.insert(id.clone(), ());
    }

    for id in ids.keys() {
        let old_hash = old.get(id).map(String::as_str);
        let new_hash = new.get(id).map(String::as_str);
        if old_hash != new_hash {
            emit_transition(id, old_hash, new_hash, &mut emit);
        }
    }
}

fn emit_transition<F>(id: &str, old: Option<&str>, new: Option<&str>, emit: &mut F)
where
    F: FnMut(&str),
{
    let line = format!("{} {} {}", id, old.unwrap_or("-"), new.unwrap_or("-"));
    emit(&line);
}

fn read_cache_file(path: &Path) -> io::Result<Option<HashMapById>> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let cache = match rkyv::from_bytes::<DaemonCacheFile, rkyv::rancor::Error>(&data) {
        Ok(cache) => cache,
        Err(_) => return Ok(None),
    };
    if cache.version != DAEMON_CACHE_VERSION {
        return Ok(None);
    }
    Ok(Some(cache.entries.into_iter().collect()))
}

fn write_cache_file(path: &Path, hashes: &HashMapById) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let cache = DaemonCacheFile {
        version: DAEMON_CACHE_VERSION,
        entries: hashes_to_btree(hashes),
    };
    let encoded = rkyv::to_bytes::<rkyv::rancor::Error>(&cache).map_err(io::Error::other)?;
    fs::write(path, encoded)
}

fn hashes_to_btree(hashes: &HashMapById) -> BTreeMap<String, String> {
    hashes.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{DataChange, RenameMode};
    use tempfile::TempDir;

    fn temp_attr_dir() -> TempDir {
        tempfile::Builder::new()
            .prefix("stashd-test-")
            .tempdir()
            .unwrap()
    }

    #[test]
    fn build_hash_map_indexes_only_ulid_named_files() {
        let dir = temp_attr_dir();
        let keep_id = "01knxf1n5ffvk9jsm8wve1pgsd";
        fs::write(dir.path().join(keep_id), b"id=1\n").unwrap();
        fs::write(dir.path().join("not-an-id"), b"skip\n").unwrap();

        let hashes = build_hash_map(dir.path()).unwrap();

        assert_eq!(hashes.len(), 1);
        assert_eq!(
            hashes.get(keep_id),
            Some(&blake3::hash(b"id=1\n").to_hex().to_string())
        );
    }

    #[test]
    fn startup_reconcile_emits_modify_create_and_remove_differences() {
        let dir = temp_attr_dir();
        let cache_path = dir.path().join("daemon.cache");

        let removed = "01knxf1n5ffvk9jsm8wve1pgsd";
        let modified = "01knxf6yj2tdgj4k8kz70pc1xm";
        let created = "01knxfeb2hgmybg39ajhec0b9h";

        write_cache_file(
            &cache_path,
            &HashMap::from([
                (
                    removed.to_string(),
                    blake3::hash(b"removed\n").to_hex().to_string(),
                ),
                (
                    modified.to_string(),
                    blake3::hash(b"before\n").to_hex().to_string(),
                ),
            ]),
        )
        .unwrap();

        fs::write(dir.path().join(modified), b"after\n").unwrap();
        fs::write(dir.path().join(created), b"created\n").unwrap();

        let mut lines = Vec::new();
        let hashes =
            reconcile_startup_state(dir.path(), &cache_path, |line| lines.push(line.to_string()))
                .unwrap();

        assert_eq!(
            lines,
            vec![
                format!("{} {} -", removed, blake3::hash(b"removed\n").to_hex()),
                format!(
                    "{} {} {}",
                    modified,
                    blake3::hash(b"before\n").to_hex(),
                    blake3::hash(b"after\n").to_hex()
                ),
                format!("{} - {}", created, blake3::hash(b"created\n").to_hex()),
            ]
        );
        assert_eq!(
            hashes.get(modified),
            Some(&blake3::hash(b"after\n").to_hex().to_string())
        );
        assert_eq!(
            hashes.get(created),
            Some(&blake3::hash(b"created\n").to_hex().to_string())
        );
        assert!(!hashes.contains_key(removed));
    }

    #[test]
    fn cache_round_trips_with_rkyv() {
        let dir = temp_attr_dir();
        let cache_path = dir.path().join("daemon.cache");
        let hashes = HashMap::from([(
            "01knxf1n5ffvk9jsm8wve1pgsd".to_string(),
            blake3::hash(b"id=1\n").to_hex().to_string(),
        )]);

        write_cache_file(&cache_path, &hashes).unwrap();
        let restored = read_cache_file(&cache_path).unwrap().unwrap();

        assert_eq!(restored, hashes);
    }

    #[test]
    fn handle_event_logs_hash_transition() {
        let dir = temp_attr_dir();
        let id = "01knxf1n5ffvk9jsm8wve1pgsd";
        let path = dir.path().join(id);
        fs::write(&path, b"first\n").unwrap();

        let mut hashes = build_hash_map(dir.path()).unwrap();
        fs::write(&path, b"second\n").unwrap();

        let lines = process_event_for_test(
            dir.path(),
            &mut hashes,
            Event {
                kind: EventKind::Modify(ModifyKind::Data(DataChange::Content)),
                paths: vec![path.clone()],
                attrs: Default::default(),
            },
        )
        .unwrap();

        assert_eq!(lines.len(), 1);
        let old = blake3::hash(b"first\n").to_hex().to_string();
        let new = blake3::hash(b"second\n").to_hex().to_string();
        assert_eq!(lines[0], format!("{id} {old} {new}"));
        assert_eq!(hashes.get(id), Some(&new));
    }

    #[test]
    fn create_event_uses_dash_for_missing_old_hash() {
        let dir = temp_attr_dir();
        let id = "01knxf1n5ffvk9jsm8wve1pgsd";
        let path = dir.path().join(id);
        fs::write(&path, b"new\n").unwrap();

        let mut hashes = HashMap::new();
        let lines = process_event_for_test(
            dir.path(),
            &mut hashes,
            Event {
                kind: EventKind::Create(CreateKind::File),
                paths: vec![path],
                attrs: Default::default(),
            },
        )
        .unwrap();

        assert_eq!(lines.len(), 1);
        let new = blake3::hash(b"new\n").to_hex().to_string();
        assert_eq!(lines[0], format!("{id} - {new}"));
    }

    #[test]
    fn remove_event_logs_transition_and_drops_cached_hash() {
        let dir = temp_attr_dir();
        let id = "01knxf1n5ffvk9jsm8wve1pgsd";
        let path = dir.path().join(id);
        fs::write(&path, b"gone\n").unwrap();
        let mut hashes = build_hash_map(dir.path()).unwrap();
        fs::remove_file(&path).unwrap();

        let lines = process_event_for_test(
            dir.path(),
            &mut hashes,
            Event {
                kind: EventKind::Remove(RemoveKind::File),
                paths: vec![path],
                attrs: Default::default(),
            },
        )
        .unwrap();

        let old = blake3::hash(b"gone\n").to_hex().to_string();
        assert_eq!(lines, vec![format!("{id} {old} -")]);
        assert!(!hashes.contains_key(id));
    }

    #[test]
    fn rename_events_are_processed() {
        let dir = temp_attr_dir();
        let id = "01knxf1n5ffvk9jsm8wve1pgsd";
        let path = dir.path().join(id);
        fs::write(&path, b"renamed\n").unwrap();

        let mut hashes = HashMap::new();
        let lines = process_event_for_test(
            dir.path(),
            &mut hashes,
            Event {
                kind: EventKind::Modify(ModifyKind::Name(RenameMode::To)),
                paths: vec![path],
                attrs: Default::default(),
            },
        )
        .unwrap();

        assert_eq!(lines.len(), 1);
    }

    fn process_event_for_test(
        attr_dir: &Path,
        hashes: &mut HashMapById,
        event: Event,
    ) -> io::Result<Vec<String>> {
        let mut lines = Vec::new();
        let _ = process_event(attr_dir, hashes, event, |line| lines.push(line.to_string()))?;
        Ok(lines)
    }
}
