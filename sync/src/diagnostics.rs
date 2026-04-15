use crate::snapshot::SnapshotEntry;
use std::io;

pub(crate) fn sync_debug_enabled() -> bool {
    std::env::var_os("STASHD_DEBUG_SYNC").is_some()
}

pub(crate) fn sync_trace(message: impl AsRef<str>) {
    if sync_debug_enabled() {
        eprintln!("stashd sync: {}", message.as_ref());
    }
}

pub(crate) fn sync_io_error(context: impl AsRef<str>, err: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("{}: {}", context.as_ref(), err))
}

pub(crate) fn emit_snapshot(id: &str, entry: SnapshotEntry) {
    eprintln!("{} {} {}", entry.changed_at_ms, id, hex_hash(entry.hash));
}

fn hex_hash(hash: [u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}
