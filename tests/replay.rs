//! End-to-end parsing against a real Trackmania 2020 replay.
//!
//! The fixture is a 2.6 second stadium replay; it exercises the container
//! (LZO body, header chunks), the replay record, the ghost node, the TM2020
//! input stream and the vehicle record data.

use trackmaniare::{model::GhostInputs, parse_replay};

fn fixture() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/tm2020-short.Replay.Gbx"
    ))
    .expect("fixture replay")
}

#[test]
fn container_is_read_completely() {
    let ex = parse_replay(&fixture(), "fixture", false).unwrap();
    assert_eq!(ex.container.class_id, 0x0309_3000, "replay record class");
    assert_eq!(ex.container.gbx_version, 6);
    assert_eq!(ex.container.body_compression, 'C');
    assert!(ex.container.body_complete, "body should decompress fully");
    assert_eq!(
        ex.container.body_size, ex.container.body_uncompressed_size,
        "decompressed body size should match the header"
    );
    assert!(
        ex.warnings.is_empty(),
        "unexpected warnings: {:?}",
        ex.warnings
    );
}

#[test]
fn replay_metadata_is_extracted() {
    let ex = parse_replay(&fixture(), "fixture", false).unwrap();
    assert_eq!(ex.replay.time_ms, Some(2590));
    assert_eq!(ex.replay.nickname.as_deref(), Some("TestDriver0001"));
    let attr = |k: &str| {
        ex.replay
            .xml_attributes
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(attr("name").as_deref(), Some("Lag Map"));
    assert_eq!(attr("author").as_deref(), Some("FixtureAuthor000000000"));
    assert_eq!(attr("exever").as_deref(), Some("3.3.0"));
    assert_eq!(attr("title").as_deref(), Some("TMStadium"));
    assert_eq!(ex.replay.author_nickname.as_deref(), Some("TestDriver0001"));
    assert!(
        ex.replay.embedded_map_size.unwrap() > 1000,
        "embedded map present"
    );
}

#[test]
fn ghost_metadata_and_samples_are_extracted() {
    let ex = parse_replay(&fixture(), "fixture", false).unwrap();
    assert_eq!(ex.ghosts.len(), 1);
    let ghost = &ex.ghosts[0];
    assert_eq!(ghost.nickname.as_deref(), Some("TestDriver0001"));
    assert_eq!(ghost.player_model_text(), "CarSport");
    assert_eq!(ghost.race_time_ms, Some(2590));
    assert_eq!(ghost.respawns, Some(0));
    assert_eq!(ghost.checkpoints.len(), 1);
    assert!(ghost.walltime_start.is_some() && ghost.walltime_end.is_some());

    let record = ghost.record_data.as_ref().expect("record data");
    // The entity list holds several tracks (car, items, objects) and all of
    // them must be decoded, not just the first one.
    assert!(
        record.lists.len() >= 3,
        "expected several entity lists, got {}",
        record.lists.len()
    );
    assert!(
        ghost.other_sample_count() > 0,
        "non-vehicle entity tracks present"
    );
    let vehicle_list = record
        .lists
        .iter()
        .find(|l| l.class_id == Some(0x0A01_8000))
        .expect("vehicle sample list");
    assert!(
        vehicle_list.samples.len() > 50,
        "samples: {}",
        vehicle_list.samples.len()
    );

    // The samples are 50 ms apart and cover the run.
    assert_eq!(vehicle_list.samples[0].time_ms, 0);
    let step = vehicle_list.samples[1].time_ms - vehicle_list.samples[0].time_ms;
    assert_eq!(step, 50);

    // The car is on the track: coordinates are plausible map units and the
    // speed never exceeds a sane top speed.
    for s in &vehicle_list.samples {
        let v = s.vehicle.as_ref().expect("decoded vehicle sample");
        assert!(v.position.iter().all(|c| c.is_finite()));
        assert!((0.0..200.0).contains(&v.speed), "speed {} m/s", v.speed);
        assert!((-1.01..=1.01).contains(&v.steer), "steer {}", v.steer);
        assert!((-0.01..=1.01).contains(&v.gas));
        assert!((-0.01..=1.01).contains(&v.brake));
    }
}

#[test]
fn inputs_are_decoded_and_plausible() {
    let ex = parse_replay(&fixture(), "fixture", false).unwrap();
    let ghost = &ex.ghosts[0];
    let Some(GhostInputs::Trackmania2020 {
        version,
        ticks,
        events,
        truncated,
        ..
    }) = &ghost.inputs
    else {
        panic!("expected TM2020 inputs, got {:?}", ghost.inputs.is_some());
    };
    assert_eq!(*version, 12);
    assert!(!truncated, "input stream should decode completely");
    assert!(*ticks > 300);
    assert!(!events.is_empty());

    // Acceleration is pressed at some point, and every event is inside the run.
    let has_accel = events
        .iter()
        .any(|e| matches!(e.event, trackmaniare::inputs::Event::Accelerate(true)));
    assert!(has_accel, "a 2.6 s run must contain throttle input");
    for e in events {
        assert!(e.time_ms < 3000, "event after the run finished: {e:?}");
        assert_eq!(e.time_ms % 10, 0, "events land on 10 ms ticks");
    }
}

#[test]
fn input_steer_agrees_with_the_recorded_wheel_state() {
    // Independent cross-check: the bit-packed inputs and the 20 Hz vehicle
    // record come from different encoders but describe the same steering.
    let ex = parse_replay(&fixture(), "fixture", false).unwrap();
    let ghost = &ex.ghosts[0];
    let Some(GhostInputs::Trackmania2020 {
        events,
        start_offset_ms,
        ..
    }) = &ghost.inputs
    else {
        panic!("expected inputs");
    };
    let offset = start_offset_ms.unwrap_or(0);
    let mut timeline: Vec<(i32, f32)> = Vec::new();
    for e in events {
        if let trackmaniare::inputs::Event::Steer(v) = e.event {
            timeline.push((e.time_ms, v as f32 / 127.0));
        }
    }
    let steer_at = |t: i32| -> Option<f32> {
        timeline
            .iter()
            .filter(|(tt, _)| *tt <= t)
            .map(|(_, v)| *v)
            .last()
    };

    let record = ghost.record_data.as_ref().unwrap();
    let list = record
        .lists
        .iter()
        .find(|l| l.class_id == Some(0x0A01_8000))
        .unwrap();
    let mut compared = 0;
    let mut worst = 0f32;
    for s in &list.samples {
        let Some(v) = &s.vehicle else { continue };
        let Some(expected) = steer_at(s.time_ms - offset) else {
            continue;
        };
        worst = worst.max((v.steer - expected).abs());
        compared += 1;
    }
    assert!(compared > 50, "only {compared} samples compared");
    // The wheel state is filtered, so it tracks rather than equals the input.
    assert!(
        worst < 0.6,
        "worst steer difference {worst} over {compared} samples"
    );
}

#[test]
fn embedded_map_can_be_extracted() {
    let data = fixture();
    let with_map = parse_replay(&data, "fixture", true).unwrap();
    let map = with_map.embedded_map.expect("map bytes");
    assert_eq!(&map[..3], b"GBX");
    assert!(map.len() > 1000);
}

#[test]
fn rejects_non_gbx_input() {
    assert!(parse_replay(b"not a gbx file at all", "x", false).is_err());
}
