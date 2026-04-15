use clap::Parser;
use iroh::{Endpoint, EndpointAddr, PublicKey, SecretKey, Watcher};
use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use serde::{Deserialize, Serialize};
use signal_hook::consts::signal::SIGTERM;
use signal_hook::flag;
use std::cmp::Ordering as CmpOrdering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

use crate::store;

const ALPN: &[u8] = b"stashd/snapshot/1";
const DAEMON_CACHE_VERSION: u32 = 5;
const RESYNC_INTERVAL: Duration = Duration::from_secs(30);
const TOMBSTONE_HASH: Blake3Hash = [0; 32];

pub type Blake3Hash = [u8; 32];
type HashMapById = HashMap<String, SnapshotEntry>;

#[derive(Parser, Debug, Clone)]
#[command(name = "stashd", about = "Replicate stash attr snapshots over iroh")]
struct Cli {
    #[arg(
        long = "peer",
        value_name = "ENDPOINT_ADDR_JSON",
        value_parser = parse_endpoint_addr,
        action = clap::ArgAction::Append,
        help = "Static peer EndpointAddr encoded as JSON"
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
}

#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
struct SnapshotEntry {
    hash: Blake3Hash,
    changed_at_ms: u64,
}

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
struct ReplicatedEntry {
    ulid: String,
    hash: Blake3Hash,
    changed_at_ms: u64,
    contents: Option<Vec<u8>>,
    origin: String,
}

#[derive(
    Debug,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
enum RequestMessage {
    SnapshotSync { from: String, entries: Vec<ReplicatedEntry> },
    LiveEvent { from: String, entry: ReplicatedEntry },
}

#[derive(
    Debug,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
enum ResponseMessage {
    SnapshotSync { entries: Vec<ReplicatedEntry> },
    Ack,
}

enum Command {
    LocalFs(Event),
    ApplyRemote {
        peer: PublicKey,
        entries: Vec<ReplicatedEntry>,
        respond_to: Option<oneshot::Sender<Vec<ReplicatedEntry>>>,
    },
    SyncPeer(EndpointAddr),
}

struct State {
    attr_dir: PathBuf,
    cache_path: PathBuf,
    endpoint: Endpoint,
    local_origin: String,
    peers: Vec<EndpointAddr>,
    entries: HashMapById,
    suppressed: HashMap<String, SnapshotEntry>,
}

pub fn run() -> io::Result<()> {
    let cli = Cli::parse();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(io::Error::other)?;
    runtime.block_on(run_async(cli))
}

