use crate::cache::read_cache_file;
use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Event, EventKind};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering as CmpOrdering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub type Blake3Hash = [u8; 32];
pub const TOMBSTONE_HASH: Blake3Hash = [0; 32];
pub type HashMapById = HashMap<String, SnapshotEntry>;

#[derive(
    Clone,
    Debug,
    Eq,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub struct SnapshotEntry {
    pub hash: Blake3Hash,
    pub lamport: u64,
    pub changed_at_ms: u64,
    pub origin: String,
}

#[derive(
    Clone,
    Debug,
    Eq,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub struct ReplicatedEntry {
    pub ulid: String,
    pub hash: Blake3Hash,
    pub lamport: u64,
    pub changed_at_ms: u64,
    pub contents: Option<Vec<u8>>,
    pub origin: String,
}

#[derive(
    Clone,
    Debug,
    Eq,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub struct SnapshotMetaEntry {
    pub ulid: String,
    pub hash: Blake3Hash,
    pub lamport: u64,
    pub changed_at_ms: u64,
    pub origin: String,
}

#[derive(
    Clone,
    Debug,
    Eq,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub struct SnapshotBucketSummary {
    pub bucket: String,
    pub count: u32,
    pub hash: Blake3Hash,
}

pub fn reconcile_startup_state<F>(
    attr_dir: &Path,
    cache_path: &Path,
    local_origin: &str,
    mut emit: F,
) -> io::Result<HashMapById>
where
    F: FnMut(&str, SnapshotEntry),
{
    let cached = read_cache_file(cache_path, 6)?;
    let mut current = build_snapshot_map(attr_dir, local_origin)?;
    if let Some(cached) = cached {
        let mut next_lamport = cached.values().map(|entry| entry.lamport).max().unwrap_or(0);
        let tombstone_ts = unix_timestamp_ms()?;
        for (id, current_entry) in &mut current {
            if let Some(old_entry) = cached.get(id) {
                if old_entry.hash == current_entry.hash {
                    *current_entry = old_entry.clone();
                    continue;
                }
            }
            next_lamport = next_lamport.saturating_add(1);
            current_entry.lamport = next_lamport;
            current_entry.origin = local_origin.to_string();
        }
        for (id, old_entry) in &cached {
            if !current.contains_key(id) && old_entry.hash != TOMBSTONE_HASH {
                current.insert(
                    id.clone(),
                    SnapshotEntry {
                        hash: TOMBSTONE_HASH,
                        lamport: next_lamport.saturating_add(1),
                        changed_at_ms: tombstone_ts,
                        origin: local_origin.to_string(),
                    },
                );
                next_lamport = next_lamport.saturating_add(1);
            }
        }
        emit_differences(&cached, &current, &mut emit);
    }
    Ok(current)
}

pub fn canonical_path(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

pub fn build_snapshot_map(attr_dir: &Path, local_origin: &str) -> io::Result<HashMapById> {
    let attr_dir = canonical_path(attr_dir.to_path_buf());
    let read_dir = match fs::read_dir(&attr_dir) {
        Ok(read_dir) => read_dir,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(err) => return Err(err),
    };

    let mut entries = HashMap::new();
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        let Some(id) = attr_id_from_path(&attr_dir, &path) else {
            continue;
        };
        if let Some(hash) = hash_file_if_present(&path)? {
            entries.insert(
                id,
                SnapshotEntry {
                    hash,
                    lamport: 0,
                    changed_at_ms: file_changed_at_ms(&path)?,
                    origin: local_origin.to_string(),
                },
            );
        }
    }
    Ok(entries)
}

pub fn file_changed_at_ms(path: &Path) -> io::Result<u64> {
    match fs::metadata(path).and_then(|meta| meta.modified()) {
        Ok(modified) => Ok(system_time_to_ms(modified)?),
        Err(err) if err.kind() == io::ErrorKind::NotFound => unix_timestamp_ms(),
        Err(err) => Err(err),
    }
}

pub fn system_time_to_ms(value: SystemTime) -> io::Result<u64> {
    let duration = value.duration_since(UNIX_EPOCH).map_err(io::Error::other)?;
    Ok(duration.as_millis() as u64)
}

fn should_process_event_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Any
            | EventKind::Other
            | EventKind::Create(CreateKind::Any | CreateKind::File)
            | EventKind::Modify(
                ModifyKind::Any
                    | ModifyKind::Data(_)
                    | ModifyKind::Metadata(_)
                    | ModifyKind::Name(_)
            )
            | EventKind::Remove(RemoveKind::Any | RemoveKind::File)
    )
}

