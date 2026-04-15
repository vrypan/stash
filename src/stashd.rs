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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

use crate::store;

const ALPN: &[u8] = b"stashd/snapshot/1";
const DAEMON_CACHE_VERSION: u32 = 6;
const RESYNC_INTERVAL: Duration = Duration::from_secs(30);
const TOMBSTONE_HASH: Blake3Hash = [0; 32];
static NEXT_SYNC_ID: AtomicU64 = AtomicU64::new(1);

pub type Blake3Hash = [u8; 32];
type HashMapById = HashMap<String, SnapshotEntry>;

#[derive(Parser, Debug, Clone)]
#[command(name = "stashd", about = "Replicate stash attr snapshots over iroh")]
struct Cli {
    #[arg(
        long = "peer-id",
        value_name = "NODE_ID",
        action = clap::ArgAction::Append,
        help = "Peer node ID to sync with using iroh discovery"
    )]
    peer_ids: Vec<PublicKey>,

    #[arg(
        long = "peer",
        value_name = "ENDPOINT_ADDR_JSON",
        value_parser = parse_endpoint_addr,
        action = clap::ArgAction::Append,
        help = "Static peer EndpointAddr encoded as JSON (advanced)"
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

    #[arg(
        long = "show-id",
        help = "Print the persisted node ID, creating it first if needed, then exit"
    )]
    show_id: bool,
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
struct SnapshotEntry {
    hash: Blake3Hash,
    lamport: u64,
    changed_at_ms: u64,
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
    lamport: u64,
    changed_at_ms: u64,
    contents: Option<Vec<u8>>,
    origin: String,
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
struct SnapshotMetaEntry {
    ulid: String,
    hash: Blake3Hash,
    lamport: u64,
    changed_at_ms: u64,
    origin: String,
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
struct SnapshotBucketSummary {
    bucket: String,
    count: u32,
    hash: Blake3Hash,
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
    SnapshotSyncStart {
        from: String,
        sync_id: u64,
        buckets: Vec<SnapshotBucketSummary>,
    },
    SnapshotSyncBuckets {
        from: String,
        sync_id: u64,
        level: u8,
        buckets: Vec<SnapshotBucketSummary>,
    },
    SnapshotSyncMeta {
        from: String,
        sync_id: u64,
        entries: Vec<SnapshotMetaEntry>,
    },
    SnapshotSyncEntries {
        from: String,
        sync_id: u64,
        entries: Vec<ReplicatedEntry>,
    },
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
    SnapshotSyncNeedBuckets {
        sync_id: u64,
        level: u8,
        buckets: Vec<String>,
    },
    SnapshotSyncNeedMeta { sync_id: u64, buckets: Vec<String> },
    SnapshotSyncNeed { sync_id: u64, ids: Vec<String> },
    Ack,
}

enum Command {
    LocalFs(Event),
    ApplyRemote {
        peer: PublicKey,
        entries: Vec<ReplicatedEntry>,
        respond_to: Option<oneshot::Sender<()>>,
    },
    CollectMissingSnapshotIds {
        entries: Vec<SnapshotMetaEntry>,
        respond_to: oneshot::Sender<Vec<String>>,
    },
    CollectMissingSnapshotBuckets {
        buckets: Vec<SnapshotBucketSummary>,
        respond_to: oneshot::Sender<io::Result<Vec<String>>>,
    },
    CollectSnapshotBuckets {
        level: u8,
        parents: Vec<String>,
        respond_to: oneshot::Sender<io::Result<Vec<SnapshotBucketSummary>>>,
    },
    CollectSnapshotMeta {
        buckets: Vec<String>,
        respond_to: oneshot::Sender<io::Result<Vec<SnapshotMetaEntry>>>,
    },
    CollectSnapshotEntries {
        ids: Vec<String>,
        respond_to: oneshot::Sender<io::Result<Vec<ReplicatedEntry>>>,
    },
    SyncPeer(EndpointAddr),
}

struct State {
    attr_dir: PathBuf,
    cache_path: PathBuf,
    endpoint: Endpoint,
    local_origin: String,
    peers: Vec<EndpointAddr>,
    lamport: u64,
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
    let key_path = cli.key_file.unwrap_or(default_key_path()?);
    let secret_key = load_or_create_secret_key(&key_path)?;

    if cli.show_id {
        println!("{}", secret_key.public());
        return Ok(());
    }

    store::init()?;
    let attr_dir = canonical_path(store::attr_dir()?);
    let cache_path = daemon_cache_path()?;
    let endpoint = Endpoint::builder()
        .alpns(vec![ALPN.to_vec()])
        .secret_key(secret_key.clone())
        .bind()
        .await
        .map_err(io::Error::other)?;

    print_node_info(&endpoint)?;

