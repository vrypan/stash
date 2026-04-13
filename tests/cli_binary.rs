use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn test_stash_dir() -> TempDir {
    tempfile::Builder::new()
        .prefix("stash-test-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap()
}

fn stash_cmd(stash_dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("stash").unwrap();
    cmd.env("STASH_DIR", stash_dir);
    cmd
}

#[cfg(feature = "completion")]
fn completion_cmd() -> Command {
    Command::cargo_bin("stash-completion").unwrap()
}

fn stdout_string(cmd: &mut Command) -> String {
    String::from_utf8(cmd.assert().success().get_output().stdout.clone()).unwrap()
}

fn push_text(stash_dir: &Path, text: &str, attrs: &[&str]) -> String {
    let mut cmd = stash_cmd(stash_dir);
    cmd.arg("push").arg("--print=stdout");
    for attr in attrs {
        cmd.args(["-a", attr]);
    }
    cmd.write_stdin(text);
    stdout_string(&mut cmd).trim().to_string()
}

fn push_file(stash_dir: &Path, path: &Path, attrs: &[&str]) -> String {
    let mut cmd = stash_cmd(stash_dir);
    cmd.arg("push").arg("--print=stdout");
    for attr in attrs {
        cmd.args(["-a", attr]);
    }
    cmd.arg(path);
    stdout_string(&mut cmd).trim().to_string()
}

#[test]
fn help_mentions_smart_mode() {
    let dir = test_stash_dir();
    stash_cmd(dir.path())
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("it behaves like `stash tee`"))
        .stdout(predicate::str::contains("it behaves like `stash push`"))
        .stdout(predicate::str::contains("attrs"));
}

#[test]
fn push_and_cat_round_trip() {
    let dir = test_stash_dir();
    let id = push_text(dir.path(), "hello from stdin\n", &["kind=note"]);
    assert_eq!(id.len(), 26);

    stash_cmd(dir.path())
        .args(["cat", &id])
        .assert()
        .success()
        .stdout("hello from stdin\n");

    stash_cmd(dir.path())
        .args(["attr", &id, "kind"])
        .assert()
        .success()
        .stdout("note\n");
}

#[test]
fn bare_stash_in_pipeline_behaves_like_tee() {
    let dir = test_stash_dir();

    let mut cmd = stash_cmd(dir.path());
    cmd.arg("--print=stderr").write_stdin("pipeline body\n");
    let output = cmd.assert().success().get_output().clone();

    assert_eq!(String::from_utf8(output.stdout).unwrap(), "pipeline body\n");
    let id = String::from_utf8(output.stderr).unwrap().trim().to_string();
    assert_eq!(id.len(), 26);

    stash_cmd(dir.path())
        .args(["cat", &id])
        .assert()
        .success()
        .stdout("pipeline body\n");
}

#[test]
fn push_file_sets_filename_and_path_resolves_paths() {
    let dir = test_stash_dir();
    let file_path = dir.path().join("sample.txt");
    fs::write(&file_path, "from file\n").unwrap();

    let id = push_file(dir.path(), &file_path, &["type=file"]);

    stash_cmd(dir.path())
        .args(["attr", &id, "filename"])
        .assert()
        .success()
        .stdout("sample.txt\n");

    let data_path = dir.path().join("data").join(&id);
    let attr_path = dir.path().join("attr").join(&id);

    stash_cmd(dir.path())
        .args(["path", &id])
        .assert()
        .success()
        .stdout(format!("{}\n", data_path.display()));

    stash_cmd(dir.path())
        .args(["path", "-a", &id])
        .assert()
        .success()
        .stdout(format!("{}\n", attr_path.display()));
}