async fn run_async(cli: Cli) -> io::Result<()> {
    store::init()?;
    let attr_dir = store::attr_dir()?;
    let cache_path = daemon_cache_path()?;
    let key_path = cli.key_file.unwrap_or(default_key_path()?);
    let secret_key = load_or_create_secret_key(&key_path)?;
    let endpoint = Endpoint::builder()
        .alpns(vec![ALPN.to_vec()])
        .secret_key(secret_key)
        .bind()
        .await
        .map_err(io::Error::other)?;

    print_node_info(&endpoint)?;

    let mut allowlist: HashSet<PublicKey> = cli.allow_peers.into_iter().collect();
    for peer in &cli.peers {
        allowlist.insert(peer.id);
    }
    let local_origin = endpoint.id().to_string();

    let terminated = Arc::new(AtomicBool::new(false));
    flag::register(SIGTERM, Arc::clone(&terminated)).map_err(io::Error::other)?;

    let entries = reconcile_startup_state(&attr_dir, &cache_path, emit_snapshot)?;
    write_cache_file(&cache_path, &entries)?;

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();

    let watch_tx = cmd_tx.clone();
    let mut watcher = RecommendedWatcher::new(
        move |result| match result {
            Ok(event) => {
                let _ = watch_tx.send(Command::LocalFs(event));
            }
            Err(err) => eprintln!("watch error: {err}"),
        },
        Config::default(),
    )
    .map_err(io::Error::other)?;
    watcher
        .watch(&attr_dir, RecursiveMode::NonRecursive)
        .map_err(io::Error::other)?;

    let accept_tx = cmd_tx.clone();
    let accept_allowlist = Arc::new(allowlist);
    let accept_endpoint = endpoint.clone();
    tokio::spawn(async move {
        loop {
            let Some(connecting) = accept_endpoint.accept().await else {
                break;
            };
            let accept_tx = accept_tx.clone();
            let accept_allowlist = Arc::clone(&accept_allowlist);
            tokio::spawn(async move {
                if let Err(err) = handle_incoming(connecting, accept_tx, accept_allowlist).await {
                    eprintln!("incoming sync error: {err}");
                }
            });
        }
    });

    let sync_tx = cmd_tx.clone();
    let sync_peers = cli.peers.clone();
    tokio::spawn(async move {
        for peer in &sync_peers {
            let _ = sync_tx.send(Command::SyncPeer(peer.clone()));
        }
        let mut interval = tokio::time::interval(RESYNC_INTERVAL);
        loop {
            interval.tick().await;
            for peer in &sync_peers {
                let _ = sync_tx.send(Command::SyncPeer(peer.clone()));
            }
        }
    });

    let mut state = State {
        attr_dir,
        cache_path,
        endpoint,
        local_origin,
        peers: cli.peers,
        entries,
        suppressed: HashMap::new(),
    };

    while !terminated.load(Ordering::Relaxed) {
        let Some(command) = cmd_rx.recv().await else {
            break;
        };
        match command {
            Command::LocalFs(event) => {
                let updates =
                    process_local_event(
                        &state.attr_dir,
                        &mut state.entries,
                        &mut state.suppressed,
                        &state.local_origin,
                        event,
                    )?;
                if updates.is_empty() {
                    continue;
                }
                for update in &updates {
                    emit_snapshot(&update.ulid, snapshot_from_wire(update));
                }
                write_cache_file(&state.cache_path, &state.entries)?;
                broadcast_entries(&state.endpoint, &state.peers, &updates, None).await;
            }
            Command::ApplyRemote {
                peer,
                entries,
                respond_to,
            } => {
                let updates =
                    apply_remote_entries(&state.attr_dir, &mut state.entries, &mut state.suppressed, &state.local_origin, entries)?;
                if !updates.is_empty() {
                    write_cache_file(&state.cache_path, &state.entries)?;
                    broadcast_entries(&state.endpoint, &state.peers, &updates, Some(peer)).await;
                }
                if let Some(reply) = respond_to {
                    let snapshot =
                        collect_network_snapshot(&state.attr_dir, &state.entries, &state.local_origin)?;
                    let _ = reply.send(snapshot);
                }
            }
            Command::SyncPeer(peer) => {
                let snapshot =
                    collect_network_snapshot(&state.attr_dir, &state.entries, &state.local_origin)?;
                let tx = cmd_tx.clone();
                let endpoint = state.endpoint.clone();
                tokio::spawn(async move {
                    if let Err(err) = sync_peer(endpoint, peer, tx, snapshot).await {
                        eprintln!("peer sync error: {err}");
                    }
                });
            }
        }
    }

    write_cache_file(&state.cache_path, &state.entries)?;
    Ok(())
}

fn parse_endpoint_addr(input: &str) -> Result<EndpointAddr, String> {
    serde_json::from_str(input).map_err(|err| format!("invalid peer JSON: {err}"))
}

fn default_key_path() -> io::Result<PathBuf> {
    Ok(store::base_dir()?.join("cache").join("iroh.key"))
}