    let mut peers = cli.peers;
    for peer_id in &cli.peer_ids {
        peers.push(EndpointAddr::new(*peer_id));
    }

    let mut allowlist: HashSet<PublicKey> = cli.allow_peers.into_iter().collect();
    for peer_id in &cli.peer_ids {
        allowlist.insert(*peer_id);
    }
    for peer in &peers {
        allowlist.insert(peer.id);
    }
    let local_origin = endpoint.id().to_string();

    let terminated = Arc::new(AtomicBool::new(false));
    flag::register(SIGTERM, Arc::clone(&terminated)).map_err(io::Error::other)?;

    let entries = reconcile_startup_state(&attr_dir, &cache_path, &local_origin, emit_snapshot)?;
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
    let sync_peers = peers.clone();
    tokio::spawn(async move {
        for peer in &sync_peers {
            let _ = sync_tx.send(Command::SyncPeer(peer.clone()));
        }
        let mut interval = tokio::time::interval(RESYNC_INTERVAL);
        interval.tick().await;
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
        peers,
        lamport: entries.values().map(|entry| entry.lamport).max().unwrap_or(0),
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
                        &mut state.lamport,
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
                    apply_remote_entries(
                        &state.attr_dir,
                        &mut state.lamport,
                        &mut state.entries,
                        &mut state.suppressed,
                        &state.local_origin,
                        entries,
                    )?;
                if !updates.is_empty() {
                    write_cache_file(&state.cache_path, &state.entries)?;
                    broadcast_entries(&state.endpoint, &state.peers, &updates, Some(peer)).await;
                }
                if let Some(reply) = respond_to {
                    write_cache_file(&state.cache_path, &state.entries)?;
                    let _ = reply.send(());
                }
            }
            Command::CollectMissingSnapshotIds { entries, respond_to } => {
                let ids = collect_missing_snapshot_ids_from_state(&state.entries, entries);
                let _ = respond_to.send(ids);
            }
            Command::CollectMissingSnapshotBuckets { buckets, respond_to } => {
                let result = collect_missing_snapshot_buckets_from_state(
                    &state.attr_dir,
                    &mut state.lamport,
                    &mut state.entries,
                    &state.local_origin,
                    buckets,
                );
                if result.is_ok() {
                    write_cache_file(&state.cache_path, &state.entries)?;
                }
                let _ = respond_to.send(result);
            }
            Command::CollectSnapshotBuckets {
                level,
                parents,
                respond_to,
            } => {
                let result = collect_snapshot_bucket_summaries(
                    &state.attr_dir,
                    &mut state.lamport,
                    &mut state.entries,
                    &state.local_origin,
                    level,
                    &parents,
                );
                if result.is_ok() {
                    write_cache_file(&state.cache_path, &state.entries)?;
                }
                let _ = respond_to.send(result);
            }
            Command::CollectSnapshotMeta { buckets, respond_to } => {
                let result = build_snapshot_meta_for_buckets(
                    &state.attr_dir,
                    &mut state.lamport,
                    &mut state.entries,
                    &state.local_origin,
                    &buckets,
                );
                if result.is_ok() {
                    write_cache_file(&state.cache_path, &state.entries)?;
                }
                let _ = respond_to.send(result);
            }
            Command::CollectSnapshotEntries { ids, respond_to } => {
                let result = build_requested_snapshot_entries(
                    &state.attr_dir,
                    &mut state.lamport,
                    &mut state.entries,
                    &state.local_origin,
                    &ids,
                );
                if result.is_ok() {
                    write_cache_file(&state.cache_path, &state.entries)?;
                }
                let _ = respond_to.send(result);
            }
            Command::SyncPeer(peer) => {
                let snapshot = collect_snapshot_bucket_summaries(
                    &state.attr_dir,
                    &mut state.lamport,
                    &mut state.entries,
                    &state.local_origin,
                    4,
                    &[],
                )?;
                write_cache_file(&state.cache_path, &state.entries)?;
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

fn sync_debug_enabled() -> bool {
    std::env::var_os("STASHD_DEBUG_SYNC").is_some()
}

fn sync_trace(message: impl AsRef<str>) {
    if sync_debug_enabled() {
        eprintln!("stashd sync: {}", message.as_ref());
    }
}

fn sync_io_error(context: impl AsRef<str>, err: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("{}: {}", context.as_ref(), err))
}

async fn collect_missing_snapshot_ids(
    entries: &[SnapshotMetaEntry],
    tx: &mpsc::UnboundedSender<Command>,
) -> io::Result<Vec<String>> {
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(Command::CollectMissingSnapshotIds {
        entries: entries.to_vec(),
        respond_to: reply_tx,
    })
    .map_err(|_| io::Error::other("snapshot metadata compare channel closed"))?;
    reply_rx
        .await
        .map_err(|_| io::Error::other("snapshot metadata compare dropped"))
}

async fn collect_missing_snapshot_buckets(
    buckets: &[SnapshotBucketSummary],
    tx: &mpsc::UnboundedSender<Command>,
) -> io::Result<Vec<String>> {
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(Command::CollectMissingSnapshotBuckets {
        buckets: buckets.to_vec(),
        respond_to: reply_tx,
    })
    .map_err(|_| io::Error::other("snapshot bucket compare channel closed"))?;
    reply_rx
        .await
        .map_err(|_| io::Error::other("snapshot bucket compare dropped"))?
}

async fn collect_snapshot_buckets(
    level: u8,
    parents: &[String],
    peer_id: PublicKey,
    tx: &mpsc::UnboundedSender<Command>,
) -> io::Result<Vec<SnapshotBucketSummary>> {
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(Command::CollectSnapshotBuckets {
        level,
        parents: parents.to_vec(),
        respond_to: reply_tx,
    })
    .map_err(|_| io::Error::other(format!("snapshot bucket collection for peer {peer_id} channel closed")))?;
    reply_rx
        .await
        .map_err(|_| io::Error::other(format!("snapshot bucket collection for peer {peer_id} dropped")))?
}

async fn collect_snapshot_meta_for_buckets(
    buckets: &[String],
    peer_id: PublicKey,
    tx: &mpsc::UnboundedSender<Command>,
) -> io::Result<Vec<SnapshotMetaEntry>> {
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(Command::CollectSnapshotMeta {
        buckets: buckets.to_vec(),
        respond_to: reply_tx,
    })
    .map_err(|_| io::Error::other(format!("snapshot metadata collection for peer {peer_id} channel closed")))?;
    reply_rx
        .await
        .map_err(|_| io::Error::other(format!("snapshot metadata collection for peer {peer_id} dropped")))?
}

async fn collect_requested_snapshot_entries(
    ids: &[String],
    peer_id: PublicKey,
    tx: &mpsc::UnboundedSender<Command>,
) -> io::Result<Vec<ReplicatedEntry>> {
    let (reply_tx, reply_rx) = oneshot::channel();
    tx.send(Command::CollectSnapshotEntries {
        ids: ids.to_vec(),
        respond_to: reply_tx,
    })
    .map_err(|_| io::Error::other(format!("snapshot entry collection for peer {peer_id} channel closed")))?;
    reply_rx
        .await
        .map_err(|_| io::Error::other(format!("snapshot entry collection for peer {peer_id} dropped")))?
}

async fn handle_incoming(
    incoming: iroh::endpoint::Incoming,
    tx: mpsc::UnboundedSender<Command>,
    allowlist: Arc<HashSet<PublicKey>>,
) -> io::Result<()> {
    let connection = incoming
        .await
        .map_err(|err| sync_io_error("incoming: awaiting connection", err))?;
    let peer = connection.remote_id();
    sync_trace(format!("incoming connection from {peer}"));
    if !allowlist.contains(&peer) {
        sync_trace(format!("rejecting non-allowlisted peer {peer}"));
        return Ok(());
    }

    let (mut send, mut recv) = connection
        .accept_bi()
        .await
        .map_err(|err| sync_io_error(format!("incoming peer {peer}: accept bi stream"), err))?;
    let request: RequestMessage = read_frame(&mut recv)
        .await
        .map_err(|err| sync_io_error(format!("incoming peer {peer}: read request"), err))?;
    match request {
        RequestMessage::SnapshotSyncStart {
            sync_id, buckets, ..
        } => {
            sync_trace(format!(
                "incoming snapshot buckets #{sync_id} from {peer}: {} buckets",
                buckets.len(),
            ));
            sync_trace(format!(
                "incoming snapshot buckets #{sync_id} from {peer}: comparing local state"
            ));
            let buckets = collect_missing_snapshot_buckets(&buckets, &tx).await?;
            sync_trace(format!(
                "incoming snapshot buckets #{sync_id} from {peer}: requesting {} buckets",
                buckets.len()
            ));
            write_frame(
                &mut send,
                &ResponseMessage::SnapshotSyncNeedBuckets {
                    sync_id,
                    level: 5,
                    buckets: buckets.clone(),
                },
            )
                .await
                .map_err(|err| {
                    sync_io_error(
                        format!("incoming peer {peer}: write snapshot bucket need #{sync_id}"),
                        err,
                    )
                })?;
            sync_trace(format!("incoming snapshot buckets #{sync_id} from {peer}: need written"));
            if buckets.is_empty() {
                return Ok(());
            }

            let request: RequestMessage = read_frame(&mut recv)
                .await
                .map_err(|err| {
                    sync_io_error(
                        format!("incoming peer {peer}: read level 5 snapshot buckets #{sync_id}"),
                        err,
                    )
                })?;
            let RequestMessage::SnapshotSyncBuckets {
                sync_id: response_sync_id,
                level,
                buckets,
                ..
            } = request
            else {
                return Err(io::Error::other(format!(
                    "incoming peer {peer}: expected level 5 snapshot buckets for sync #{sync_id}"
                )));
            };
            if response_sync_id != sync_id || level != 5 {
                return Err(io::Error::other(format!(
                    "incoming peer {peer}: snapshot bucket phase mismatch {response_sync_id}/{level} != {sync_id}/5"
                )));
            }
            sync_trace(format!(
                "incoming level 5 snapshot buckets #{sync_id} from {peer}: {} buckets",
                buckets.len()
            ));
            let buckets = collect_missing_snapshot_buckets(&buckets, &tx).await?;
            sync_trace(format!(
                "incoming level 5 snapshot buckets #{sync_id} from {peer}: requesting {} buckets",
                buckets.len()
            ));
            write_frame(
                &mut send,
                &ResponseMessage::SnapshotSyncNeedBuckets {
                    sync_id,
                    level: 6,
                    buckets: buckets.clone(),
                },
            )
                .await
                .map_err(|err| {
                    sync_io_error(
                        format!("incoming peer {peer}: write level 6 snapshot bucket need #{sync_id}"),
                        err,
                    )
                })?;
            sync_trace(format!(
                "incoming level 5 snapshot buckets #{sync_id} from {peer}: need written"
            ));
            if buckets.is_empty() {
                return Ok(());
            }

            let request: RequestMessage = read_frame(&mut recv)
                .await
                .map_err(|err| {
                    sync_io_error(
                        format!("incoming peer {peer}: read level 6 snapshot buckets #{sync_id}"),
                        err,
                    )
                })?;
            let RequestMessage::SnapshotSyncBuckets {
                sync_id: response_sync_id,
                level,
                buckets,
                ..
            } = request
            else {
                return Err(io::Error::other(format!(
                    "incoming peer {peer}: expected level 6 snapshot buckets for sync #{sync_id}"
                )));
            };
            if response_sync_id != sync_id || level != 6 {
                return Err(io::Error::other(format!(
                    "incoming peer {peer}: snapshot bucket phase mismatch {response_sync_id}/{level} != {sync_id}/6"
                )));
            }
            sync_trace(format!(
                "incoming level 6 snapshot buckets #{sync_id} from {peer}: {} buckets",
                buckets.len()
            ));
            write_frame(
                &mut send,
                &ResponseMessage::SnapshotSyncNeedMeta {
                    sync_id,
                    buckets: buckets.iter().map(|bucket| bucket.bucket.clone()).collect(),
                },
            )
            .await
            .map_err(|err| {
                sync_io_error(
                    format!("incoming peer {peer}: write snapshot metadata need #{sync_id}"),
                    err,
                )
            })?;
            sync_trace(format!(
                "incoming level 6 snapshot buckets #{sync_id} from {peer}: metadata need written"
            ));
            if buckets.is_empty() {
                return Ok(());
            }

            let request: RequestMessage = read_frame(&mut recv)
                .await
                .map_err(|err| {
                    sync_io_error(
                        format!("incoming peer {peer}: read snapshot metadata #{sync_id}"),
                        err,
                    )
                })?;
            let RequestMessage::SnapshotSyncMeta {
                sync_id: response_sync_id,
                entries,
                ..
            } = request
            else {
                return Err(io::Error::other(format!(
                    "incoming peer {peer}: expected snapshot metadata for sync #{sync_id}"
                )));
            };
            if response_sync_id != sync_id {
                return Err(io::Error::other(format!(
                    "incoming peer {peer}: snapshot sync id mismatch {response_sync_id} != {sync_id}"
                )));
            }
            sync_trace(format!(
                "incoming snapshot metadata #{sync_id} from {peer}: {} entries",
                entries.len()
            ));
            let ids = collect_missing_snapshot_ids(&entries, &tx).await?;
            sync_trace(format!(
                "incoming snapshot metadata #{sync_id} from {peer}: requesting {} entries",
                ids.len()
            ));
            write_frame(&mut send, &ResponseMessage::SnapshotSyncNeed { sync_id, ids: ids.clone() })
                .await
                .map_err(|err| {
                    sync_io_error(
                        format!("incoming peer {peer}: write snapshot entry need #{sync_id}"),
                        err,
                    )
                })?;
            sync_trace(format!(
                "incoming snapshot metadata #{sync_id} from {peer}: need written"
            ));
            if ids.is_empty() {
                return Ok(());
            }

            let request: RequestMessage = read_frame(&mut recv)
                .await
                .map_err(|err| {
                    sync_io_error(
                        format!("incoming peer {peer}: read snapshot entries #{sync_id}"),
                        err,
                    )
                })?;
            let RequestMessage::SnapshotSyncEntries {
                sync_id: response_sync_id,
                entries,
                ..
            } = request
            else {
                return Err(io::Error::other(format!(
                    "incoming peer {peer}: expected snapshot entries for sync #{sync_id}"
                )));
            };
            if response_sync_id != sync_id {
                return Err(io::Error::other(format!(
                    "incoming peer {peer}: snapshot sync id mismatch {response_sync_id} != {sync_id}"
                )));
            }
            sync_trace(format!(
                "incoming snapshot entries #{sync_id} from {peer}: {} entries",
                entries.len()
            ));
            let (reply_tx, reply_rx) = oneshot::channel();
            let _ = tx.send(Command::ApplyRemote {
                peer,
                entries,
                respond_to: Some(reply_tx),
            });
            sync_trace(format!(
                "incoming snapshot entries #{sync_id} from {peer}: waiting for apply"
            ));
            reply_rx.await.unwrap_or(());
            write_frame(&mut send, &ResponseMessage::Ack)
                .await
                .map_err(|err| {
                    sync_io_error(format!("incoming peer {peer}: write snapshot ack #{sync_id}"), err)
                })?;
            sync_trace(format!("incoming snapshot entries #{sync_id} from {peer}: ack written"));
        }
        RequestMessage::SnapshotSyncMeta { sync_id, .. } => {
            return Err(io::Error::other(format!(
                "incoming peer {peer}: unexpected snapshot metadata request #{sync_id}"
            )));
        }
        RequestMessage::SnapshotSyncBuckets { sync_id, level, .. } => {
            return Err(io::Error::other(format!(
                "incoming peer {peer}: unexpected snapshot bucket request #{sync_id}/{level}"
            )));
        }
        RequestMessage::SnapshotSyncEntries { sync_id, .. } => {
            return Err(io::Error::other(format!(
                "incoming peer {peer}: unexpected snapshot entries request #{sync_id}"
            )));
        }
        RequestMessage::LiveEvent { entry, .. } => {
            sync_trace(format!("incoming live event from {peer}: {}", entry.ulid));
            let _ = tx.send(Command::ApplyRemote {
                peer,
                entries: vec![entry],
                respond_to: None,
            });
            write_frame(&mut send, &ResponseMessage::Ack)
                .await
                .map_err(|err| sync_io_error(format!("incoming peer {peer}: write ack"), err))?;
        }
    }
    Ok(())
}

async fn sync_peer(
    endpoint: Endpoint,
    peer: EndpointAddr,
    tx: mpsc::UnboundedSender<Command>,
    buckets: Vec<SnapshotBucketSummary>,
) -> io::Result<()> {
    let peer_id = peer.id;
    let sync_id = NEXT_SYNC_ID.fetch_add(1, Ordering::Relaxed);
    sync_trace(format!(
        "starting snapshot sync #{sync_id} to {peer_id}: {} buckets",
        buckets.len()
    ));
    let connection = endpoint
        .connect(peer.clone(), ALPN)
        .await
        .map_err(|err| sync_io_error(format!("peer {peer_id}: connect"), err))?;
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|err| sync_io_error(format!("peer {peer_id}: open bi stream"), err))?;
    write_frame(
        &mut send,
        &RequestMessage::SnapshotSyncStart {
            from: endpoint.id().to_string(),
            sync_id,
            buckets,
        },
    )
    .await
    .map_err(|err| {
        sync_io_error(
            format!("peer {peer_id}: write snapshot request #{sync_id}"),
            err,
        )
    })?;
    sync_trace(format!(
        "snapshot sync #{sync_id} to {peer_id}: request written"
    ));
    let response: ResponseMessage = read_frame(&mut recv)
        .await
        .map_err(|err| {
            sync_io_error(
                format!("peer {peer_id}: read snapshot response #{sync_id}"),
                err,
            )
        })?;
    if let ResponseMessage::SnapshotSyncNeedBuckets {
        sync_id: response_sync_id,
        level,
        buckets,
    } = response
    {
        if response_sync_id != sync_id || level != 5 {
            return Err(io::Error::other(format!(
                "peer {peer_id}: snapshot bucket phase mismatch {response_sync_id}/{level} != {sync_id}/5"
            )));
        }
        sync_trace(format!(
            "received level 5 snapshot bucket need #{sync_id} from {peer_id}: {} buckets",
            buckets.len()
        ));
        if buckets.is_empty() {
            sync_trace(format!(
                "snapshot sync #{sync_id} to {peer_id}: no level 5 buckets requested"
            ));
            return Ok(());
        }
        let level_5_buckets = collect_snapshot_buckets(5, &buckets, peer_id, &tx).await?;
        sync_trace(format!(
            "snapshot sync #{sync_id} to {peer_id}: sending {} level 5 buckets",
            level_5_buckets.len()
        ));
        write_frame(
            &mut send,
            &RequestMessage::SnapshotSyncBuckets {
                from: endpoint.id().to_string(),
                sync_id,
                level: 5,
                buckets: level_5_buckets,
            },
        )
        .await
        .map_err(|err| {
            sync_io_error(
                format!("peer {peer_id}: write level 5 snapshot buckets #{sync_id}"),
                err,
            )
        })?;
        sync_trace(format!(
            "snapshot sync #{sync_id} to {peer_id}: level 5 buckets written"
        ));
        let response: ResponseMessage = read_frame(&mut recv).await.map_err(|err| {
            sync_io_error(format!("peer {peer_id}: read level 6 snapshot bucket need #{sync_id}"), err)
        })?;
        let ResponseMessage::SnapshotSyncNeedBuckets {
            sync_id: response_sync_id,
            level,
            buckets,
        } = response
        else {
            return Err(io::Error::other(format!(
                "peer {peer_id}: expected level 6 snapshot bucket need for sync #{sync_id}"
            )));
        };
        if response_sync_id != sync_id || level != 6 {
            return Err(io::Error::other(format!(
                "peer {peer_id}: snapshot bucket phase mismatch {response_sync_id}/{level} != {sync_id}/6"
            )));
        }
        sync_trace(format!(
            "received level 6 snapshot bucket need #{sync_id} from {peer_id}: {} buckets",
            buckets.len()
        ));
        if buckets.is_empty() {
            sync_trace(format!(
                "snapshot sync #{sync_id} to {peer_id}: no level 6 buckets requested"
            ));
            return Ok(());
        }
        let level_6_buckets = collect_snapshot_buckets(6, &buckets, peer_id, &tx).await?;
        sync_trace(format!(
            "snapshot sync #{sync_id} to {peer_id}: sending {} level 6 buckets",
            level_6_buckets.len()
        ));
        write_frame(
            &mut send,
            &RequestMessage::SnapshotSyncBuckets {
                from: endpoint.id().to_string(),
                sync_id,
                level: 6,
                buckets: level_6_buckets,
            },
        )
        .await
        .map_err(|err| {
            sync_io_error(
                format!("peer {peer_id}: write level 6 snapshot buckets #{sync_id}"),
                err,
            )
        })?;
        sync_trace(format!(
            "snapshot sync #{sync_id} to {peer_id}: level 6 buckets written"
        ));
        let response: ResponseMessage = read_frame(&mut recv).await.map_err(|err| {
            sync_io_error(format!("peer {peer_id}: read snapshot metadata need #{sync_id}"), err)
        })?;
        let ResponseMessage::SnapshotSyncNeedMeta {
            sync_id: response_sync_id,
            buckets,
        } = response
        else {
            return Err(io::Error::other(format!(
                "peer {peer_id}: expected snapshot metadata need for sync #{sync_id}"
            )));
        };
        if response_sync_id != sync_id {
            return Err(io::Error::other(format!(
                "peer {peer_id}: snapshot sync id mismatch {response_sync_id} != {sync_id}"
            )));
        }
        sync_trace(format!(
            "received snapshot metadata need #{sync_id} from {peer_id}: {} buckets",
            buckets.len()
        ));
        if buckets.is_empty() {
            sync_trace(format!(
                "snapshot sync #{sync_id} to {peer_id}: no metadata buckets requested"
            ));
            return Ok(());
        }
        let metadata_entries = collect_snapshot_meta_for_buckets(&buckets, peer_id, &tx).await?;
        sync_trace(format!(
            "snapshot sync #{sync_id} to {peer_id}: sending {} metadata entries",
            metadata_entries.len()
        ));
        write_frame(
            &mut send,
            &RequestMessage::SnapshotSyncMeta {
                from: endpoint.id().to_string(),
                sync_id,
                entries: metadata_entries,
            },
        )
        .await
        .map_err(|err| {
            sync_io_error(
                format!("peer {peer_id}: write snapshot metadata #{sync_id}"),
                err,
            )
        })?;
        sync_trace(format!(
            "snapshot sync #{sync_id} to {peer_id}: metadata written"
        ));
        let response: ResponseMessage = read_frame(&mut recv).await.map_err(|err| {
            sync_io_error(format!("peer {peer_id}: read snapshot entry need #{sync_id}"), err)
        })?;
        let ResponseMessage::SnapshotSyncNeed { sync_id: response_sync_id, ids } = response else {
            return Err(io::Error::other(format!(
                "peer {peer_id}: expected snapshot entry need for sync #{sync_id}"
            )));
        };
        if response_sync_id != sync_id {
            return Err(io::Error::other(format!(
                "peer {peer_id}: snapshot sync id mismatch {response_sync_id} != {sync_id}"
            )));
        }
        sync_trace(format!(
            "received snapshot entry need #{sync_id} from {peer_id}: {} entries",
            ids.len()
        ));
        if ids.is_empty() {
            sync_trace(format!(
                "snapshot sync #{sync_id} to {peer_id}: no entries requested"
            ));
            return Ok(());
        }
        let requested_entries = collect_requested_snapshot_entries(&ids, peer_id, &tx).await?;
        sync_trace(format!(
            "snapshot sync #{sync_id} to {peer_id}: sending {} requested entries",
            requested_entries.len()
        ));
        write_frame(
            &mut send,
            &RequestMessage::SnapshotSyncEntries {
                from: endpoint.id().to_string(),
                sync_id,
                entries: requested_entries,
            },
        )
        .await
        .map_err(|err| {
            sync_io_error(
                format!("peer {peer_id}: write snapshot entries #{sync_id}"),
                err,
            )
        })?;
        sync_trace(format!(
            "snapshot sync #{sync_id} to {peer_id}: requested entries written"
        ));
        let _: ResponseMessage = read_frame(&mut recv).await.map_err(|err| {
            sync_io_error(format!("peer {peer_id}: read snapshot ack #{sync_id}"), err)
        })?;
        sync_trace(format!("snapshot sync #{sync_id} to {peer_id}: ack received"));
    }
    Ok(())
}

async fn send_live_event(
    endpoint: Endpoint,
    peer: EndpointAddr,
    entry: ReplicatedEntry,
) -> io::Result<()> {
    let peer_id = peer.id;
    let entry_id = entry.ulid.clone();
    sync_trace(format!("sending live event to {peer_id}: {entry_id}"));
    let connection = endpoint
        .connect(peer, ALPN)
        .await
        .map_err(|err| sync_io_error(format!("peer {peer_id}: connect for live event {entry_id}"), err))?;
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|err| {
            sync_io_error(
                format!("peer {peer_id}: open bi stream for live event {entry_id}"),
                err,
            )
        })?;
    write_frame(
        &mut send,
        &RequestMessage::LiveEvent {
            from: endpoint.id().to_string(),
            entry,
        },
    )
    .await
    .map_err(|err| {
        sync_io_error(
            format!("peer {peer_id}: write live event request {entry_id}"),
            err,
        )
    })?;
    let _: ResponseMessage = read_frame(&mut recv).await.map_err(|err| {
        sync_io_error(
            format!("peer {peer_id}: read live event ack {entry_id}"),
            err,
        )
    })?;
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
    local_origin: &str,
    mut emit: F,
) -> io::Result<HashMapById>
where
    F: FnMut(&str, SnapshotEntry),
{
    let cached = read_cache_file(cache_path)?;
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

fn daemon_cache_path() -> io::Result<PathBuf> {
    Ok(store::base_dir()?.join("cache").join("daemon.cache"))
}

fn canonical_path(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn build_snapshot_map(attr_dir: &Path, local_origin: &str) -> io::Result<HashMapById> {
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

fn file_changed_at_ms(path: &Path) -> io::Result<u64> {
    match fs::metadata(path).and_then(|meta| meta.modified()) {
        Ok(modified) => Ok(system_time_to_ms(modified)?),
        Err(err) if err.kind() == io::ErrorKind::NotFound => unix_timestamp_ms(),
        Err(err) => Err(err),
    }
}

fn system_time_to_ms(value: SystemTime) -> io::Result<u64> {
    let duration = value
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?;
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

fn should_rescan_event(attr_dir: &Path, event: &Event) -> bool {
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

fn process_local_event(
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
        let previous_changed_at_ms = entries
            .get(&id)
            .map(|entry| entry.changed_at_ms)
            .unwrap_or(0);
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

fn rescan_local_state(
    attr_dir: &Path,
    lamport: &mut u64,
    entries: &mut HashMapById,
    suppressed: &mut HashMap<String, SnapshotEntry>,
    local_origin: &str,
) -> io::Result<Vec<ReplicatedEntry>> {
    let scanned = build_snapshot_map(attr_dir, local_origin)?;
    let mut ids = BTreeMap::new();
    for id in entries.keys() {
        ids.insert(id.clone(), ());
    }
    for id in scanned.keys() {
        ids.insert(id.clone(), ());
    }

    let mut updates = Vec::new();
    for id in ids.into_keys() {
        let scanned_entry = scanned.get(&id).cloned();
        let observed_hash = scanned_entry
            .as_ref()
            .map(|entry| entry.hash)
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
        let previous_changed_at_ms = entries
            .get(&id)
            .map(|entry| entry.changed_at_ms)
            .unwrap_or(0);
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
                changed_at_ms: unix_timestamp_ms()?
                    .max(previous_changed_at_ms.saturating_add(1)),
                origin: local_origin.to_string(),
            }
        };
        entries.insert(id.clone(), entry.clone());
        updates.push(build_replication_entry(attr_dir, &id, entry)?);
    }
    Ok(updates)
}

fn apply_remote_entries(
    attr_dir: &Path,
    lamport: &mut u64,
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
        let current = entries.get(&entry.ulid).cloned();
        if !should_accept_remote(current, remote_snapshot.clone()) {
            continue;
        }
        apply_remote_entry_to_disk(attr_dir, &entry)?;
        *lamport = (*lamport).max(remote_snapshot.lamport);
        entries.insert(entry.ulid.clone(), remote_snapshot.clone());
        suppressed.insert(entry.ulid.clone(), remote_snapshot.clone());
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
    left.lamport
        .cmp(&right.lamport)
        .then_with(|| left.changed_at_ms.cmp(&right.changed_at_ms))
        .then_with(|| left.origin.cmp(&right.origin))
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

fn collect_missing_snapshot_ids_from_state(
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

fn collect_missing_snapshot_buckets_from_state(
    attr_dir: &Path,
    lamport: &mut u64,
    entries: &mut HashMapById,
    local_origin: &str,
    remote_buckets: Vec<SnapshotBucketSummary>,
) -> io::Result<Vec<String>> {
    if remote_buckets.is_empty() {
        return Ok(Vec::new());
    }
    let level = remote_buckets
        .first()
        .map(|bucket| bucket.bucket.len() as u8)
        .unwrap_or(6);
    let local_buckets = collect_snapshot_bucket_summaries(
        attr_dir,
        lamport,
        entries,
        local_origin,
        level,
        &[],
    )?;
    let local_map: HashMap<String, SnapshotBucketSummary> = local_buckets
        .into_iter()
        .map(|bucket| (bucket.bucket.clone(), bucket))
        .collect();
    Ok(remote_buckets
        .into_iter()
        .filter_map(|bucket| {
            (local_map.get(&bucket.bucket) != Some(&bucket)).then_some(bucket.bucket)
        })
        .collect())
}

fn collect_snapshot_bucket_summaries(
    attr_dir: &Path,
    lamport: &mut u64,
    entries: &mut HashMapById,
    local_origin: &str,
    level: u8,
    parents: &[String],
) -> io::Result<Vec<SnapshotBucketSummary>> {
    let metadata_entries = build_snapshot_meta_for_buckets(
        attr_dir,
        lamport,
        entries,
        local_origin,
        parents,
    )?;
    let mut grouped: BTreeMap<String, Vec<SnapshotMetaEntry>> = BTreeMap::new();
    for entry in metadata_entries {
        grouped
            .entry(bucket_for_ulid(&entry.ulid, level))
            .or_default()
            .push(entry);
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

fn build_requested_snapshot_entries(
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

fn build_snapshot_meta_for_buckets(
    attr_dir: &Path,
    lamport: &mut u64,
    entries: &mut HashMapById,
    local_origin: &str,
    buckets: &[String],
) -> io::Result<Vec<SnapshotMetaEntry>> {
    let bucket_filter: Option<HashSet<&str>> = (!buckets.is_empty())
        .then(|| buckets.iter().map(String::as_str).collect());
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

fn build_snapshot_meta_entry(
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

fn build_replication_entry(
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

fn snapshot_from_wire(entry: &ReplicatedEntry) -> SnapshotEntry {
    SnapshotEntry {
        hash: entry.hash,
        lamport: entry.lamport,
        changed_at_ms: entry.changed_at_ms,
        origin: entry.origin.clone(),
    }
}

fn snapshot_meta_to_snapshot(entry: &SnapshotMetaEntry) -> SnapshotEntry {
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

fn bucket_for_ulid(id: &str, level: u8) -> String {
    let len = usize::from(level).min(id.len());
    id[..len].to_string()
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
        let old_entry = old.get(id);
        let new_entry = new.get(id);
        if old_entry.map(|entry| entry.hash) != new_entry.map(|entry| entry.hash) {
            if let Some(new_entry) = new_entry {
                emit(id, new_entry.clone());
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
        entries: entries
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
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
        let updates = process_local_event(
            dir.path(),
            &mut lamport,
            &mut entries,
            &mut suppressed,
            "peer-a",
            event,
        )
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
        let updates = process_local_event(
            dir.path(),
            &mut lamport,
            &mut entries,
            &mut suppressed,
            "peer-a",
            event,
        )
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
