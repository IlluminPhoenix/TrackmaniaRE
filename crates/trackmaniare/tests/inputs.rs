//! Trackmania 2020 input stream decoding.
//!
//! The bit layout follows the GBX.NET reference implementation
//! (`CGameCtnGhost.PlayerInputData` → `ProcessTrackmaniaInputs`): a stream of
//! per-tick deltas, least significant bit first, where every tick starts with
//! a "nothing changed" flag.

use trackmaniare::inputs::{BitReader, Event, PlayerInputData, decode_trackmania_inputs};

/// Writes values least significant bit first, like the game does.
struct BitWriter {
    bytes: Vec<u8>,
    bit: usize,
}

impl BitWriter {
    fn new() -> Self {
        BitWriter {
            bytes: Vec::new(),
            bit: 0,
        }
    }

    fn push(&mut self, value: u64, bits: u32) {
        for i in 0..bits {
            let set = (value >> i) & 1 == 1;
            let byte = self.bit / 8;
            if byte >= self.bytes.len() {
                self.bytes.push(0);
            }
            if set {
                self.bytes[byte] |= 1 << (self.bit % 8);
            }
            self.bit += 1;
        }
    }
}

#[test]
fn bit_reader_is_least_significant_bit_first() {
    let data = [0b1010_0101u8, 0b0000_0011];
    let mut r = BitReader::new(&data);
    let mut bits = Vec::new();
    for _ in 0..16 {
        bits.push(r.read_bit().unwrap());
    }
    assert_eq!(
        bits,
        vec![
            true, false, true, false, false, true, false, true, true, true, false, false, false,
            false, false, false
        ]
    );
}

#[test]
fn decodes_vehicle_input_stream() {
    let mut w = BitWriter::new();

    // Tick 0: full state, vehicle, no horn, steer 0, accelerate, no brake.
    w.push(0, 1); // sameState = false
    w.push(0, 1); // only2Bit = false
    w.push(2, 34); // states: kind = 2 (vehicle)
    w.push(1, 1); // sameMouse = true
    w.push(0, 1); // sameVehicleValue = false
    w.push(0, 8); // steer = 0
    w.push(1, 1); // accelerate
    w.push(0, 1); // brake

    // Tick 1: nothing changed at all.
    w.push(1, 1); // sameState
    w.push(1, 1); // sameMouse
    w.push(1, 1); // sameVehicleValue

    // Tick 2: only the vehicle block changes: full steer right and braking.
    w.push(1, 1); // sameState
    w.push(1, 1); // sameMouse
    w.push(0, 1); // sameVehicleValue = false
    w.push(127, 8); // steer = 127
    w.push(1, 1); // accelerate
    w.push(1, 1); // brake

    let data = PlayerInputData {
        version: 12,
        start_offset_ms: Some(100),
        ticks: 3,
        data: w.bytes,
    };

    let decoded = decode_trackmania_inputs(&data);
    assert!(decoded.stop.is_none(), "stream should decode completely");
    assert!(decoded.warnings.is_empty(), "{:?}", decoded.warnings);
    let summary: Vec<(i32, String)> = decoded
        .events
        .iter()
        .map(|e| (e.time_ms, format!("{:?}", e.event)))
        .collect();
    assert_eq!(
        summary,
        vec![
            (100, "Horn(false)".to_string()),
            (100, "Steer(0)".to_string()),
            (100, "Accelerate(true)".to_string()),
            (100, "Brake(false)".to_string()),
            (120, "Steer(127)".to_string()),
            (120, "Brake(true)".to_string()),
        ]
    );
    assert!(
        decoded
            .events
            .iter()
            .any(|e| e.event == Event::Accelerate(true) && e.tick == 0)
    );
}

#[test]
fn extreme_ticks_and_offsets_saturate_instead_of_overflowing() {
    use trackmaniare::inputs::tick_time_ms;
    assert_eq!(tick_time_ms(0, i32::MAX), i32::MAX);
    assert_eq!(tick_time_ms(1, i32::MAX), i32::MAX);
    assert_eq!(tick_time_ms(u32::MAX, 0), i32::MAX);
    assert_eq!(tick_time_ms(1, i32::MIN), i32::MIN + 10);
    assert_eq!(tick_time_ms(1234, 0), 12340);
}