fn load_or_create_secret_key(path: &Path) -> io::Result<SecretKey> {
    if let Ok(bytes) = fs::read(path) {
        let array: [u8; 32] = bytes
            .try_into()
            .map_err(|_| io::Error::other("invalid iroh key file length"))?;
        return Ok(SecretKey::from_bytes(&array));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = [0u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    fs::write(path, bytes)?;
    Ok(SecretKey::from_bytes(&bytes))
}

fn print_node_info(endpoint: &Endpoint) -> io::Result<()> {
    let addr = endpoint.watch_addr().get();
    let addr_json = serde_json::to_string(&addr).map_err(io::Error::other)?;
    eprintln!("stashd node-id {}", endpoint.id());
    eprintln!("stashd peer {addr_json}");
    Ok(())
}

async fn handle_incoming(
    incoming: iroh::endpoint::Incoming,
    tx: mpsc::UnboundedSender<Command>,
    allowlist: Arc<HashSet<PublicKey>>,
) -> io::Result<()> {
    let connection = incoming.await.map_err(io::Error::other)?;
    let peer = connection.remote_id();
    if !allowlist.contains(&peer) {
        return Ok(());
    }

    let (mut send, mut recv) = connection.accept_bi().await.map_err(io::Error::other)?;
    let request: RequestMessage = read_frame(&mut recv).await?;
    match request {
        RequestMessage::SnapshotSync { entries, .. } => {
            let (reply_tx, reply_rx) = oneshot::channel();
            let _ = tx.send(Command::ApplyRemote {
                peer,
                entries,
                respond_to: Some(reply_tx),
            });
            let entries = reply_rx.await.unwrap_or_default();
            write_frame(&mut send, &ResponseMessage::SnapshotSync { entries }).await?;
        }
        RequestMessage::LiveEvent { entry, .. } => {
            let _ = tx.send(Command::ApplyRemote {
                peer,
                entries: vec![entry],
                respond_to: None,
            });
            write_frame(&mut send, &ResponseMessage::Ack).await?;
        }
    }
    Ok(())
}

async fn sync_peer(
    endpoint: Endpoint,
    peer: EndpointAddr,
    tx: mpsc::UnboundedSender<Command>,
    entries: Vec<ReplicatedEntry>,
) -> io::Result<()> {
    let connection = endpoint
        .connect(peer.clone(), ALPN)
        .await
        .map_err(io::Error::other)?;
    let (mut send, mut recv) = connection.open_bi().await.map_err(io::Error::other)?;
    write_frame(
        &mut send,
        &RequestMessage::SnapshotSync {
            from: endpoint.id().to_string(),
            entries,
        },
    )
    .await?;
    let response: ResponseMessage = read_frame(&mut recv).await?;
    if let ResponseMessage::SnapshotSync { entries } = response {
        let _ = tx.send(Command::ApplyRemote {
            peer: peer.id,
            entries,
            respond_to: None,
        });
    }
    Ok(())
}

async fn send_live_event(
    endpoint: Endpoint,
    peer: EndpointAddr,
    entry: ReplicatedEntry,
) -> io::Result<()> {
    let connection = endpoint
        .connect(peer, ALPN)
        .await
        .map_err(io::Error::other)?;
    let (mut send, mut recv) = connection.open_bi().await.map_err(io::Error::other)?;
    write_frame(
        &mut send,
        &RequestMessage::LiveEvent {
            from: endpoint.id().to_string(),
            entry,
        },
    )
    .await?;
    let _: ResponseMessage = read_frame(&mut recv).await?;
    Ok(())
}

async fn write_frame<T>(send: &mut iroh::endpoint::SendStream, value: &T) -> io::Result<()>
where
    T: Serialize,
{
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    send.write_u32(bytes.len() as u32).await?;
    send.write_all(&bytes).await?;
    Ok(())
}

async fn read_frame<T>(recv: &mut iroh::endpoint::RecvStream) -> io::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let len = recv.read_u32().await? as usize;
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await.map_err(io::Error::other)?;
    serde_json::from_slice(&buf).map_err(io::Error::other)
}

fn reconcile_startup_state<F>(
    attr_dir: &Path,
    cache_path: &Path,
    mut emit: F,
) -> io::Result<HashMapById>
where
    F: FnMut(&str, SnapshotEntry),
{
    let cached = read_cache_file(cache_path)?;
    let mut current = build_snapshot_map(attr_dir)?;
    if let Some(cached) = cached {
        let tombstone_ts = unix_timestamp_ms()?;
        for (id, old_entry) in &cached {
            if !current.contains_key(id) && old_entry.hash != TOMBSTONE_HASH {
                current.insert(
                    id.clone(),
                    SnapshotEntry {
                        hash: TOMBSTONE_HASH,
                        changed_at_ms: tombstone_ts,
                    },
                );
            }
        }
        emit_differences(&cached, &current, &mut emit);
    }
    Ok(current)
}

fn daemon_cache_path() -> io::Result<PathBuf> {
    Ok(store::base_dir()?.join("cache").join("daemon.cache"))
}

fn build_snapshot_map(attr_dir: &Path) -> io::Result<HashMapById> {
    let read_dir = match fs::read_dir(attr_dir) {
        Ok(read_dir) => read_dir,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(err) => return Err(err),
    };

    let mut entries = HashMap::new();
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        let Some(id) = attr_id_from_path(attr_dir, &path) else {
            continue;
        };
        if let Some(hash) = hash_file_if_present(&path)? {
            entries.insert(
                id,
                SnapshotEntry {
                    hash,
                    changed_at_ms: 0,
                },
            );
        }
    }
    Ok(entries)
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

fn hash_file_if_present(path: &Path) -> io::Result<Option<Blake3Hash>> {
    match fs::read(path) {
        Ok(data) => Ok(Some(*blake3::hash(&data).as_bytes())),
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

fn process_local_event(
    attr_dir: &Path,
    entries: &mut HashMapById,
    suppressed: &mut HashMap<String, SnapshotEntry>,
    local_origin: &str,
    event: Event,
) -> io::Result<Vec<ReplicatedEntry>> {
    if !should_process_event_kind(&event.kind) {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let changed_at_ms = unix_timestamp_ms()?;
    for path in unique_attr_paths(attr_dir, &event.paths) {
        let Some(id) = attr_id_from_path(attr_dir, &path) else {
            continue;
        };
        let observed_hash = hash_file_if_present(&path)?.unwrap_or(TOMBSTONE_HASH);
        if let Some(expected) = suppressed.get(&id).copied() {
            if expected.hash == observed_hash {
                entries.insert(id.clone(), expected);
                suppressed.remove(&id);
                continue;
            }
        }
        if entries.get(&id).map(|entry| entry.hash) == Some(observed_hash) {
            continue;
        }
        let entry = SnapshotEntry {
            hash: observed_hash,
            changed_at_ms,
        };
        entries.insert(id.clone(), entry);
        out.push(build_replication_entry(
            attr_dir,
            &id,
            entry,
            local_origin.to_string(),
        )?);
    }
    Ok(out)
}

fn apply_remote_entries(
    attr_dir: &Path,
    entries: &mut HashMapById,
    suppressed: &mut HashMap<String, SnapshotEntry>,
    local_origin: &str,
    remote_entries: Vec<ReplicatedEntry>,
) -> io::Result<Vec<ReplicatedEntry>> {
    let mut accepted = Vec::new();
    for entry in remote_entries {
        if entry.origin == local_origin {
            continue;
        }
        let remote_snapshot = snapshot_from_wire(&entry);
        let current = entries.get(&entry.ulid).copied();
        if !should_accept_remote(current, remote_snapshot) {
            continue;
        }
        apply_remote_entry_to_disk(attr_dir, &entry)?;
        entries.insert(entry.ulid.clone(), remote_snapshot);
        suppressed.insert(entry.ulid.clone(), remote_snapshot);
        emit_snapshot(&entry.ulid, remote_snapshot);
        accepted.push(entry);
    }
    Ok(accepted)
}

fn should_accept_remote(current: Option<SnapshotEntry>, remote: SnapshotEntry) -> bool {
    match current {
        None => true,
        Some(current) => compare_snapshot(remote, current) == CmpOrdering::Greater,
    }
}

fn compare_snapshot(left: SnapshotEntry, right: SnapshotEntry) -> CmpOrdering {
    left.changed_at_ms
        .cmp(&right.changed_at_ms)
        .then_with(|| left.hash.cmp(&right.hash))
}

fn apply_remote_entry_to_disk(attr_dir: &Path, entry: &ReplicatedEntry) -> io::Result<()> {
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

fn collect_network_snapshot(
    attr_dir: &Path,
    entries: &HashMapById,
    origin: &str,
) -> io::Result<Vec<ReplicatedEntry>> {
    let mut out = Vec::with_capacity(entries.len());
    for (id, entry) in entries {
        out.push(build_replication_entry(attr_dir, id, *entry, origin.to_string())?);
    }
    Ok(out)
}

fn build_replication_entry(
    attr_dir: &Path,
    id: &str,
    entry: SnapshotEntry,
    origin: String,
) -> io::Result<ReplicatedEntry> {
    let contents = if entry.hash == TOMBSTONE_HASH {
        None
    } else {
        Some(fs::read(attr_dir.join(id))?)
    };
    Ok(ReplicatedEntry {
        ulid: id.to_string(),
        hash: entry.hash,
        changed_at_ms: entry.changed_at_ms,
        contents,
        origin,
    })
}

fn snapshot_from_wire(entry: &ReplicatedEntry) -> SnapshotEntry {
    SnapshotEntry {
        hash: entry.hash,
        changed_at_ms: entry.changed_at_ms,
    }
}

async fn broadcast_entries(
    endpoint: &Endpoint,
    peers: &[EndpointAddr],
    entries: &[ReplicatedEntry],
    skip_peer: Option<PublicKey>,
) {
    for peer in peers {
        if skip_peer == Some(peer.id) {
            continue;
        }
        for entry in entries {
            let endpoint = endpoint.clone();
            let peer = peer.clone();
            let entry = entry.clone();
            tokio::spawn(async move {
                if let Err(err) = send_live_event(endpoint, peer, entry).await {
                    eprintln!("live event send error: {err}");
                }
            });
        }
    }
}

fn emit_differences<F>(old: &HashMapById, new: &HashMapById, emit: &mut F)
where
    F: FnMut(&str, SnapshotEntry),
{
    let mut ids = BTreeMap::new();
    for id in old.keys() {
        ids.insert(id.clone(), ());
    }
    for id in new.keys() {
        ids.insert(id.clone(), ());
    }

    for id in ids.keys() {
        let old_entry = old.get(id).copied();
        let new_entry = new.get(id).copied();
        if old_entry.map(|entry| entry.hash) != new_entry.map(|entry| entry.hash) {
            if let Some(new_entry) = new_entry {
                emit(id, new_entry);
            }
        }
    }
}

fn emit_snapshot(id: &str, entry: SnapshotEntry) {
    println!("{} {} {}", entry.changed_at_ms, id, hex_hash(entry.hash));
}

fn hex_hash(hash: Blake3Hash) -> String {
    blake3::Hash::from_bytes(hash).to_hex().to_string()
}

fn unix_timestamp_ms() -> io::Result<u64> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
    Ok(now.as_millis() as u64)
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

fn write_cache_file(path: &Path, entries: &HashMapById) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let cache = DaemonCacheFile {
        version: DAEMON_CACHE_VERSION,
        entries: entries.iter().map(|(k, v)| (k.clone(), *v)).collect(),
    };
    let encoded = rkyv::to_bytes::<rkyv::rancor::Error>(&cache).map_err(io::Error::other)?;
    fs::write(path, encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_attr_dir() -> TempDir {
        tempfile::Builder::new()
            .prefix("stashd-test-")
            .tempdir()
            .unwrap()
    }

    #[test]
    fn build_snapshot_map_uses_zero_timestamp_for_scan() {
        let dir = temp_attr_dir();
        let id = "01knxf1n5ffvk9jsm8wve1pgsd";
        fs::write(dir.path().join(id), b"id=1\n").unwrap();

        let entries = build_snapshot_map(dir.path()).unwrap();
        assert_eq!(
            entries.get(id),
            Some(&SnapshotEntry {
                hash: *blake3::hash(b"id=1\n").as_bytes(),
                changed_at_ms: 0,
            })
        );
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
                    changed_at_ms: 7,
                },
            )]),
        )
        .unwrap();

        let mut lines = Vec::new();
        let entries = reconcile_startup_state(dir.path(), &cache_path, |id, entry| {
            lines.push(format!("{} {} {}", entry.changed_at_ms, id, hex_hash(entry.hash)));
        })
        .unwrap();

        let entry = entries.get(&removed).unwrap();
        assert_eq!(entry.hash, TOMBSTONE_HASH);
        assert!(entry.changed_at_ms > 0);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn local_event_suppresses_remote_apply_echo() {
        let dir = temp_attr_dir();
        let id = "01knxf1n5ffvk9jsm8wve1pgsd".to_string();
        fs::write(dir.path().join(&id), b"remote\n").unwrap();
        let remote = SnapshotEntry {
            hash: *blake3::hash(b"remote\n").as_bytes(),
            changed_at_ms: 42,
        };
        let mut entries = HashMap::from([(id.clone(), remote)]);
        let mut suppressed = HashMap::from([(id.clone(), remote)]);
        let event = Event {
            kind: EventKind::Modify(ModifyKind::Any),
            paths: vec![dir.path().join(&id)],
            attrs: Default::default(),
        };

        let updates =
            process_local_event(dir.path(), &mut entries, &mut suppressed, "peer-a", event)
                .unwrap();
        assert!(updates.is_empty());
        assert!(suppressed.is_empty());
        assert_eq!(entries.get(&id), Some(&remote));
    }

    #[test]
    fn remote_newer_snapshot_wins_by_timestamp() {
        let current = SnapshotEntry {
            hash: *blake3::hash(b"old\n").as_bytes(),
            changed_at_ms: 1,
        };
        let remote = SnapshotEntry {
            hash: *blake3::hash(b"new\n").as_bytes(),
            changed_at_ms: 2,
        };
        assert!(should_accept_remote(Some(current), remote));
    }

    #[test]
    fn equal_timestamp_uses_hash_tiebreak() {
        let low = SnapshotEntry {
            hash: [1; 32],
            changed_at_ms: 5,
        };
        let high = SnapshotEntry {
            hash: [2; 32],
            changed_at_ms: 5,
        };
        assert_eq!(compare_snapshot(high, low), CmpOrdering::Greater);
    }
}
