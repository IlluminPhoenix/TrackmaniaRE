//! End-to-end parsing against real Trackmania 2020 replays.
//!
//! `tm2020-short` is a 2.6 second stadium replay; it exercises the container
//! (LZO body, header chunks), the replay record, the ghost node, the TM2020
//! input stream and the vehicle record data.
//!
//! `tm2020-merged-record` is a 7.6 second TAS/preview export whose samples
//! live in a record data node at replay level (`0x0911F000`) instead of in
//! the ghost.
//!
//! The fixtures are sanitized: the nickname, zone, account id and wall-clock
//! timestamps are placeholders, so the test data does not identify a player.

use trackmaniare::{model::GhostInputs, parse_replay};

fn fixture() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/tm2020-short.Replay.Gbx"
    ))
    .expect("fixture replay")
}

fn merged_fixture() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/tm2020-merged-record.Replay.Gbx"
    ))
    .expect("merged-record fixture")
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
    assert_eq!(
        ex.replay.author_zone.as_deref(),
        Some("World|Europe|Testlan|Test|TestTownXYZW")
    );
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
    assert_eq!(ghost.walltime_start, Some(1_577_836_800));
    assert_eq!(ghost.walltime_end, Some(1_577_836_803));

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
            .next_back()
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
    // The two encoders agree to within one steer quantisation step (measured
    // worst case on this fixture: 0.003922 = 1/255). A 0.01 bound still leaves
    // 2.5x headroom while failing on any real steering regression.
    assert!(
        worst < 0.01,
        "worst steer difference {worst} over {compared} samples"
    );
}

#[test]
fn speed_agrees_with_distance_between_samples() {
    // The 20 Hz record data and the position/speed fields share an encoder, so
    // distance over time must match the recorded speed away from standstill.
    // Stated tolerances (this fixture: median deviation 0.3%, p90 1.9%,
    // worst 6.3%; over a 692-replay collection: median 0.2%, p90 2.7%, with
    // respawn/teleport jumps as the only large outliers).
    let ex = parse_replay(&fixture(), "fixture", false).unwrap();
    let ghost = &ex.ghosts[0];
    let record = ghost.record_data.as_ref().unwrap();
    let mut ratios = Vec::new();
    for list in &record.lists {
        for pair in list.samples.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let (Some(va), Some(vb)) = (&a.vehicle, &b.vehicle) else {
                continue;
            };
            let dt = (b.time_ms - a.time_ms) as f32 / 1000.0;
            let speed = (va.speed + vb.speed) / 2.0;
            // Below 5 m/s quantisation noise dominates the ratio.
            if dt <= 0.0 || speed < 5.0 {
                continue;
            }
            let dist = va
                .position
                .iter()
                .zip(&vb.position)
                .map(|(x, y)| (y - x) * (y - x))
                .sum::<f32>()
                .sqrt();
            ratios.push((dist / dt) / speed);
        }
    }
    assert!(
        ratios.len() > 50,
        "only {} sample pairs compared",
        ratios.len()
    );
    let mut deviations: Vec<f32> = ratios.iter().map(|r| (r - 1.0).abs()).collect();
    deviations.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = deviations[deviations.len() / 2];
    let p90 = deviations[deviations.len() * 9 / 10];
    let worst = *deviations.last().unwrap();
    assert!(median < 0.05, "median deviation {median}");
    assert!(p90 < 0.10, "p90 deviation {p90}");
    assert!(worst < 0.25, "worst deviation {worst}");
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

#[test]
fn replay_level_record_data_is_decoded() {
    // Merge/preview tooling writes one combined `CPlugEntRecordData` at
    // replay level. The walk used to stop at that chunk; the samples must be
    // decoded and the chunks after the node must still be visited.
    let ex = parse_replay(&merged_fixture(), "fixture", false).unwrap();
    assert!(
        ex.warnings.is_empty(),
        "unexpected warnings: {:?}",
        ex.warnings
    );
    assert!(ex.container.body_complete);

    let record = ex
        .replay
        .record_data
        .as_ref()
        .expect("replay-level record data");
    assert_eq!(record.version, 11);
    assert_eq!(record.end_ms, Some(19540));
    assert_eq!(record.vehicle_sample_count(), 391);
    assert_eq!(record.other_sample_count(), 155);

    // The ghost carries a legacy blob, not a modern record: the samples above
    // are the only decoded car states in the file.
    assert_eq!(ex.ghosts.len(), 1);
    assert!(ex.ghosts[0].record_data.is_none());
    assert_eq!(ex.ghosts[0].sample_count(), 0);
    assert_eq!(ex.replay.time_ms, Some(7603));

    // The walk continued past the record node's end-of-node marker into the
    // trailing (skippable) chunks of the replay record.
    let trailing: Vec<u32> = ex
        .chunks
        .iter()
        .skip_while(|c| c.id != 0x0911_F000)
        .skip(1)
        .map(|c| c.id)
        .collect();
    assert_eq!(
        trailing,
        vec![
            0x0309_3025,
            0x0309_3026,
            0x0309_3027,
            0x0309_3028,
            0x0309_3029
        ]
    );
}
