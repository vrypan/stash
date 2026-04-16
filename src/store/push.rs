use signal_hook::SigId;
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::low_level;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use super::PartialSavedError;

struct PartialSaveOptions {
    save_on_error: bool,
    save_empty: bool,
    signal: Option<i32>,
}

pub fn push_from_reader<R: Read>(
    reader: &mut R,
    attrs: BTreeMap<String, String>,
) -> io::Result<String> {
    super::init()?;
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal = Arc::new(AtomicI32::new(0));
    let _signal_guard = SignalGuard::new(&interrupted, &signal)?;
    let id = super::new_ulid()?;
    let data_path = super::tmp_dir()?.join(format!("{id}.data"));
    let data = File::create(&data_path)?;
    run_read_loop(
        reader,
        None,
        data,
        data_path,
        id,
        attrs,
        &interrupted,
        &signal,
        true,
    )
}

pub fn tee_from_reader_partial<R: Read, W: Write>(
    reader: &mut R,
    stdout: &mut W,
    attrs: BTreeMap<String, String>,
    save_on_error: bool,
) -> io::Result<String> {
    super::init()?;
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal = Arc::new(AtomicI32::new(0));
    let _signal_guard = SignalGuard::new(&interrupted, &signal)?;
    let id = super::new_ulid()?;
    let data_path = super::tmp_dir()?.join(format!("{id}.data"));
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

#[inline]
fn check_interrupted(interrupted: &AtomicBool, signal: &AtomicI32) -> Option<(io::Error, i32)> {
    if interrupted.load(Ordering::Relaxed) {
        let signo = signal.load(Ordering::Relaxed);
        let msg = match signo {
            SIGTERM => "terminated by signal",
            _ => "interrupted by signal",
        };
        Some((io::Error::new(io::ErrorKind::Interrupted, msg), signo))
    } else {
        None
    }
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
    let mut sample = Vec::with_capacity(512);
    let mut total = 0i64;
    let mut buf = [0u8; 65536];
    loop {
        if let Some((err, signo)) = check_interrupted(interrupted, signal) {
            return save_or_abort_partial(
                id,
                data_path,
                &sample,
                total,
                attrs,
                err,
                PartialSaveOptions {
                    save_on_error,
                    save_empty: true,
                    signal: Some(signo),
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
            if let Some((err, signo)) = check_interrupted(interrupted, signal) {
                return save_or_abort_partial(
                    id,
                    data_path,
                    &sample,
                    total,
                    attrs,
                    err,
                    PartialSaveOptions {
                        save_on_error,
                        save_empty: true,
                        signal: Some(signo),
                    },
                );
            }
            break;
        }
        let sample_len = sample.len();
        if sample_len < 512 {
            let need = (512 - sample_len).min(n);
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
                    return super::finalize_saved_entry(id, data_path, &sample, total, attrs);
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
    super::finalize_saved_entry(id, data_path, &sample, total, attrs)
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
    super::finalize_saved_entry(id.clone(), data_path, sample, total, attrs)?;
    Err(io::Error::other(PartialSavedError {
        id,
        cause: err,
        signal: options.signal,
    }))
}

struct SignalGuard {
    ids: [SigId; 2],
}

impl SignalGuard {
    fn new(flag: &Arc<AtomicBool>, signal: &Arc<AtomicI32>) -> io::Result<Self> {
        let id0 = register_signal(SIGINT, flag, signal)?;
        let id1 = register_signal(SIGTERM, flag, signal)?;
        Ok(Self { ids: [id0, id1] })
    }
}

impl Drop for SignalGuard {
    fn drop(&mut self) {
        for id in &self.ids {
            low_level::unregister(*id);
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