#[test]
fn tee_prints_stream_and_can_report_id_on_stderr() {
    let dir = test_stash_dir();
    let mut cmd = stash_cmd(dir.path());
    cmd.args(["tee", "--print=stderr", "-a", "flow=tee"])
        .write_stdin("alpha\nbeta\n");
    let output = cmd.assert().success().get_output().clone();

    assert_eq!(String::from_utf8(output.stdout).unwrap(), "alpha\nbeta\n");
    let id = String::from_utf8(output.stderr).unwrap().trim().to_string();
    assert_eq!(id.len(), 26);

    stash_cmd(dir.path())
        .args(["attr", &id, "flow"])
        .assert()
        .success()
        .stdout("tee\n");
}

#[test]
fn attr_supports_set_get_unset_and_json() {
    let dir = test_stash_dir();
    let id = push_text(dir.path(), "config=true\n", &[]);

    stash_cmd(dir.path())
        .args(["attr", &id, "note=keep", "label=test"])
        .assert()
        .success()
        .stdout("");

    stash_cmd(dir.path())
        .args(["attr", &id, "note", "label"])
        .assert()
        .success()
        .stdout("note\tkeep\nlabel\ttest\n");

    let mut json_cmd = stash_cmd(dir.path());
    json_cmd.args(["attr", &id, "--json"]);
    let json = stdout_string(&mut json_cmd);
    let value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["note"], "keep");
    assert_eq!(value["label"], "test");

    stash_cmd(dir.path())
        .args(["attr", &id, "--unset", "label"])
        .assert()
        .success();

    stash_cmd(dir.path())
        .args(["attr", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("note\tkeep"))
        .stdout(predicate::str::contains("label").not());
}

#[test]
fn ls_log_and_attrs_cover_current_listing_modes() {
    let dir = test_stash_dir();
    let first = push_text(dir.path(), "first body\n", &["type=text", "label=one"]);
    let second = push_text(dir.path(), "second line\n", &["type=text", "kind=sample"]);
    let file_path = dir.path().join("report.txt");
    fs::write(&file_path, "report body\n").unwrap();
    let file_id = push_file(dir.path(), &file_path, &["note=report"]);

    stash_cmd(dir.path())
        .args(["ls", "--id=full"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&second))
        .stdout(predicate::str::contains(&first));

    stash_cmd(dir.path())
        .args(["ls", "-A", "--color=false"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sample"))
        .stdout(predicate::str::contains("one"));

    stash_cmd(dir.path())
        .args(["ls", "-a", "+type", "-a", "++kind", "--color=false"])
        .assert()
        .success()
        .stdout(predicate::str::contains("text"))
        .stdout(predicate::str::contains("sample"))
        .stdout(predicate::str::contains(&second[second.len() - 8..]))
        .stdout(predicate::str::contains(&first[first.len() - 8..]).not());

    let mut ls_long_cmd = stash_cmd(dir.path());
    ls_long_cmd.args(["ls", "-l", "--color=false"]);
    let ls_long = stdout_string(&mut ls_long_cmd);
    assert!(ls_long.contains('*'));
    assert!(ls_long.contains("report body"));
    assert!(ls_long.contains(&file_id[file_id.len() - 8..]));
    assert!(!ls_long.contains("report.txt"));

    let mut ls_headers_cmd = stash_cmd(dir.path());
    ls_headers_cmd.args(["ls", "--headers", "--date", "--size", "-A", "--color=false"]);
    let ls_headers = stdout_string(&mut ls_headers_cmd);
    let mut lines = ls_headers.lines();
    let header = lines.next().unwrap();
    assert!(header.contains("id"));
    assert!(header.contains("size"));
    assert!(header.contains("date"));
    assert!(header.contains("attrs"));

    let mut ls_with_attr_cmd = stash_cmd(dir.path());
    ls_with_attr_cmd.args(["ls", "-a", "+label", "--color=false"]);
    let ls_with_attr = stdout_string(&mut ls_with_attr_cmd);
    assert!(ls_with_attr.contains(&first[first.len() - 8..]));
    assert!(ls_with_attr.contains(&second[second.len() - 8..]));
    assert!(ls_with_attr.contains("one"));

    let mut ls_filtered_cmd = stash_cmd(dir.path());
    ls_filtered_cmd.args(["ls", "-a", "label", "--id=full", "--color=false"]);
    let ls_filtered = stdout_string(&mut ls_filtered_cmd);
    assert!(ls_filtered.contains(&first));
    assert!(!ls_filtered.contains("one"));
    assert!(!ls_filtered.contains(&second));

    let mut ls_filter_and_show_cmd = stash_cmd(dir.path());
    ls_filter_and_show_cmd.args(["ls", "-a", "++label", "--id=full", "--color=false"]);
    let ls_filter_and_show = stdout_string(&mut ls_filter_and_show_cmd);
    assert!(ls_filter_and_show.contains(&first));
    assert!(ls_filter_and_show.contains("one"));
    assert!(!ls_filter_and_show.contains(&second));

    stash_cmd(dir.path())
        .args(["attrs", "--count"])
        .assert()
        .success()
        .stdout("filename\t1\nkind\t1\nlabel\t1\nnote\t1\ntype\t2\n");

    let mut ls_json_cmd = stash_cmd(dir.path());
    ls_json_cmd.args(["ls", "--json", "-a", "kind"]);
    let ls_json = stdout_string(&mut ls_json_cmd);
    let value: Value = serde_json::from_str(&ls_json).unwrap();
    let rows = value.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], second);
    assert_eq!(rows[0]["kind"], "sample");
    assert!(rows[0].get("short_id").is_some());
    assert!(rows[0].get("stack_ref").is_some());
    assert!(rows[0].get("size_human").is_some());
}

