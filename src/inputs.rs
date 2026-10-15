//! Input extraction.
//!
//! Two formats are handled:
//!
//! * Trackmania 2020 stores a bit-packed, tick-based (100 Hz) player input
//!   stream in the ghost chunk `0x0309201D`. The bit layout was reverse
//!   engineered by the GBX.NET community (see
//!   <https://github.com/BigBang1112/gbx-net/wiki/Extracting-input-data-from-ghosts>);
//!   this implementation follows that description.
//! * Older games (TMU / TMNF / TMUF / TM2) store a list of control entries in
//!   the ghost validation chunks (`0x03092011` / `0x03092019` / `0x03092025`).

use crate::error::Result;
use crate::reader::Reader;

/// Least-significant-bit-first bit reader, matching the engine's format.
pub struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        BitReader { data, bit_pos: 0 }
    }

    pub fn bits_left(&self) -> usize {
        self.data.len() * 8 - self.bit_pos
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

    /// True when every remaining bit is zero (the buffer is expected to be
    /// zero-padded to a byte boundary).
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
pub fn parse_player_input_data(
    r: &mut Reader<'_>,
    chunk_version: u32,
) -> Result<Vec<PlayerInputData>> {
    let count = r.u32()?;
    let mut out = Vec::new();
    for _ in 0..count.min(16) {
        let version = r.u32()?;
        let _u01 = r.u32()?;
        let start_offset_ms = if chunk_version >= 4 {
            let v = r.i32()?;
            if v == -1 { None } else { Some(v) }
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
        if out.len() == 1 && count > 1 {
            // Some files store the array without a leading count; keep going.
        }
    }
    Ok(out)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Start {
    NotStarted,
    Character,
    Vehicle,
}

fn get_start(states: u64) -> Start {
    match states & 0xF {
        0 => Start::NotStarted,
        2..=4 => Start::Vehicle,
        _ => Start::Character,
    }
}

/// Decodes a Trackmania 2020 input stream into discrete input events.
///
/// Returns the events plus a flag telling whether the stream ended early
/// (corrupted input, which happens on runs recorded before 2020-07-20).
pub fn decode_trackmania_inputs(
    p: &PlayerInputData,
    include_unchanged_states: bool,
) -> (Vec<InputEvent>, bool) {
    let mut events = Vec::new();
    let offset = p.start_offset_ms.unwrap_or(0);
    let mut bits = BitReader::new(&p.data);

    let mut started = Start::NotStarted;
    let mut prev_states: Option<u64> = None;
    let mut prev_horn: Option<bool> = None;
    let mut prev_steer: Option<i8> = None;
    let mut prev_accel: Option<bool> = None;
    let mut prev_brake: Option<bool> = None;
    let mut prev_slots: [Option<bool>; 10] = [None; 10];
    let mut prev_mouse: Option<(u16, u16)> = None;
    let mut prev_strafe: Option<u8> = None;
    let mut prev_walk: Option<u8> = None;
    let mut prev_vertical: Option<u8> = None;
    let mut prev_horizontal: Option<u8> = None;
    let mut prev_gun: Option<bool> = None;
    let mut prev_action: Option<bool> = None;

    let state_bits = if p.version == 11 { 33 } else { 34 };
    let mut truncated = false;

    'ticks: for tick in 0..p.ticks {
        let time_ms = (tick as i32) * 10 + offset;
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
                        if Some(horn) != prev_horn || include_unchanged_states {
                            push(Event::Horn(horn), &mut events);
                            prev_horn = Some(horn);
                        }
                    }
                    Start::Character => {
                        let gun = states & 1 != 0;
                        if Some(gun) != prev_gun || include_unchanged_states {
                            push(Event::GunTrigger(gun), &mut events);
                            prev_gun = Some(gun);
                        }
                        let action = states & 2 != 0;
                        if Some(action) != prev_action || include_unchanged_states {
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
                started = get_start(states);

                if started == Start::Character {
                    let gun = states & (1 << 5) != 0;
                    if Some(gun) != prev_gun || include_unchanged_states {
                        push(Event::GunTrigger(gun), &mut events);
                        prev_gun = Some(gun);
                    }
                }

                let horn_or_action = states & (1 << 6) != 0;
                match started {
                    Start::Vehicle => {
                        if Some(horn_or_action) != prev_horn || include_unchanged_states {
                            push(Event::Horn(horn_or_action), &mut events);
                            prev_horn = Some(horn_or_action);
                        }
                    }
                    Start::Character => {
                        if Some(horn_or_action) != prev_action || include_unchanged_states {
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

                for i in 0..10 {
                    prev_slots[i] = Some(states & (1 << (if i < 9 { 14 + i } else { 23 })) != 0);
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
                    if Some((x, y)) != prev_mouse || include_unchanged_states {
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
                    if Some(steer) != prev_steer || include_unchanged_states {
                        push(Event::Steer(steer), &mut events);
                        prev_steer = Some(steer);
                    }
                    if Some(accel) != prev_accel || include_unchanged_states {
                        push(Event::Accelerate(accel), &mut events);
                        prev_accel = Some(accel);
                    }
                    if Some(brake) != prev_brake || include_unchanged_states {
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

    let _ = bits.rest_is_zero();
    (events, truncated)
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
    for _ in 0..num_inputs.max(0).min(4_000_000) {
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