pub fn should_rescan_event(attr_dir: &Path, event: &Event) -> bool {
    let attr_dir = canonical_path(attr_dir.to_path_buf());
    event
        .paths
        .iter()
        .map(|path| canonical_path(path.clone()))
        .any(|path| path == attr_dir)
}

fn unique_attr_paths(attr_dir: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    let attr_dir = canonical_path(attr_dir.to_path_buf());
    let mut out = Vec::new();
    for path in paths {
        let path = canonical_path(path.clone());
        if path.parent() != Some(&attr_dir) {
            continue;
        }
        if out.iter().any(|existing| existing == &path) {
            continue;
        }
        out.push(path);
    }
    out
}

fn attr_id_from_path(attr_dir: &Path, path: &Path) -> Option<String> {
    let attr_dir = canonical_path(attr_dir.to_path_buf());
    let path = canonical_path(path.to_path_buf());
    if path.parent() != Some(&attr_dir) {
        return None;
    }
    let id = path.file_name()?.to_str()?.to_ascii_lowercase();
    is_full_entry_id(&id).then_some(id)
}

fn hash_file_if_present(path: &Path) -> io::Result<Option<Blake3Hash>> {
    match fs::read(path) {
        Ok(data) => Ok(Some(*blake3::hash(&data).as_bytes())),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn read_local_attr_snapshot(path: &Path) -> io::Result<Option<(Vec<u8>, Blake3Hash, u64)>> {
    match fs::read(path) {
        Ok(data) => {
            let hash = *blake3::hash(&data).as_bytes();
            let changed_at_ms = file_changed_at_ms(path)?;
            Ok(Some((data, hash, changed_at_ms)))
        }
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

pub fn process_local_event(
    attr_dir: &Path,
    lamport: &mut u64,
    entries: &mut HashMapById,
    suppressed: &mut HashMap<String, SnapshotEntry>,
    local_origin: &str,
    event: Event,
) -> io::Result<Vec<ReplicatedEntry>> {
    if !should_process_event_kind(&event.kind) {
        return Ok(Vec::new());
    }
    if should_rescan_event(attr_dir, &event) {
        return rescan_local_state(attr_dir, lamport, entries, suppressed, local_origin);
    }
    let mut out = Vec::new();
    for path in unique_attr_paths(attr_dir, &event.paths) {
        let Some(id) = attr_id_from_path(attr_dir, &path) else {
            continue;
        };
        let local_state = read_local_attr_snapshot(&path)?;
        let observed_hash = local_state
            .as_ref()
            .map(|(_, hash, _)| *hash)
            .unwrap_or(TOMBSTONE_HASH);
        if let Some(expected) = suppressed.get(&id).cloned() {
            if expected.hash == observed_hash {
                entries.insert(id.clone(), expected);
                suppressed.remove(&id);
                continue;
            }
        }
        if entries.get(&id).map(|entry| entry.hash) == Some(observed_hash) {
            continue;
        }
        *lamport = lamport.saturating_add(1);
        let observed_changed_at_ms = local_state
            .as_ref()
            .map(|(_, _, changed_at_ms)| *changed_at_ms)
            .unwrap_or(unix_timestamp_ms()?);
        let previous_changed_at_ms = entries.get(&id).map(|entry| entry.changed_at_ms).unwrap_or(0);
        let entry = SnapshotEntry {
            hash: observed_hash,
            lamport: *lamport,
            changed_at_ms: observed_changed_at_ms.max(previous_changed_at_ms.saturating_add(1)),
            origin: local_origin.to_string(),
        };
        entries.insert(id.clone(), entry.clone());
        out.push(ReplicatedEntry {
            ulid: id.clone(),
            hash: entry.hash,
            lamport: entry.lamport,
            changed_at_ms: entry.changed_at_ms,
            contents: local_state.as_ref().map(|(contents, _, _)| contents.clone()),
            origin: local_origin.to_string(),
        });
    }
    Ok(out)
}

pub fn rescan_local_state(
    attr_dir: &Path,
    lamport: &mut u64,
    entries: &mut HashMapById,
    suppressed: &mut HashMap<String, SnapshotEntry>,
    local_origin: &str,
) -> io::Result<Vec<ReplicatedEntry>> {
    let scanned = build_snapshot_map(attr_dir, local_origin)?;
    let mut ids: BTreeMap<String, ()> = BTreeMap::new();
    for id in entries.keys() {
        ids.insert(id.clone(), ());
    }
    for id in scanned.keys() {
        ids.insert(id.clone(), ());
    }

    let mut updates = Vec::new();
    for id in ids.into_keys() {
        let scanned_entry = scanned.get(&id).cloned();
        let observed_hash = scanned_entry.as_ref().map(|entry| entry.hash).unwrap_or(TOMBSTONE_HASH);
        if let Some(expected) = suppressed.get(&id).cloned() {
            if expected.hash == observed_hash {
                entries.insert(id.clone(), expected);
                suppressed.remove(&id);
                continue;
            }
        }
        if entries.get(&id).map(|entry| entry.hash) == Some(observed_hash) {
            continue;
        }
        *lamport = lamport.saturating_add(1);
        let previous_changed_at_ms = entries.get(&id).map(|entry| entry.changed_at_ms).unwrap_or(0);
        let entry = if let Some(scanned_entry) = scanned_entry {
            SnapshotEntry {
                hash: scanned_entry.hash,
                lamport: *lamport,
                changed_at_ms: scanned_entry
                    .changed_at_ms
                    .max(previous_changed_at_ms.saturating_add(1)),
                origin: local_origin.to_string(),
            }
        } else {
            SnapshotEntry {
                hash: TOMBSTONE_HASH,
                lamport: *lamport,
                changed_at_ms: unix_timestamp_ms()?.max(previous_changed_at_ms.saturating_add(1)),
                origin: local_origin.to_string(),
            }
        };
        entries.insert(id.clone(), entry.clone());
        updates.push(build_replication_entry(attr_dir, &id, entry)?);
    }
    Ok(updates)
}

pub fn apply_remote_entries<F>(
    attr_dir: &Path,
    lamport: &mut u64,
    entries: &mut HashMapById,
    suppressed: &mut HashMap<String, SnapshotEntry>,
    local_origin: &str,
    remote_entries: Vec<ReplicatedEntry>,
    mut emit: F,
) -> io::Result<Vec<ReplicatedEntry>>
where
    F: FnMut(&str, SnapshotEntry),
{
    let mut accepted = Vec::new();
    for entry in remote_entries {
        if entry.origin == local_origin {
            continue;
        }
        let remote_snapshot = snapshot_from_wire(&entry);
        let current = entries.get(&entry.ulid).cloned();
        if !should_accept_remote(current, remote_snapshot.clone()) {
            continue;
        }
        apply_remote_entry_to_disk(attr_dir, &entry)?;
        *lamport = (*lamport).max(remote_snapshot.lamport);
        entries.insert(entry.ulid.clone(), remote_snapshot.clone());
        suppressed.insert(entry.ulid.clone(), remote_snapshot.clone());
        emit(&entry.ulid, remote_snapshot);
        accepted.push(entry);
    }
    Ok(accepted)
}

pub fn should_accept_remote(current: Option<SnapshotEntry>, remote: SnapshotEntry) -> bool {
    match current {
        None => true,
        Some(current) => compare_snapshot(remote, current) == CmpOrdering::Greater,
    }
}

pub fn compare_snapshot(left: SnapshotEntry, right: SnapshotEntry) -> CmpOrdering {
    left.lamport
        .cmp(&right.lamport)
        .then_with(|| left.changed_at_ms.cmp(&right.changed_at_ms))
        .then_with(|| left.origin.cmp(&right.origin))
        .then_with(|| left.hash.cmp(&right.hash))
}

pub fn apply_remote_entry_to_disk(attr_dir: &Path, entry: &ReplicatedEntry) -> io::Result<()> {
    let path = attr_dir.join(&entry.ulid);
    if entry.hash == TOMBSTONE_HASH {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    } else {
        let Some(contents) = entry.contents.as_ref() else {
            return Err(io::Error::other("remote snapshot missing attr contents"));
        };
        let hash = *blake3::hash(contents).as_bytes();
        if hash != entry.hash {
            return Err(io::Error::other("remote snapshot hash/content mismatch"));
        }
        fs::write(path, contents)
    }
}

pub fn collect_missing_snapshot_ids_from_state(
    local_entries: &HashMapById,
    remote_entries: Vec<SnapshotMetaEntry>,
) -> Vec<String> {
    remote_entries
        .into_iter()
        .filter_map(|entry| {
            let remote_snapshot = snapshot_meta_to_snapshot(&entry);
            let current = local_entries.get(&entry.ulid).cloned();
            should_accept_remote(current, remote_snapshot).then_some(entry.ulid)
        })
        .collect()
}

pub fn collect_missing_snapshot_buckets_from_state(
    attr_dir: &Path,
    lamport: &mut u64,
    entries: &mut HashMapById,
    local_origin: &str,
    remote_buckets: Vec<SnapshotBucketSummary>,
) -> io::Result<Vec<String>> {
    if remote_buckets.is_empty() {
        return Ok(Vec::new());
    }
    let level = remote_buckets.first().map(|bucket| bucket.bucket.len() as u8).unwrap_or(6);
    let local_buckets =
        collect_snapshot_bucket_summaries(attr_dir, lamport, entries, local_origin, level, &[])?;
    let local_map: HashMap<String, SnapshotBucketSummary> = local_buckets
        .into_iter()
        .map(|bucket| (bucket.bucket.clone(), bucket))
        .collect();
    Ok(remote_buckets
        .into_iter()
        .filter_map(|bucket| (local_map.get(&bucket.bucket) != Some(&bucket)).then_some(bucket.bucket))
        .collect())
}

pub fn collect_snapshot_bucket_summaries(
    attr_dir: &Path,
    lamport: &mut u64,
    entries: &mut HashMapById,
    local_origin: &str,
    level: u8,
    parents: &[String],
) -> io::Result<Vec<SnapshotBucketSummary>> {
    let metadata_entries = build_snapshot_meta_for_buckets(attr_dir, lamport, entries, local_origin, parents)?;
    let mut grouped: BTreeMap<String, Vec<SnapshotMetaEntry>> = BTreeMap::new();
    for entry in metadata_entries {
        grouped.entry(bucket_for_ulid(&entry.ulid, level)).or_default().push(entry);
    }

    let mut out = Vec::with_capacity(grouped.len());
    for (bucket, mut bucket_entries) in grouped {
        bucket_entries.sort_by(|left, right| left.ulid.cmp(&right.ulid));
        let mut hasher = blake3::Hasher::new();
        for entry in &bucket_entries {
            hasher.update(entry.ulid.as_bytes());
            hasher.update(&[0]);
            hasher.update(&entry.hash);
            hasher.update(&[0]);
            hasher.update(&entry.lamport.to_le_bytes());
            hasher.update(&[0]);
            hasher.update(&entry.changed_at_ms.to_le_bytes());
            hasher.update(&[0]);
            hasher.update(entry.origin.as_bytes());
            hasher.update(&[b'\n']);
        }
        out.push(SnapshotBucketSummary {
            bucket,
            count: bucket_entries.len() as u32,
            hash: *hasher.finalize().as_bytes(),
        });
    }
    Ok(out)
}

pub fn build_requested_snapshot_entries(
    attr_dir: &Path,
    lamport: &mut u64,
    entries: &mut HashMapById,
    local_origin: &str,
    ids: &[String],
) -> io::Result<Vec<ReplicatedEntry>> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(entry) = entries.get(id).cloned() else {
            continue;
        };
        match build_replication_entry(attr_dir, id, entry.clone()) {
            Ok(replication_entry) => out.push(replication_entry),
            Err(err) if err.kind() == io::ErrorKind::NotFound && entry.hash != TOMBSTONE_HASH => {
                *lamport = lamport.saturating_add(1);
                let tombstone = SnapshotEntry {
                    hash: TOMBSTONE_HASH,
                    lamport: *lamport,
                    changed_at_ms: unix_timestamp_ms()?,
                    origin: local_origin.to_string(),
                };
                entries.insert(id.clone(), tombstone.clone());
                out.push(build_replication_entry(attr_dir, id, tombstone)?);
            }
            Err(err) => return Err(err),
        }
    }
    Ok(out)
}

pub fn build_snapshot_meta_for_buckets(
    attr_dir: &Path,
    lamport: &mut u64,
    entries: &mut HashMapById,
    local_origin: &str,
    buckets: &[String],
) -> io::Result<Vec<SnapshotMetaEntry>> {
    let bucket_filter: Option<HashSet<&str>> =
        (!buckets.is_empty()).then(|| buckets.iter().map(String::as_str).collect());
    let mut out = Vec::with_capacity(entries.len());
    let ids: Vec<String> = entries.keys().cloned().collect();
    for id in ids {
        if let Some(filter) = bucket_filter.as_ref() {
            if !matches_bucket_prefixes(&id, filter) {
                continue;
            }
        }
        let Some(entry) = entries.get(&id).cloned() else {
            continue;
        };
        match build_snapshot_meta_entry(attr_dir, &id, entry.clone()) {
            Ok(snapshot_entry) => out.push(snapshot_entry),
            Err(err) if err.kind() == io::ErrorKind::NotFound && entry.hash != TOMBSTONE_HASH => {
                *lamport = lamport.saturating_add(1);
                let tombstone = SnapshotEntry {
                    hash: TOMBSTONE_HASH,
                    lamport: *lamport,
                    changed_at_ms: unix_timestamp_ms()?,
                    origin: local_origin.to_string(),
                };
                entries.insert(id.clone(), tombstone.clone());
                out.push(build_snapshot_meta_entry(attr_dir, &id, tombstone)?);
            }
            Err(err) => return Err(err),
        }
    }
    Ok(out)
}

pub fn build_snapshot_meta_entry(
    attr_dir: &Path,
    id: &str,
    entry: SnapshotEntry,
) -> io::Result<SnapshotMetaEntry> {
    if entry.hash != TOMBSTONE_HASH {
        fs::metadata(attr_dir.join(id))?;
    }
    Ok(SnapshotMetaEntry {
        ulid: id.to_string(),
        hash: entry.hash,
        lamport: entry.lamport,
        changed_at_ms: entry.changed_at_ms,
        origin: entry.origin,
    })
}

pub fn build_replication_entry(
    attr_dir: &Path,
    id: &str,
    entry: SnapshotEntry,
) -> io::Result<ReplicatedEntry> {
    let contents = if entry.hash == TOMBSTONE_HASH {
        None
    } else {
        Some(fs::read(attr_dir.join(id))?)
    };
    Ok(ReplicatedEntry {
        ulid: id.to_string(),
        hash: entry.hash,
        lamport: entry.lamport,
        changed_at_ms: entry.changed_at_ms,
        contents,
        origin: entry.origin,
    })
}

pub fn snapshot_from_wire(entry: &ReplicatedEntry) -> SnapshotEntry {
    SnapshotEntry {
        hash: entry.hash,
        lamport: entry.lamport,
        changed_at_ms: entry.changed_at_ms,
        origin: entry.origin.clone(),
    }
}

pub fn snapshot_meta_to_snapshot(entry: &SnapshotMetaEntry) -> SnapshotEntry {
    SnapshotEntry {
        hash: entry.hash,
        lamport: entry.lamport,
        changed_at_ms: entry.changed_at_ms,
        origin: entry.origin.clone(),
    }
}

fn matches_bucket_prefixes(id: &str, prefixes: &HashSet<&str>) -> bool {
    prefixes.iter().any(|prefix| id.starts_with(prefix))
}

pub fn bucket_for_ulid(id: &str, level: u8) -> String {
    let len = usize::from(level).min(id.len());
    id[..len].to_string()
}

fn emit_differences<F>(old: &HashMapById, new: &HashMapById, emit: &mut F)
where
    F: FnMut(&str, SnapshotEntry),
{
    let mut ids: BTreeMap<String, ()> = BTreeMap::new();
    for id in old.keys() {
        ids.insert(id.clone(), ());
    }
    for id in new.keys() {
        ids.insert(id.clone(), ());
    }
    for id in ids.keys() {
        let old_entry = old.get(id);
        let new_entry = new.get(id);
        if old_entry.map(|entry| entry.hash) != new_entry.map(|entry| entry.hash) {
            if let Some(new_entry) = new_entry {
                emit(id, new_entry.clone());
            }
        }
    }
}

pub fn unix_timestamp_ms() -> io::Result<u64> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    Ok(now.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::write_cache_file;
    use tempfile::TempDir;

    fn temp_attr_dir() -> TempDir {
        tempfile::Builder::new().prefix("stashd-attr").tempdir().unwrap()
    }

    fn hex_hash(hash: Blake3Hash) -> String {
        hash.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn build_snapshot_map_uses_mtime_for_scan() {
        let dir = temp_attr_dir();
        let id = "01knxf1n5ffvk9jsm8wve1pgsd";
        fs::write(dir.path().join(id), b"id=1\n").unwrap();
        let entries = build_snapshot_map(dir.path(), "peer-a").unwrap();
        let entry = entries.get(id).unwrap();
        assert_eq!(entry.hash, *blake3::hash(b"id=1\n").as_bytes());
        assert_eq!(entry.lamport, 0);
        assert!(entry.changed_at_ms > 0);
        assert_eq!(entry.origin, "peer-a");
    }

    #[test]
    fn startup_reconcile_creates_tombstone_for_removed_cached_entry() {
        let dir = temp_attr_dir();
        let cache_path = dir.path().join("daemon.cache");
        let removed = "01knxf1n5ffvk9jsm8wve1pgsd".to_string();
        write_cache_file(
            &cache_path,
            &HashMap::from([(
                removed.clone(),
                SnapshotEntry {
                    hash: *blake3::hash(b"old\n").as_bytes(),
                    lamport: 7,
                    changed_at_ms: 7,
                    origin: "peer-a".into(),
                },
            )]),
            6,
        )
        .unwrap();
        let mut lines = Vec::new();
        let entries = reconcile_startup_state(dir.path(), &cache_path, "peer-a", |id, entry| {
            lines.push(format!("{} {} {}", entry.changed_at_ms, id, hex_hash(entry.hash)));
        })
        .unwrap();
        let entry = entries.get(&removed).unwrap();
        assert_eq!(entry.hash, TOMBSTONE_HASH);
        assert!(entry.changed_at_ms > 0);
        assert!(entry.lamport > 0);
        assert_eq!(entry.origin, "peer-a");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn local_event_suppresses_remote_apply_echo() {
        let dir = temp_attr_dir();
        let id = "01knxf1n5ffvk9jsm8wve1pgsd".to_string();
        fs::write(dir.path().join(&id), b"remote\n").unwrap();
        let remote = SnapshotEntry {
            hash: *blake3::hash(b"remote\n").as_bytes(),
            lamport: 4,
            changed_at_ms: 42,
            origin: "peer-b".into(),
        };
        let mut entries = HashMap::from([(id.clone(), remote.clone())]);
        let mut suppressed = HashMap::from([(id.clone(), remote.clone())]);
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![dir.path().join(&id)],
            attrs: Default::default(),
        };
        let mut lamport = 4;
        let updates =
            process_local_event(dir.path(), &mut lamport, &mut entries, &mut suppressed, "peer-a", event)
                .unwrap();
        assert!(updates.is_empty());
        assert!(suppressed.is_empty());
        assert_eq!(entries.get(&id), Some(&remote));
    }

    #[test]
    fn directory_event_rescans_new_attr_files() {
        let dir = temp_attr_dir();
        let id = "01knxf1n5ffvk9jsm8wve1pgsd".to_string();
        fs::write(dir.path().join(&id), b"kind=test\n").unwrap();
        let event = Event {
            kind: EventKind::Create(CreateKind::Any),
            paths: vec![dir.path().to_path_buf()],
            attrs: Default::default(),
        };
        let mut lamport = 0;
        let mut entries = HashMap::new();
        let mut suppressed = HashMap::new();
        let updates =
            process_local_event(dir.path(), &mut lamport, &mut entries, &mut suppressed, "peer-a", event)
                .unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].ulid, id);
        assert_eq!(updates[0].contents.as_deref(), Some(b"kind=test\n".as_slice()));
        assert_eq!(entries.len(), 1);
        assert!(lamport > 0);
    }

    #[test]
    fn remote_newer_snapshot_wins_by_timestamp() {
        let current = SnapshotEntry {
            hash: *blake3::hash(b"old\n").as_bytes(),
            lamport: 1,
            changed_at_ms: 1,
            origin: "peer-a".into(),
        };
        let remote = SnapshotEntry {
            hash: *blake3::hash(b"new\n").as_bytes(),
            lamport: 2,
            changed_at_ms: 2,
            origin: "peer-b".into(),
        };
        assert!(should_accept_remote(Some(current), remote));
    }

    #[test]
    fn equal_lamport_uses_changed_at_ms_before_origin_and_hash() {
        let older = SnapshotEntry {
            hash: [9; 32],
            lamport: 5,
            changed_at_ms: 10,
            origin: "peer-z".into(),
        };
        let newer = SnapshotEntry {
            hash: [1; 32],
            lamport: 5,
            changed_at_ms: 11,
            origin: "peer-a".into(),
        };
        assert_eq!(compare_snapshot(newer, older), CmpOrdering::Greater);
    }

    #[test]
    fn equal_lamport_and_changed_at_uses_origin_then_hash() {
        let low_origin = SnapshotEntry {
            hash: [1; 32],
            lamport: 5,
            changed_at_ms: 10,
            origin: "peer-a".into(),
        };
        let high_origin = SnapshotEntry {
            hash: [1; 32],
            lamport: 5,
            changed_at_ms: 10,
            origin: "peer-b".into(),
        };
        assert_eq!(compare_snapshot(high_origin, low_origin), CmpOrdering::Greater);
    }
}
