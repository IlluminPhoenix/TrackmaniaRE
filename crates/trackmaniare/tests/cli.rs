//! CLI behaviour: quiet mode, flag order, output naming and the raw dump.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_TrackmaniaRE")
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tm2020-short.Replay.Gbx")
}

/// A fresh, empty directory, unique per test.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("trackmaniare-cli-{}-{name}", std::process::id()));
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

#[test]
fn quiet_suppresses_the_per_file_separator() {
    let dir = temp_dir("quiet");
    copy_fixture(&dir, "a.Replay.Gbx");
    copy_fixture(&dir, "b.Replay.Gbx");

    let loud = run([dir.as_os_str()]);
    let text = String::from_utf8_lossy(&loud.stdout);
    assert_eq!(text.matches("=== ").count(), 2);

    let quiet = run([std::ffi::OsStr::new("--quiet"), dir.as_os_str()]);
    let text = String::from_utf8_lossy(&quiet.stdout);
    assert_eq!(text.matches("=== ").count(), 0);
    assert_eq!(text.matches("file:").count(), 2, "summaries still print");
}

#[test]
fn flag_order_does_not_change_out_dir_behaviour() {
    let dir = temp_dir("order");
    let replay = copy_fixture(&dir, "a.Replay.Gbx");
    let out1 = dir.join("out1");
    let out2 = dir.join("out2");

    let first = run([
        std::ffi::OsStr::new("-s"),
        std::ffi::OsStr::new("-o"),
        out1.as_os_str(),
        replay.as_os_str(),
    ]);
    let second = run([
        std::ffi::OsStr::new("-o"),
        out2.as_os_str(),
        std::ffi::OsStr::new("-s"),
        replay.as_os_str(),
    ]);

    for (output, out) in [(&first, &out1), (&second, &out2)] {
        assert!(output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("file:"),
            "summary expected"
        );
        assert_eq!(
            std::fs::read_dir(out).unwrap().count(),
            0,
            "an explicit --summary must not write files"
        );
    }
}

#[test]
fn same_stem_replays_do_not_collide() {
    let dir = temp_dir("collision");
    let d1 = dir.join("d1");
    let d2 = dir.join("d2");
    std::fs::create_dir_all(&d1).unwrap();
    std::fs::create_dir_all(&d2).unwrap();
    copy_fixture(&d1, "Race.Replay.Gbx");
    copy_fixture(&d2, "Race.Replay.Gbx");
    let out = dir.join("out");

    let output = run([
        std::ffi::OsStr::new("-j"),
        std::ffi::OsStr::new("-o"),
        out.as_os_str(),
        d1.as_os_str(),
        d2.as_os_str(),
    ]);
    assert!(output.status.success());

    for expected in [
        "Race.Replay.json",
        "Race.Replay-2.json",
        "Race.Replay.inputs.csv",
        "Race.Replay-2.inputs.csv",
        "Race.Replay.samples.csv",
        "Race.Replay-2.samples.csv",
    ] {
        assert!(out.join(expected).is_file(), "{expected} missing");
    }
}

#[test]
fn raw_dump_contains_real_header_chunk_payloads() {
    let dir = temp_dir("raw");
    let replay = copy_fixture(&dir, "a.Replay.Gbx");
    let out = dir.join("out");
    let output = run([
        std::ffi::OsStr::new("--raw"),
        std::ffi::OsStr::new("-o"),
        out.as_os_str(),
        replay.as_os_str(),
    ]);
    assert!(output.status.success());

    // Header chunk 0x03093000 holds the replay version (8), not the body.
    let version_chunk = out.join("a.Replay.chunks/000000_header_03093000.bin");
    let bytes = std::fs::read(&version_chunk).expect("version chunk dump");
    assert_eq!(bytes.len(), 143, "declared header chunk size");
    assert_eq!(&bytes[..4], &8u32.to_le_bytes());
    assert_ne!(&bytes[..3], b"GBX", "body bytes must not leak into it");

    // Header chunk 0x03093001 holds the community XML string.
    let xml_chunk = out.join("a.Replay.chunks/000000_header_03093001.bin");
    let bytes = std::fs::read(&xml_chunk).expect("xml chunk dump");
    assert_eq!(&bytes[4..11], b"<header");
}

#[test]
fn tas_flag_writes_a_tick_script() {
    let dir = temp_dir("tas");
    let replay = copy_fixture(&dir, "a.Replay.Gbx");
    let out = dir.join("out");
    let output = run([
        std::ffi::OsStr::new("-T"),
        std::ffi::OsStr::new("-o"),
        out.as_os_str(),
        replay.as_os_str(),
    ]);
    assert!(output.status.success());

    let script = std::fs::read_to_string(out.join("a.Replay.tas.txt")).expect("script written");
    assert_eq!(script, "0 seed 25665675\n0 accel 1\n");
}

#[test]
fn double_dash_allows_paths_starting_with_a_dash() {
    let dir = temp_dir("dashdash");
    copy_fixture(&dir, "-weird.Replay.Gbx");
    let output = Command::new(bin())
        .current_dir(&dir)
        .args(["--", "-weird.Replay.Gbx"])
        .output()
        .expect("run binary");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("file:"));
}
