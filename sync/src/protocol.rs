use crate::snapshot::{ReplicatedEntry, SnapshotBucketSummary, SnapshotMetaEntry};
use serde::{Deserialize, Serialize};
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(
    Debug,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub enum RequestMessage {
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
    LiveEvent {
        from: String,
        entry: ReplicatedEntry,
    },
}

#[derive(
    Debug,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    Serialize,
    Deserialize,
)]
pub enum ResponseMessage {
    SnapshotSyncNeedBuckets {
        sync_id: u64,
        level: u8,
        buckets: Vec<String>,
    },
    SnapshotSyncNeedMeta {
        sync_id: u64,
        buckets: Vec<String>,
    },
    SnapshotSyncNeed {
        sync_id: u64,
        ids: Vec<String>,
    },
    Ack,
}

pub async fn write_frame<T>(send: &mut iroh::endpoint::SendStream, value: &T) -> io::Result<()>
where
    T: Serialize,
{
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    send.write_u32(bytes.len() as u32).await?;
    send.write_all(&bytes).await?;
    Ok(())
}

pub async fn read_frame<T>(recv: &mut iroh::endpoint::RecvStream) -> io::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let len = recv.read_u32().await? as usize;
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await.map_err(io::Error::other)?;
    serde_json::from_slice(&buf).map_err(io::Error::other)
}
