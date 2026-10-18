//! Input extraction.
//!
//! Two formats are handled:
//!
//! * Trackmania 2020 stores a bit-packed, tick-based (100 Hz) player input
//!   stream in the ghost chunk `0x0309201D`. The bit layout lives in the
//!   GBX.NET reference implementation (`CGameCtnGhost.PlayerInputData` →
//!   `ProcessTrackmaniaInputs`,
//!   <https://github.com/BigBang1112/gbx-net>); the wiki page linked from its
//!   repository only documents the input API. This decoder follows the
//!   reference, including the states it rejects as unsupported or corrupted.
//! * Older games (TMU / TMNF / TMUF / TM2) store a list of control entries in
//!   the ghost validation chunks (`0x03092011` / `0x03092019` / `0x03092025`).

use crate::error::Result;
use crate::reader::Reader;

/// Maximum number of `PlayerInputData` entries read from one chunk. Real
/// replays contain one per ghost (team replays a handful); the cap keeps a
/// corrupt count from being taken as an allocation bound.
const MAX_PLAYER_INPUT_DATA: u32 = 16;

/// Start offsets beyond a day in either direction are treated as implausible
/// and flagged (they are kept, but timestamps saturate).
const MAX_START_OFFSET_MS: i32 = 86_400_000;

/// Least-significant-bit-first bit reader, matching the engine's format.
pub struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        BitReader { data, bit_pos: 0 }
    }

    pub fn read_bit(&mut self) -> Option<bool> {
        if self.bit_pos >= self.data.len() * 8 {
            return None;
        }
        let byte = self.data[self.bit_pos / 8];
        let bit = (byte >> (self.bit_pos % 8)) & 1 != 0;
        self.bit_pos += 1;
        Some(bit)
    }

    pub fn read_bit_u(&mut self) -> Option<u64> {
        self.read_bit().map(|b| b as u64)
    }

    pub fn read_bits(&mut self, n: usize) -> Option<u64> {
        let mut v = 0u64;
        for i in 0..n {
            v |= self.read_bit_u()? << i;
        }
        Some(v)
    }

    pub fn read_sbyte(&mut self) -> Option<i8> {
        self.read_bits(8).map(|v| v as u8 as i8)
    }

    pub fn read_byte(&mut self) -> Option<u8> {
        self.read_bits(8).map(|v| v as u8)
    }

    pub fn read_u16(&mut self) -> Option<u16> {
        self.read_bits(16).map(|v| v as u16)
    }

    /// True when every bit after the current position is zero. The reference
    /// implementation expects the buffer to be zero-padded to a byte
    /// boundary and rejects it otherwise.
    pub fn rest_is_zero(&self) -> bool {
        let mut i = self.bit_pos;
        while i < self.data.len() * 8 {
            let byte = self.data[i / 8];
            if (byte >> (i % 8)) & 1 != 0 {
                return false;
            }
            i += 1;
        }
        true
    }
}

/// One entry of the Trackmania 2020 input archive (`PlayerInputData`).
#[derive(Debug, Clone)]
pub struct PlayerInputData {
    /// 8 = ShootMania, 11 = TM 2020.04, 12 = TM 2020.07+.
    pub version: u32,
    pub start_offset_ms: Option<i32>,
    pub ticks: u32,
    pub data: Vec<u8>,
}

/// Reads the `PlayerInputData` array of ghost chunk `0x0309201D`.
///
/// Returns the entries plus any warnings (currently only a declared count that
/// exceeds [`MAX_PLAYER_INPUT_DATA`]).
pub fn parse_player_input_data(
    r: &mut Reader<'_>,
    chunk_version: u32,
) -> Result<(Vec<PlayerInputData>, Vec<String>)> {
    let count = r.u32()?;
    let mut warnings = Vec::new();
    if count > MAX_PLAYER_INPUT_DATA {
        warnings.push(format!(
            "input chunk declares {count} PlayerInputData entries; only the first {MAX_PLAYER_INPUT_DATA} are read"
        ));
    }
    let mut out = Vec::new();
    for _ in 0..count.min(MAX_PLAYER_INPUT_DATA) {
        let version = r.u32()?;
        let _u01 = r.u32()?;
        let start_offset_ms = if chunk_version >= 4 {
            let v = r.i32()?;
            if v == -1 {
                None
            } else {
                if !(-MAX_START_OFFSET_MS..=MAX_START_OFFSET_MS).contains(&v) {
                    warnings.push(format!(
                        "input stream start offset {v} ms is outside +-24 h; timestamps saturate"
                    ));
                }
                Some(v)
            }
        } else {
            None
        };
        let ticks = r.u32()?;
        let data = r.data()?;
        out.push(PlayerInputData {
            version,
            start_offset_ms,
            ticks,
            data,
        });
    }
    Ok((out, warnings))
}

