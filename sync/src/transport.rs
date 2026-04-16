use crate::daemon::Command;
use crate::diagnostics::{sync_io_error, sync_trace};
use crate::protocol::{RequestMessage, ResponseMessage, read_frame, write_frame};
use crate::snapshot::{ReplicatedEntry, SnapshotBucketSummary, SnapshotMetaEntry};
use iroh::{Endpoint, EndpointAddr, PublicKey};
use std::collections::HashSet;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{mpsc, oneshot};

static NEXT_SYNC_ID: AtomicU64 = AtomicU64::new(1);

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
    .map_err(|_| {
        io::Error::other(format!(
            "snapshot bucket collection for peer {peer_id} channel closed"
        ))
    })?;
    reply_rx.await.map_err(|_| {
        io::Error::other(format!(
            "snapshot bucket collection for peer {peer_id} dropped"
        ))
    })?
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
    .map_err(|_| {
        io::Error::other(format!(
            "snapshot metadata collection for peer {peer_id} channel closed"
        ))
    })?;
    reply_rx.await.map_err(|_| {
        io::Error::other(format!(
            "snapshot metadata collection for peer {peer_id} dropped"
        ))
    })?
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
    .map_err(|_| {
        io::Error::other(format!(
            "snapshot entry collection for peer {peer_id} channel closed"
        ))
    })?;
    reply_rx.await.map_err(|_| {
        io::Error::other(format!(
            "snapshot entry collection for peer {peer_id} dropped"
        ))
    })?
}

