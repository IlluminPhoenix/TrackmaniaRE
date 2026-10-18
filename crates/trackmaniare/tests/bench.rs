//! Bench harness behaviour: clean runs, failure reporting, crash survival
//! and report files.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_trackmaniare-bench")
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tm2020-short.Replay.Gbx")
}

/// A fresh, empty directory, unique per test.
fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("trackmaniare-bench-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn copy_fixture(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::copy(fixture(), &path).expect("copy fixture");
    path
}

fn run<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new(bin()).args(args).output().expect("run binary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn clean_collection_exits_zero_and_writes_reports() {
    let dir = temp_dir("clean");
    copy_fixture(&dir, "a.Replay.Gbx");
    // A nested copy: directory scans must be recursive.
    let nested = dir.join("sub");
    std::fs::create_dir_all(&nested).unwrap();
    copy_fixture(&nested, "b.Replay.Gbx");

    let json = dir.join("report.json");
    let csv = dir.join("report.csv");
    let output = run([
        std::ffi::OsStr::new("--json"),
        json.as_os_str(),
        std::ffi::OsStr::new("--csv"),
        csv.as_os_str(),
        dir.as_os_str(),
    ]);
    assert!(output.status.success(), "clean collection must exit 0");

    let text = stdout(&output);
    assert!(text.contains("ok 2"), "both files parsed: {text}");
    assert!(text.contains("partial 0, errors 0, panics 0"));
    assert!(text.contains("all 2 files parsed"));

    let report = std::fs::read_to_string(&json).expect("json report");
    assert!(report.contains("\"ok\": 2"), "summary counts: {report}");
    assert!(report.contains("\"files\": ["), "per-file array");

    let csv_text = std::fs::read_to_string(&csv).expect("csv report");
    assert_eq!(csv_text.lines().count(), 3, "header plus two rows");
}

#[test]
fn unreadable_file_is_reported_and_fails_the_run() {
    let dir = temp_dir("garbage");
    copy_fixture(&dir, "a.Replay.Gbx");
    std::fs::write(dir.join("broken.Replay.Gbx"), b"this is not a gbx file").unwrap();

    let output = run([dir.as_os_str()]);
    assert!(!output.status.success(), "a broken file must fail the run");

    let text = stdout(&output);
    assert!(text.contains("ERROR"), "issue line: {text}");
    assert!(text.contains("broken.Replay.Gbx"));
    assert!(text.contains("errors 1"));
    assert!(text.contains("1 of 2 files failed"));
}

#[test]
fn panics_are_caught_and_the_run_continues() {
    let dir = temp_dir("panic");
    copy_fixture(&dir, "a.Replay.Gbx");
    copy_fixture(&dir, "b.Replay.Gbx");

    let output = run([
        std::ffi::OsStr::new("--simulate-panic"),
        std::ffi::OsStr::new("1"),
        dir.as_os_str(),
    ]);
    assert!(!output.status.success(), "a panic must fail the run");

    let text = stdout(&output);
    assert!(text.contains("PANIC"), "panic listed: {text}");
    assert!(text.contains("simulated parser panic"));
    assert!(text.contains("panics 1"));
    assert!(text.contains("ok 1"), "the other file was still parsed");
    assert!(text.contains("1 of 2 files failed"));
}

#[test]
fn partially_parsed_file_is_flagged_and_fails_the_run() {
    let dir = temp_dir("partial");
    let data = std::fs::read(fixture()).expect("fixture");
    // A file cut off inside the compressed body parses the header, then the
    // replay record stops early. That is a partial parse, not a clean warning,
    // so it must be visible and fail the run.
    let cut = data.len() * 3 / 4;
    std::fs::write(dir.join("truncated.Replay.Gbx"), &data[..cut]).unwrap();

    let output = run([dir.as_os_str()]);
    assert!(
        !output.status.success(),
        "a partial parse must fail the run"
    );

    let text = stdout(&output);
    assert!(text.contains("PARTIAL"), "{text}");
    assert!(text.contains("partial 1"), "{text}");
    assert!(text.contains("1 of 1 files failed or parsed only partially"));
}

#[test]
fn repeat_parses_each_file_multiple_times() {
    let output = run([
        std::ffi::OsStr::new("--repeat"),
        std::ffi::OsStr::new("3"),
        fixture().as_os_str(),
    ]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("3 repeats per file"), "{text}");
    assert!(text.contains("all 1 files parsed"));
}

#[test]
fn replay_level_record_samples_are_counted() {
    // Replay-level record data (merge/preview tooling) must count towards the
    // sample totals even though no ghost carries a modern record.
    let dir = temp_dir("merged-record");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tm2020-merged-record.Replay.Gbx");
    std::fs::copy(&source, dir.join("merged.Replay.Gbx")).unwrap();

    let output = run([dir.as_os_str()]);
    assert!(
        output.status.success(),
        "merged record must parse cleanly: {}",
        stdout(&output)
    );
    let text = stdout(&output);
    assert!(text.contains("ok 1"), "{text}");
    assert!(
        text.contains("samples    1/1 files with car samples"),
        "{text}"
    );
}
