use criterion::{Criterion, criterion_group, criterion_main};
use stash::store;
use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

fn bench_binary() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let status = Command::new("cargo")
            .arg("build")
            .arg("--release")
            .arg("--bin")
            .arg("stash")
            .current_dir(&manifest_dir)
            .status()
            .expect("cargo build --release --bin stash");
        assert!(status.success(), "cargo build failed");
        manifest_dir.join("target").join("release").join("stash")
    })
}

fn temp_stash_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("stash-bench-{name}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fill_stash(dir: &Path, count: usize) {
    // SAFETY: benchmark setup is single-threaded here and uses STASH_DIR only
    // around store writes for this temporary directory.
    unsafe { std::env::set_var("STASH_DIR", dir) };
    for i in 0..count {
        let body = format!(
            "entry-{i}\npreview line for benchmark item {i}\nmetadata line {}\n",
            i % 17
        );
        let mut attrs = BTreeMap::new();
        attrs.insert("filename".into(), format!("file-{i}.txt"));
        if i % 2 == 0 {
            attrs.insert("source".into(), "bench".into());
        }
        if i % 3 == 0 {
            attrs.insert("stage".into(), "raw".into());
        }
        let mut reader = Cursor::new(body.into_bytes());
        store::push_from_reader(&mut reader, attrs).unwrap();
    }
    // SAFETY: matches the scoped benchmark setup above.
    unsafe { std::env::remove_var("STASH_DIR") };
}

fn run_cli(dir: &Path, args: &[&str]) {
    let output = Command::new(bench_binary())
        .args(args)
        .env("STASH_DIR", dir)
        .output()
        .expect("run stash bench command");
    assert!(
        output.status.success(),
        "command failed: {:?}\nstderr: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn bench_ls(c: &mut Criterion) {
    let dir = temp_stash_dir("ls");
    fill_stash(&dir, 1000);
    c.bench_function("BenchmarkLs1000", |b| {
        b.iter(|| run_cli(&dir, &["ls", "-l", "-n", "20"]))
    });
    let _ = fs::remove_dir_all(dir);
}

fn bench_log(c: &mut Criterion) {
    let dir = temp_stash_dir("log");
    fill_stash(&dir, 1000);
    c.bench_function("BenchmarkLog1000", |b| {
        b.iter(|| run_cli(&dir, &["log", "-n", "20", "--color=false"]))
    });
    let _ = fs::remove_dir_all(dir);
}

fn bench_attr(c: &mut Criterion) {
    let dir = temp_stash_dir("attr");
    fill_stash(&dir, 1000);
    c.bench_function("BenchmarkAttrNewest1000", |b| {
        b.iter(|| run_cli(&dir, &["attr", "@1", "--preview"]))
    });
    let _ = fs::remove_dir_all(dir);
}

criterion_group!(cli_benches, bench_ls, bench_log, bench_attr);
criterion_main!(cli_benches);
