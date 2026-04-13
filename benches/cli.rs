use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use stash_cli::store;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use tempfile::TempDir;

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

fn bench_stash_dir() -> &'static Path {
    static DIR: OnceLock<TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = TempDir::new().expect("create benchmark stash dir");
        fill_stash(dir.path(), 1000);
        dir
    })
    .path()
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

fn run_cli_with_stdin(dir: &Path, args: &[&str], stdin: &[u8]) {
    let mut child = Command::new(bench_binary())
        .args(args)
        .env("STASH_DIR", dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn stash bench command");
    child
        .stdin
        .take()
        .expect("stash stdin")
        .write_all(stdin)
        .expect("write stash bench stdin");
    let output = child
        .wait_with_output()
        .expect("wait for stash bench command");
    assert!(
        output.status.success(),
        "command failed: {:?}\nstderr: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn bench_ls(c: &mut Criterion) {
    let dir = bench_stash_dir();
    c.bench_function("BenchmarkLs1000", |b| {
        b.iter(|| run_cli(dir, &["ls", "-l", "-n", "20"]))
    });
}

fn bench_ls_all(c: &mut Criterion) {
    let dir = bench_stash_dir();
    c.bench_function("BenchmarkLsAll1000", |b| {
        b.iter(|| run_cli(dir, &["ls", "-l"]))
    });
}

fn bench_ls_json(c: &mut Criterion) {
    let dir = bench_stash_dir();
    c.bench_function("BenchmarkLsJson1000", |b| {
        b.iter(|| run_cli(dir, &["ls", "--json", "-n", "20"]))
    });
}

fn bench_ls_json_all(c: &mut Criterion) {
    let dir = bench_stash_dir();
    c.bench_function("BenchmarkLsJsonAll1000", |b| {
        b.iter(|| run_cli(dir, &["ls", "--json"]))
    });
}

fn bench_attr(c: &mut Criterion) {
    let dir = bench_stash_dir();
    c.bench_function("BenchmarkAttrNewest1000", |b| {
        b.iter(|| run_cli(dir, &["attr", "@1", "--preview"]))
    });
}

fn bench_push(c: &mut Criterion) {
    let payload =
        b"entry for push benchmark\npreview line for push benchmark\nmetadata line push\n";
    let mut group = c.benchmark_group("push");
    group.sample_size(60);
    group.bench_function("BenchmarkPush", |b| {
        b.iter_batched(
            || TempDir::new().expect("create push benchmark stash dir"),
            |dir| run_cli_with_stdin(dir.path(), &["push", "--print=null"], payload),
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_push_100(c: &mut Criterion) {
    let payload =
        b"entry for push benchmark\npreview line for push benchmark\nmetadata line push\n";
    let mut group = c.benchmark_group("push-100");
    group.sample_size(60);
    group.measurement_time(std::time::Duration::from_secs(20));
    group.bench_function("BenchmarkPush100", |b| {
        b.iter_batched(
            || TempDir::new().expect("create push-100 benchmark stash dir"),
            |dir| {
                for _ in 0..100 {
                    run_cli_with_stdin(dir.path(), &["push", "--print=null"], payload);
                }
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_cat_dir() -> &'static Path {
    static DIR: OnceLock<TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let _ = bench_binary(); // ensure binary is built
        let dir = TempDir::new().expect("create cat benchmark stash dir");
        // Push a 10MB entry via the CLI to avoid cached_base_dir conflicts.
        let payload = vec![b'x'; 10 * 1024 * 1024];
        run_cli_with_stdin(dir.path(), &["push", "--print=null"], &payload);
        dir
    })
    .path()
}

fn bench_cat(c: &mut Criterion) {
    let dir = bench_cat_dir();
    c.bench_function("BenchmarkCat10MB", |b| {
        b.iter(|| run_cli(dir, &["cat"]))
    });
}

fn bench_config() -> Criterion {
    Criterion::default().measurement_time(std::time::Duration::from_secs(10))
}

criterion_group! {
    name = cli_benches;
    config = bench_config();
    targets = bench_ls, bench_ls_all, bench_ls_json, bench_ls_json_all, bench_attr, bench_push, bench_push_100, bench_cat
}
criterion_main!(cli_benches);