/// Milliseconds from the start of the run for a 10 ms tick. Saturates instead
/// of overflowing on absurd tick counts or start offsets.
pub fn tick_time_ms(tick: u32, offset: i32) -> i32 {
    (i64::from(tick) * 10 + i64::from(offset)).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Steering axis, -127..127.
    Steer(i8),
    /// Acceleration pedal / key.
    Accelerate(bool),
    /// Brake pedal / key.
    Brake(bool),
    Horn(bool),
    /// One-way press, no release event is stored.
    Respawn,
    /// One-way press (velocity reset without double tap).
    SecondaryRespawn,
    ActionSlot(u8, bool),
    Jump(bool),
    FreeLook(bool),
    Camera2(bool),
    /// ShootMania / CharacterPilot only.
    GunTrigger(bool),
    Action(bool),
    Strafe(u8),
    Walk(u8),
    Vertical(u8),
    Horizontal(u8),
    /// Mouse position in subpixels (CharacterPilot).
    Mouse(u16, u16),
}

#[derive(Debug, Clone)]
pub struct InputEvent {
    /// Milliseconds from the start of the run.
    pub time_ms: i32,
    /// Tick index (10 ms each).
    pub tick: u32,
    pub event: Event,
}

/// Result of decoding one TM2020 input stream.
#[derive(Debug, Clone)]
pub struct InputDecode {
    /// Every event emitted before decoding stopped.
    pub events: Vec<InputEvent>,
    /// `None` when all declared ticks were decoded. Otherwise why and where
    /// decoding stopped, meaning the trailing ticks carry no data.
    pub stop: Option<DecodeStop>,
    /// Non-fatal format violations; the reference implementation throws on
    /// these, this decoder reports them and keeps the events it decoded.
    pub warnings: Vec<String>,
}

/// Why decoding stopped before the declared tick count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeStop {
    /// The bit stream ran out before the declared tick count.
    OutOfData { tick: u32 },
    /// Input kind 0xC/0xD: float-based steering, which neither the reference
    /// nor this decoder implements.
    UnsupportedKind { kind: u8, tick: u32 },
    /// A 2-bit strafe or walk value of 2, which the reference marks corrupted.
    CorruptedValue { what: &'static str, tick: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Start {
    NotStarted,
    Character,
    Vehicle,
}

/// Input kind of a full state word. `None` for the unsupported kinds 0xC/0xD.
fn get_start(states: u64) -> Option<Start> {
    match states & 0xF {
        0 => Some(Start::NotStarted),
        2..=4 => Some(Start::Vehicle),
        0xC | 0xD => None,
        _ => Some(Start::Character),
    }
}