pub(crate) async fn handle_incoming(
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
                buckets.len()
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
            if buckets.is_empty() {
                return Ok(());
            }

            let request: RequestMessage = read_frame(&mut recv).await.map_err(|err| {
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

            let buckets = collect_missing_snapshot_buckets(&buckets, &tx).await?;
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
            if buckets.is_empty() {
                return Ok(());
            }

            let request: RequestMessage = read_frame(&mut recv).await.map_err(|err| {
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
            if buckets.is_empty() {
                return Ok(());
            }

            let request: RequestMessage = read_frame(&mut recv).await.map_err(|err| {
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

            let ids = collect_missing_snapshot_ids(&entries, &tx).await?;
            write_frame(
                &mut send,
                &ResponseMessage::SnapshotSyncNeed {
                    sync_id,
                    ids: ids.clone(),
                },
            )
            .await
            .map_err(|err| {
                sync_io_error(
                    format!("incoming peer {peer}: write snapshot entry need #{sync_id}"),
                    err,
                )
            })?;
            if ids.is_empty() {
                return Ok(());
            }

            let request: RequestMessage = read_frame(&mut recv).await.map_err(|err| {
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

            let (reply_tx, reply_rx) = oneshot::channel();
            let _ = tx.send(Command::ApplyRemote {
                peer,
                entries,
                respond_to: Some(reply_tx),
            });
            reply_rx.await.unwrap_or(());
            write_frame(&mut send, &ResponseMessage::Ack)
                .await
                .map_err(|err| {
                    sync_io_error(
                        format!("incoming peer {peer}: write snapshot ack #{sync_id}"),
                        err,
                    )
                })?;
        }
        RequestMessage::SnapshotSyncBuckets { sync_id, level, .. } => {
            return Err(io::Error::other(format!(
                "incoming peer {peer}: unexpected snapshot bucket request #{sync_id}/{level}"
            )));
        }
        RequestMessage::SnapshotSyncMeta { sync_id, .. } => {
            return Err(io::Error::other(format!(
                "incoming peer {peer}: unexpected snapshot metadata request #{sync_id}"
            )));
        }
        RequestMessage::SnapshotSyncEntries { sync_id, .. } => {
            return Err(io::Error::other(format!(
                "incoming peer {peer}: unexpected snapshot entries request #{sync_id}"
            )));
        }
        RequestMessage::LiveEvent { entry, .. } => {
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

pub(crate) async fn sync_peer(
    endpoint: Endpoint,
    peer: EndpointAddr,
    tx: mpsc::UnboundedSender<Command>,
    buckets: Vec<SnapshotBucketSummary>,
) -> io::Result<()> {
    let peer_id = peer.id;
    let sync_id = NEXT_SYNC_ID.fetch_add(1, Ordering::Relaxed);
    let connection = endpoint
        .connect(peer.clone(), crate::daemon::ALPN)
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
    .map_err(|err| sync_io_error(format!("peer {peer_id}: write snapshot request #{sync_id}"), err))?;

    let response: ResponseMessage = read_frame(&mut recv)
        .await
        .map_err(|err| sync_io_error(format!("peer {peer_id}: read snapshot response #{sync_id}"), err))?;
    let ResponseMessage::SnapshotSyncNeedBuckets {
        sync_id: response_sync_id,
        level,
        buckets,
    } = response
    else {
        return Err(io::Error::other(format!(
            "peer {peer_id}: expected level 5 snapshot bucket need for sync #{sync_id}"
        )));
    };
    if response_sync_id != sync_id || level != 5 {
        return Err(io::Error::other(format!(
            "peer {peer_id}: snapshot bucket phase mismatch {response_sync_id}/{level} != {sync_id}/5"
        )));
    }
    if buckets.is_empty() {
        return Ok(());
    }

    let level_5_buckets = collect_snapshot_buckets(5, &buckets, peer_id, &tx).await?;
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
    .map_err(|err| sync_io_error(format!("peer {peer_id}: write level 5 snapshot buckets #{sync_id}"), err))?;

    let response: ResponseMessage = read_frame(&mut recv).await.map_err(|err| {
        sync_io_error(
            format!("peer {peer_id}: read level 6 snapshot bucket need #{sync_id}"),
            err,
        )
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
    if buckets.is_empty() {
        return Ok(());
    }

    let level_6_buckets = collect_snapshot_buckets(6, &buckets, peer_id, &tx).await?;
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
    .map_err(|err| sync_io_error(format!("peer {peer_id}: write level 6 snapshot buckets #{sync_id}"), err))?;

    let response: ResponseMessage = read_frame(&mut recv)
        .await
        .map_err(|err| sync_io_error(format!("peer {peer_id}: read snapshot metadata need #{sync_id}"), err))?;
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
    if buckets.is_empty() {
        return Ok(());
    }

    let metadata_entries = collect_snapshot_meta_for_buckets(&buckets, peer_id, &tx).await?;
    write_frame(
        &mut send,
        &RequestMessage::SnapshotSyncMeta {
            from: endpoint.id().to_string(),
            sync_id,
            entries: metadata_entries,
        },
    )
    .await
    .map_err(|err| sync_io_error(format!("peer {peer_id}: write snapshot metadata #{sync_id}"), err))?;

    let response: ResponseMessage = read_frame(&mut recv)
        .await
        .map_err(|err| sync_io_error(format!("peer {peer_id}: read snapshot entry need #{sync_id}"), err))?;
    let ResponseMessage::SnapshotSyncNeed {
        sync_id: response_sync_id,
        ids,
    } = response
    else {
        return Err(io::Error::other(format!(
            "peer {peer_id}: expected snapshot entry need for sync #{sync_id}"
        )));
    };
    if response_sync_id != sync_id {
        return Err(io::Error::other(format!(
            "peer {peer_id}: snapshot sync id mismatch {response_sync_id} != {sync_id}"
        )));
    }
    if ids.is_empty() {
        return Ok(());
    }

    let requested_entries = collect_requested_snapshot_entries(&ids, peer_id, &tx).await?;
    write_frame(
        &mut send,
        &RequestMessage::SnapshotSyncEntries {
            from: endpoint.id().to_string(),
            sync_id,
            entries: requested_entries,
        },
    )
    .await
    .map_err(|err| sync_io_error(format!("peer {peer_id}: write snapshot entries #{sync_id}"), err))?;

    let _: ResponseMessage = read_frame(&mut recv)
        .await
        .map_err(|err| sync_io_error(format!("peer {peer_id}: read snapshot ack #{sync_id}"), err))?;
    Ok(())
}

async fn send_live_event(endpoint: Endpoint, peer: EndpointAddr, entry: ReplicatedEntry) -> io::Result<()> {
    let peer_id = peer.id;
    let entry_id = entry.ulid.clone();
    let connection = endpoint
        .connect(peer, crate::daemon::ALPN)
        .await
        .map_err(|err| sync_io_error(format!("peer {peer_id}: connect for live event {entry_id}"), err))?;
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|err| sync_io_error(format!("peer {peer_id}: open bi stream for live event {entry_id}"), err))?;

    write_frame(
        &mut send,
        &RequestMessage::LiveEvent {
            from: endpoint.id().to_string(),
            entry,
        },
    )
    .await
    .map_err(|err| sync_io_error(format!("peer {peer_id}: write live event request {entry_id}"), err))?;
    let _: ResponseMessage = read_frame(&mut recv)
        .await
        .map_err(|err| sync_io_error(format!("peer {peer_id}: read live event ack {entry_id}"), err))?;
    Ok(())
}

pub(crate) async fn broadcast_entries(
    endpoint: &Endpoint,
    peers: &[EndpointAddr],
    entries: &[ReplicatedEntry],
    skip_peer: Option<PublicKey>,
) {
    for peer in peers {
        if skip_peer == Some(peer.id) {
            continue;
        }
        let endpoint = endpoint.clone();
        let peer = peer.clone();
        let entries = entries.to_vec();
        tokio::spawn(async move {
            for entry in entries {
                if let Err(err) = send_live_event(endpoint.clone(), peer.clone(), entry).await {
                    eprintln!("live event send error: {err}");
                }
            }
        });
    }
}