#[test]
fn rm_and_pop_remove_expected_entries() {
    let dir = test_stash_dir();
    let first = push_text(dir.path(), "one\n", &["group=a"]);
    let second = push_text(dir.path(), "two\n", &["group=b"]);
    let third = push_text(dir.path(), "three\n", &["group=a"]);

    stash_cmd(dir.path())
        .args(["rm", "-f", &first, &second])
        .assert()
        .success();

    stash_cmd(dir.path())
        .args(["ls", "--id=full"])
        .assert()
        .success()
        .stdout(format!("{third}\n"));

    let extra = push_text(dir.path(), "older\n", &["bucket=x"]);
    let newest = push_text(dir.path(), "newest\n", &["bucket=y"]);

    stash_cmd(dir.path())
        .args(["rm", "-f", "--before", &newest])
        .assert()
        .success();

    let mut ls_cmd = stash_cmd(dir.path());
    ls_cmd.args(["ls", "--id=full"]);
    let ls_out = stdout_string(&mut ls_cmd);
    assert!(ls_out.contains(&newest));
    assert!(!ls_out.contains(&extra));

    let mut pop_cmd = stash_cmd(dir.path());
    pop_cmd.arg("pop");
    let pop_out = stdout_string(&mut pop_cmd);
    assert_eq!(pop_out, "newest\n");

    stash_cmd(dir.path())
        .args(["cat", &newest])
        .assert()
        .failure();

    let first = push_text(dir.path(), "first\n", &["bucket=a"]);
    let second = push_text(dir.path(), "second\n", &["bucket=b"]);
    let third = push_text(dir.path(), "third\n", &["bucket=c"]);

    stash_cmd(dir.path())
        .args(["rm", "-f", "--after", &second])
        .assert()
        .success();

    let mut ls_after_cmd = stash_cmd(dir.path());
    ls_after_cmd.args(["ls", "--id=full"]);
    let ls_after = stdout_string(&mut ls_after_cmd);
    assert!(ls_after.contains(&second));
    assert!(ls_after.contains(&first));
    assert!(!ls_after.contains(&third));
}

#[cfg(feature = "completion")]
#[test]
fn completion_binary_smoke_test() {
    completion_cmd()
        .arg("zsh")
        .assert()
        .success()
        .stdout(predicate::str::contains("_stash"));
}
