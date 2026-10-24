//! TICK script rendering over real replay fixtures.
//!
//! The formatter only emits the part of TICK's format that could be verified
//! against TICK itself, so these tests pin the vocabulary and the line
//! grammar, and check that the output agrees with the decoded event stream.

use std::path::{Path, PathBuf};

use trackmaniare::inputs::Event;
use trackmaniare::model::{Extraction, GhostInputs};
use trackmaniare::parse_replay;
use trackmaniare::tas::{ghosts_with_tas, tas_script};

fn parse(name: &str) -> Extraction {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let data = std::fs::read(&path).expect("read fixture");
    parse_replay(&data, name, false).expect("parse fixture")
}

/// Asserts every line is `<time_ms> <action>` or `<time_ms> <action> <value>`
/// with the vocabulary this writer claims, and returns the line count.
fn assert_valid_script(text: &str) -> usize {
    let mut lines = 0;
    let mut last_time: i64 = 0;
    for line in text.lines() {
        lines += 1;
        let parts: Vec<&str> = line.split(' ').collect();
        assert!((2..=3).contains(&parts.len()), "bad line: {line:?}");
        let time: i64 = parts[0]
            .parse()
            .unwrap_or_else(|_| panic!("bad time: {line:?}"));
        assert!(time >= 0, "negative time: {line:?}");
        assert!(time >= last_time, "time went backwards: {line:?}");
        last_time = time;
        match parts[1] {
            "seed" => {
                assert_eq!(parts.len(), 3, "seed takes a value: {line:?}");
                assert_eq!(time, 0, "seed must sit at time 0: {line:?}");
                parts[2]
                    .parse::<i64>()
                    .unwrap_or_else(|_| panic!("bad seed: {line:?}"));
            }
            "steer" => {
                let v: i32 = parts[2]
                    .parse()
                    .unwrap_or_else(|_| panic!("bad steer: {line:?}"));
                assert!((-127..=127).contains(&v), "steer out of range: {line:?}");
            }
            "accel" | "brake" => {
                assert!(matches!(parts[2], "0" | "1"), "bad pedal value: {line:?}")
            }
            "respawn" | "srespawn" => {
                assert_eq!(parts.len(), 2, "respawn takes no value: {line:?}")
            }
            other => panic!("unexpected action {other:?} in {line:?}"),
        }
    }
    lines
}

#[test]
fn short_fixture_renders_the_initial_state() {
    let ex = parse("tm2020-short.Replay.Gbx");
    let script = tas_script(&ex, 0).expect("short fixture has a TM2020 input stream");
    assert_eq!(script.text, "0 seed 25665675\n0 accel 1\n");
    assert_eq!(script.lines, 2);
    // The fixture's first state is neutral (steer 0, brake 0), so those lines
    // are not written — the script's implied initial state already is. It
    // also carries horn and CharacterPilot events (gun, action, strafe, walk,
    // vertical, horizontal), which have no TICK keyword; they are counted,
    // not written.
    assert_eq!(script.dropped_events, 7);
}

#[test]
fn merged_record_fixture_renders_a_well_formed_script() {
    let ex = parse("tm2020-merged-record.Replay.Gbx");
    let script = tas_script(&ex, 0).expect("fixture has a TM2020 input stream");
    assert!(script.lines > 100, "expected a sizeable script: {script:?}");
    assert_eq!(assert_valid_script(&script.text), script.lines);

    let times: Vec<i64> = script
        .text
        .lines()
        .map(|l| l.split(' ').next().unwrap().parse().unwrap())
        .collect();
    assert!(
        times.windows(2).all(|w| w[0] <= w[1]),
        "times must be non-decreasing"
    );

    let GhostInputs::Trackmania2020 { events, .. } = ex.ghosts[0].inputs.as_ref().unwrap() else {
        panic!("fixture holds TM2020 inputs");
    };

    // One `steer` line per decoded steering change, at the decoded time; the
    // only exception is a first state that equals the neutral 0, which the
    // script's implied initial state already is.
    let steers: Vec<&trackmaniare::inputs::InputEvent> = events
        .iter()
        .filter(|e| matches!(e.event, Event::Steer(_)))
        .collect();
    for (i, e) in steers.iter().enumerate() {
        let Event::Steer(v) = e.event else {
            unreachable!()
        };
        if i == 0 && v == 0 {
            continue; // neutral initial state, never written
        }
        assert!(
            script.text.contains(&format!("{} steer {v}\n", e.time_ms)),
            "missing steer line for {e:?}"
        );
    }
    let steer_lines = script
        .text
        .lines()
        .filter(|l| l.split(' ').nth(1) == Some("steer"))
        .count();
    let first_steer_is_neutral =
        matches!(steers.first(), Some(e) if matches!(e.event, Event::Steer(0)));
    assert_eq!(
        steer_lines,
        steers.len() - usize::from(first_steer_is_neutral)
    );

    // The script pins the ghost's validation seed on its first line.
    let seed = ex.ghosts[0]
        .validation
        .validation_seed
        .expect("fixture has a validation seed");
    assert!(script.text.starts_with(&format!("0 seed {seed}\n")));

    // Only the verified vocabulary is ever written.
    let actions: std::collections::BTreeSet<&str> = script
        .text
        .lines()
        .map(|l| l.split(' ').nth(1).unwrap())
        .collect();
    assert!(
        actions.is_subset(&std::collections::BTreeSet::from([
            "seed", "steer", "accel", "brake", "respawn", "srespawn"
        ])),
        "unexpected actions: {actions:?}"
    );
}

#[test]
fn scripts_are_listed_for_exactly_the_ghosts_with_tm2020_inputs() {
    for name in ["tm2020-short.Replay.Gbx", "tm2020-merged-record.Replay.Gbx"] {
        let ex = parse(name);
        let listed = ghosts_with_tas(&ex);
        let expected: Vec<usize> = ex
            .ghosts
            .iter()
            .enumerate()
            .filter(|(_, g)| matches!(g.inputs, Some(GhostInputs::Trackmania2020 { .. })))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(listed, expected, "{name}");
        for gi in listed {
            assert_eq!(
                assert_valid_script(&tas_script(&ex, gi).unwrap().text),
                tas_script(&ex, gi).unwrap().lines
            );
        }
    }
}