/// Decodes a Trackmania 2020 input stream into discrete input events.
///
/// Events are emitted on change only (the dense per-tick view is derived from
/// them); respawn bits are emitted whenever set, which is what the reference
/// does.
pub fn decode_trackmania_inputs(p: &PlayerInputData) -> InputDecode {
    let mut events = Vec::new();
    let mut warnings = Vec::new();
    let offset = p.start_offset_ms.unwrap_or(0);
    let mut bits = BitReader::new(&p.data);

    let mut started = Start::NotStarted;
    let mut prev_states: Option<u64> = None;
    let mut prev_horn: Option<bool> = None;
    let mut prev_steer: Option<i8> = None;
    let mut prev_accel: Option<bool> = None;
    let mut prev_brake: Option<bool> = None;
    let mut prev_mouse: Option<(u16, u16)> = None;
    let mut prev_strafe: Option<u8> = None;
    let mut prev_walk: Option<u8> = None;
    let mut prev_vertical: Option<u8> = None;
    let mut prev_horizontal: Option<u8> = None;
    let mut prev_gun: Option<bool> = None;
    let mut prev_action: Option<bool> = None;

    let state_bits = if p.version == 11 { 33 } else { 34 };
    let mut stop: Option<DecodeStop> = None;
    let mut truncated = false;
    let mut last_tick = 0u32;

    'ticks: for tick in 0..p.ticks {
        last_tick = tick;
        let time_ms = tick_time_ms(tick, offset);
        let push = |ev: Event, events: &mut Vec<InputEvent>| {
            events.push(InputEvent {
                time_ms,
                tick,
                event: ev,
            });
        };

        let same_state = match bits.read_bit() {
            Some(b) => b,
            None => {
                truncated = true;
                break 'ticks;
            }
        };

        if !same_state {
            let only_2_bit = match bits.read_bit() {
                Some(b) => b,
                None => {
                    truncated = true;
                    break 'ticks;
                }
            };

            if only_2_bit {
                let states = match bits.read_bits(2) {
                    Some(s) => s,
                    None => {
                        truncated = true;
                        break 'ticks;
                    }
                };
                match started {
                    Start::Vehicle => {
                        let horn = states & 2 != 0;
                        if Some(horn) != prev_horn {
                            push(Event::Horn(horn), &mut events);
                            prev_horn = Some(horn);
                        }
                    }
                    Start::Character => {
                        let gun = states & 1 != 0;
                        if Some(gun) != prev_gun {
                            push(Event::GunTrigger(gun), &mut events);
                            prev_gun = Some(gun);
                        }
                        let action = states & 2 != 0;
                        if Some(action) != prev_action {
                            push(Event::Action(action), &mut events);
                            prev_action = Some(action);
                        }
                    }
                    Start::NotStarted => {}
                }
            } else {
                let states = match bits.read_bits(state_bits) {
                    Some(s) => s,
                    None => {
                        truncated = true;
                        break 'ticks;
                    }
                };
                started = match get_start(states) {
                    Some(s) => s,
                    None => {
                        let kind = (states & 0xF) as u8;
                        stop = Some(DecodeStop::UnsupportedKind { kind, tick });
                        break 'ticks;
                    }
                };

                if started == Start::Character {
                    let gun = states & (1 << 5) != 0;
                    if Some(gun) != prev_gun {
                        push(Event::GunTrigger(gun), &mut events);
                        prev_gun = Some(gun);
                    }
                }

                let horn_or_action = states & (1 << 6) != 0;
                match started {
                    Start::Vehicle => {
                        if Some(horn_or_action) != prev_horn {
                            push(Event::Horn(horn_or_action), &mut events);
                            prev_horn = Some(horn_or_action);
                        }
                    }
                    Start::Character => {
                        if Some(horn_or_action) != prev_action {
                            push(Event::Action(horn_or_action), &mut events);
                            prev_action = Some(horn_or_action);
                        }
                        if changed(10, states, prev_states) {
                            push(Event::Camera2(states & (1 << 10) != 0), &mut events);
                        }
                    }
                    Start::NotStarted => {}
                }

                if changed(12, states, prev_states) {
                    push(Event::Jump(states & (1 << 12) != 0), &mut events);
                }
                if changed(13, states, prev_states) {
                    push(Event::FreeLook(states & (1 << 13) != 0), &mut events);
                }
                for j in 0..9 {
                    if changed(14 + j, states, prev_states) {
                        push(
                            Event::ActionSlot(j as u8 + 1, states & (1 << (14 + j)) != 0),
                            &mut events,
                        );
                    }
                }
                if changed(23, states, prev_states) {
                    push(Event::ActionSlot(0, states & (1 << 23) != 0), &mut events);
                }
                if states & (1 << 31) != 0 {
                    push(Event::Respawn, &mut events);
                }
                if state_bits > 33 && states & (1u64 << 33) != 0 {
                    push(Event::SecondaryRespawn, &mut events);
                }

                prev_states = Some(states);
            }
        }

        let same_mouse = match bits.read_bit() {
            Some(b) => b,
            None => {
                truncated = true;
                break 'ticks;
            }
        };
        if !same_mouse {
            let x = bits.read_u16();
            let y = bits.read_u16();
            match (x, y) {
                (Some(x), Some(y)) => {
                    if Some((x, y)) != prev_mouse {
                        push(Event::Mouse(x, y), &mut events);
                        prev_mouse = Some((x, y));
                    }
                }
                _ => {
                    truncated = true;
                    break 'ticks;
                }
            }
        }

        match started {
            Start::Vehicle => {
                let same = match bits.read_bit() {
                    Some(b) => b,
                    None => {
                        truncated = true;
                        break 'ticks;
                    }
                };
                if !same {
                    let (steer, accel, brake) =
                        match (bits.read_sbyte(), bits.read_bit(), bits.read_bit()) {
                            (Some(s), Some(a), Some(b)) => (s, a, b),
                            _ => {
                                truncated = true;
                                break 'ticks;
                            }
                        };
                    if Some(steer) != prev_steer {
                        push(Event::Steer(steer), &mut events);
                        prev_steer = Some(steer);
                    }
                    if Some(accel) != prev_accel {
                        push(Event::Accelerate(accel), &mut events);
                        prev_accel = Some(accel);
                    }
                    if Some(brake) != prev_brake {
                        push(Event::Brake(brake), &mut events);
                        prev_brake = Some(brake);
                    }
                }
            }
            Start::Character => {
                let same = match bits.read_bit() {
                    Some(b) => b,
                    None => {
                        truncated = true;
                        break 'ticks;
                    }
                };
                if !same {
                    let vals = match (
                        bits.read_bits(2),
                        bits.read_bits(2),
                        bits.read_bits(2),
                        bits.read_bits(2),
                    ) {
                        (Some(a), Some(b), Some(c), Some(d)) => {
                            (a as u8, b as u8, c as u8, d as u8)
                        }
                        _ => {
                            truncated = true;
                            break 'ticks;
                        }
                    };
                    // The reference rejects these values as corrupted, so the
                    // rest of the stream cannot be trusted either.
                    if vals.0 == 2 || vals.1 == 2 {
                        let what = if vals.0 == 2 { "strafe" } else { "walk" };
                        stop = Some(DecodeStop::CorruptedValue { what, tick });
                        break 'ticks;
                    }
                    if Some(vals.0) != prev_strafe {
                        push(Event::Strafe(vals.0), &mut events);
                        prev_strafe = Some(vals.0);
                    }
                    if Some(vals.1) != prev_walk {
                        push(Event::Walk(vals.1), &mut events);
                        prev_walk = Some(vals.1);
                    }
                    if Some(vals.2) != prev_vertical {
                        push(Event::Vertical(vals.2), &mut events);
                        prev_vertical = Some(vals.2);
                    }
                    if Some(vals.3) != prev_horizontal {
                        push(Event::Horizontal(vals.3), &mut events);
                        prev_horizontal = Some(vals.3);
                    }
                }
            }
            Start::NotStarted => {}
        }
    }

    if stop.is_none() && truncated {
        stop = Some(DecodeStop::OutOfData { tick: last_tick });
    }
    if stop.is_none() && !bits.rest_is_zero() {
        warnings.push("input buffer has non-zero bits after the last tick".to_string());
    }

    InputDecode {
        events,
        stop,
        warnings,
    }
}

