use crate::cache::write_cache_file;
use crate::config::Config;
use crate::diagnostics::emit_snapshot;
use crate::snapshot::{
    HashMapById, ReplicatedEntry, SnapshotBucketSummary, SnapshotEntry, SnapshotMetaEntry,
    apply_remote_entries, build_requested_snapshot_entries, build_snapshot_meta_for_buckets,
    canonical_path, collect_missing_snapshot_buckets_from_state,
    collect_missing_snapshot_ids_from_state, collect_snapshot_bucket_summaries, process_local_event,
    reconcile_startup_state, snapshot_from_wire,
};
use crate::transport::{broadcast_entries, handle_incoming, sync_peer};
use iroh::{Endpoint, EndpointAddr, PublicKey, SecretKey, Watcher};
use notify::{Config as NotifyConfig, Event, RecommendedWatcher, RecursiveMode, Watcher as _};
use signal_hook::consts::signal::SIGTERM;
use signal_hook::flag;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

pub(crate) const ALPN: &[u8] = b"stashd/snapshot/1";
const DAEMON_CACHE_VERSION: u32 = 6;
const RESYNC_INTERVAL: Duration = Duration::from_secs(30);

pub(crate) enum Command {
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
    attr_dir: std::path::PathBuf,
    cache_path: std::path::PathBuf,
    endpoint: Endpoint,
    local_origin: String,
    peers: Vec<EndpointAddr>,
    lamport: u64,
    entries: HashMapById,
    suppressed: HashMap<String, SnapshotEntry>,
}

pub fn run(config: Config) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(io::Error::other)?;
    runtime.block_on(run_async(config))
}

async fn run_async(config: Config) -> io::Result<()> {
    let secret_key = load_or_create_secret_key(&config.key_path)?;
    if config.show_id {
        println!("{}", secret_key.public());
        return Ok(());
    }

    let attr_dir = canonical_path(config.attr_dir);
    let cache_path = config.cache_path;
    let endpoint = Endpoint::builder()
        .alpns(vec![ALPN.to_vec()])
        .secret_key(secret_key.clone())
        .bind()
        .await
        .map_err(io::Error::other)?;

    print_node_info(&endpoint)?;

    let mut peers = config.peers;
    for peer_id in &config.peer_ids {
        peers.push(EndpointAddr::new(*peer_id));
    }
    let mut allowlist: HashSet<PublicKey> = config.allow_peers.into_iter().collect();
    for peer_id in &config.peer_ids {
        allowlist.insert(*peer_id);
    }
    for peer in &peers {
        allowlist.insert(peer.id);
    }
    let local_origin = endpoint.id().to_string();
    let terminated = Arc::new(AtomicBool::new(false));
    flag::register(SIGTERM, Arc::clone(&terminated)).map_err(io::Error::other)?;

    let entries = reconcile_startup_state(&attr_dir, &cache_path, &local_origin, emit_snapshot)?;
    write_cache_file(&cache_path, &entries, DAEMON_CACHE_VERSION)?;

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();

    let watch_tx = cmd_tx.clone();
    let mut watcher = RecommendedWatcher::new(
        move |result| match result {
            Ok(event) => {
                let _ = watch_tx.send(Command::LocalFs(event));
            }
            Err(err) => eprintln!("watch error: {err}"),
        },
        NotifyConfig::default(),
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
        let Some(command) = cmd_rx.recv().await else { break; };
        match command {
            Command::LocalFs(event) => {
                let updates = process_local_event(
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
                    emit_snapshot(&update.ulid, &snapshot_from_wire(update));
                }
                write_cache_file(&state.cache_path, &state.entries, DAEMON_CACHE_VERSION)?;
                broadcast_entries(&state.endpoint, &state.peers, &updates, None).await;
            }
            Command::ApplyRemote { peer, entries, respond_to } => {
                let updates = apply_remote_entries(
                    &state.attr_dir,
                    &mut state.lamport,
                    &mut state.entries,
                    &mut state.suppressed,
                    &state.local_origin,
                    entries,
                    emit_snapshot,
                )?;
                if !updates.is_empty() {
                    write_cache_file(&state.cache_path, &state.entries, DAEMON_CACHE_VERSION)?;
                    broadcast_entries(&state.endpoint, &state.peers, &updates, Some(peer)).await;
                }
                if let Some(reply) = respond_to {
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
                let _ = respond_to.send(result);
            }
            Command::CollectSnapshotBuckets { level, parents, respond_to } => {
                let result = collect_snapshot_bucket_summaries(
                    &state.attr_dir,
                    &mut state.lamport,
                    &mut state.entries,
                    &state.local_origin,
                    level,
                    &parents,
                );
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

    write_cache_file(&state.cache_path, &state.entries, DAEMON_CACHE_VERSION)?;
    Ok(())
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
