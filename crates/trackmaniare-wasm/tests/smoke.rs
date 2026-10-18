//! Native smoke test for the WASM shim: the bindings compile on any target,
//! so this exercises the real API against the end-to-end replay fixture
//! without going through a browser.
//!
//! The error path is deliberately not tested here: `JsError` construction
//! requires a real JS runtime (`wasm-pack test --node`), and natively
//! wasm-bindgen's stubs abort. Parser robustness is covered by the core
//! crate's `tests/robustness.rs`.

use trackmaniare_wasm::Replay;

fn fixture() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../trackmaniare/tests/fixtures/tm2020-short.Replay.Gbx"
    ))
    .expect("read fixture")
}

#[test]
fn fixture_parses_through_the_shim() {
    let replay = Replay::new(&fixture(), "fixture.Replay.Gbx")
        .unwrap_or_else(|_| panic!("fixture must parse"));
    assert_eq!(replay.ghost_count(), 1);

    let meta = replay.metadata_json();
    assert!(meta.contains("\"ghosts\""), "metadata looks wrong");
    assert!(
        meta.contains("\"container\""),
        "container info is in metadata"
    );

    // Samples: the car track is present and every array agrees on its length.
    let track = replay.vehicle_track_index(0).expect("car track");
    assert_eq!(
        replay.track_class_id(0, track),
        0x0A01_8000,
        "vehicle track class id"
    );
    let n = replay.track_sample_count(0, track);
    assert!(n > 50, "only {n} samples");
    assert_eq!(replay.track_times_ms(0, track).len(), n);
    assert_eq!(replay.track_positions(0, track).len(), n * 3);
    assert_eq!(replay.track_rotations(0, track).len(), n * 3);
    assert_eq!(replay.track_speeds_mps(0, track).len(), n);
    assert_eq!(replay.track_steer(0, track).len(), n);
    let positions = replay.track_positions(0, track);
    assert!(positions.iter().all(|v| v.is_finite()));
    assert!(
        positions
            .chunks(3)
            .all(|p| p[0] != 0.0 || p[1] != 0.0 || p[2] != 0.0),
        "positions decode to real coordinates"
    );

    // Inputs: dense arrays are parallel and as long as the tick count.
    assert!(replay.input_ticks(0) > 0);
    let steer = replay.input_steer(0);
    let flags = replay.input_flags(0);
    assert_eq!(steer.len(), flags.len());
    assert_eq!(steer.len(), replay.input_ticks(0) as usize);
    assert!(replay.inputs_json(0).contains("\"tm2020-bitpacked\""));

    // Text exports are non-empty and aligned with the data.
    assert!(replay.inputs_csv().contains("time_ms"));
    assert!(replay.inputs_dense_csv().lines().count() > 2);
    assert!(replay.samples_csv().lines().count() > 2);

    // The map is kept for export and is a real GameBox when present.
    let map = replay.map_bytes();
    if !map.is_empty() {
        assert_eq!(&map[..3], b"GBX", "embedded map is a GameBox");
    }
}