#[test]
fn implausible_start_offsets_are_flagged() {
    use trackmaniare::reader::Reader;
    let mut bytes = Vec::new();
    bytes.extend(1u32.to_le_bytes()); // one entry
    bytes.extend(12u32.to_le_bytes()); // archive version
    bytes.extend(0u32.to_le_bytes()); // u01
    bytes.extend(i32::MAX.to_le_bytes()); // start offset
    bytes.extend(0u32.to_le_bytes()); // ticks
    bytes.extend(0u32.to_le_bytes()); // data length
    let mut r = Reader::new(&bytes);
    let (list, warnings) = trackmaniare::inputs::parse_player_input_data(&mut r, 4).expect("parse");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].start_offset_ms, Some(i32::MAX));
    assert!(
        warnings.iter().any(|w| w.contains("start offset")),
        "{warnings:?}"
    );
}

#[test]
fn truncated_stream_is_reported_not_fatal() {
    let data = PlayerInputData {
        version: 12,
        start_offset_ms: None,
        ticks: 4096,
        data: vec![0x00],
    };
    let decoded = decode_trackmania_inputs(&data);
    assert!(matches!(
        decoded.stop,
        Some(trackmaniare::inputs::DecodeStop::OutOfData { .. })
    ));
    // Every declared tick that could not be read produced no event, and the
    // decoder must not panic.
    assert!(decoded.events.len() <= 4096);
}

#[test]
fn unsupported_input_kind_stops_decoding() {
    let mut w = BitWriter::new();
    w.push(0, 1); // sameState = false
    w.push(0, 1); // only2Bit = false
    w.push(0xC, 34); // states: kind 0xC (float-based steering)

    let data = PlayerInputData {
        version: 12,
        start_offset_ms: None,
        ticks: 4,
        data: w.bytes,
    };
    let decoded = decode_trackmania_inputs(&data);
    assert!(matches!(
        decoded.stop,
        Some(trackmaniare::inputs::DecodeStop::UnsupportedKind { kind: 0xC, .. })
    ));
    assert!(
        decoded.events.is_empty(),
        "no events from an unsupported kind"
    );
}

#[test]
fn corrupted_strafe_value_stops_decoding() {
    let mut w = BitWriter::new();
    w.push(0, 1); // sameState = false
    w.push(0, 1); // only2Bit = false
    w.push(1, 34); // states: kind 1 (character)
    w.push(1, 1); // sameMouse
    w.push(0, 1); // sameCharacterValue = false
    w.push(2, 2); // strafe = 2 (corrupted)
    w.push(0, 2); // walk
    w.push(0, 2); // vertical
    w.push(0, 2); // horizontal

    let data = PlayerInputData {
        version: 12,
        start_offset_ms: None,
        ticks: 4,
        data: w.bytes,
    };
    let decoded = decode_trackmania_inputs(&data);
    assert!(matches!(
        decoded.stop,
        Some(trackmaniare::inputs::DecodeStop::CorruptedValue { what: "strafe", .. })
    ));
    assert!(
        !decoded
            .events
            .iter()
            .any(|e| matches!(e.event, Event::Strafe(_))),
        "corrupted values must not be emitted"
    );
}

#[test]
fn character_stream_uses_two_bit_fields() {
    let mut w = BitWriter::new();
    // Character pilot (kind = 1) jumping forward with strafe right.
    w.push(0, 1); // sameState
    w.push(0, 1); // only2Bit
    w.push(1 | (1 << 12), 34); // kind 1, jump bit
    w.push(1, 1); // sameMouse
    w.push(0, 1); // sameCharacterValue
    w.push(3, 2); // strafe = right
    w.push(0, 2); // walk = none
    w.push(0, 2); // vertical
    w.push(0, 2); // horizontal

    let data = PlayerInputData {
        version: 12,
        start_offset_ms: None,
        ticks: 1,
        data: w.bytes,
    };
    let decoded = decode_trackmania_inputs(&data);
    assert!(decoded.stop.is_none());
    let kinds: Vec<&Event> = decoded.events.iter().map(|e| &e.event).collect();
    assert!(kinds.contains(&&Event::Jump(true)));
    assert!(kinds.contains(&&Event::Strafe(3)));
}