fn changed(bit: u32, states: u64, prev: Option<u64>) -> bool {
    let masked = states & (1u64 << bit);
    match prev {
        None => masked != 0,
        Some(p) => masked != (p & (1u64 << bit)),
    }
}

/// One control entry of a pre-TM2020 replay (TMU/TMNF/TMUF/TM2).
#[derive(Debug, Clone)]
pub struct ControlEntry {
    pub time_ms: i32,
    pub control: String,
    pub onoff: u32,
    pub value: u32,
}

/// Reads the legacy control-entry input list of a ghost validation chunk.
pub fn parse_legacy_inputs(r: &mut Reader<'_>) -> Result<(Vec<ControlEntry>, Vec<String>)> {
    let count = r.u32()?;
    let mut names = Vec::new();
    for _ in 0..count.min(4096) {
        names.push(r.id_string()?);
    }
    let num_inputs = r.i32()?;
    let _count_limit = r.i32()?;
    let mut entries = Vec::new();
    for _ in 0..num_inputs.clamp(0, 4_000_000) {
        let time = r.i32()? - 100000;
        let name_index = r.u8()? as usize;
        let data = r.u32()?;
        entries.push(ControlEntry {
            time_ms: time,
            control: names
                .get(name_index)
                .cloned()
                .unwrap_or_else(|| format!("#{name_index}")),
            onoff: data & 0xFFFF,
            value: data >> 16,
        });
    }
    Ok((entries, names))
}
