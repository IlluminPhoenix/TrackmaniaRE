//! WebAssembly bindings for the `trackmaniare` replay extractor.
//!
//! The core crate is I/O-free and parses from a byte slice, so the entire
//! extractor runs in the browser. This shim only adapts the result for
//! JavaScript. Timelines are exposed as typed arrays rather than JSON: a 2 h
//! replay serialises to ~161 MB of timeline JSON, while the same data as
//! `Float32Array`s is a few MB with zero parse cost.
//!
//! All arrays are snapshots; call the method again after a re-parse. The whole
//! extraction lives in WASM memory until the returned `Replay` is freed (or
//! garbage-collected by JS, which calls `free()` for you).

use wasm_bindgen::prelude::*;

use trackmaniare::inputs::{Event, InputEvent, tick_time_ms};
use trackmaniare::model::{Extraction, GhostInputs};
use trackmaniare::record::{
    CLASS_CSCENE_VEHICLE_VIS, EntListElem, RecordData, Sample, VehicleSample,
};
use trackmaniare::{ExtractOptions, parse_replay_with, tabular};

/// Installs a panic hook that logs a real message to the browser console
/// instead of the bare `unreachable executed` trap.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// A parsed `.Replay.Gbx` / `.Ghost.Gbx`. The embedded map is always kept so
/// the frontend can offer it for download.
#[wasm_bindgen]
pub struct Replay {
    ex: Extraction,
}

#[wasm_bindgen]
impl Replay {
    /// Parses a replay from its raw bytes; throws on container-level errors.
    /// `file_name` is echoed in the metadata JSON.
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8], file_name: &str) -> Result<Replay, JsError> {
        parse_replay_with(
            bytes,
            file_name,
            ExtractOptions {
                keep_embedded_map: true,
                keep_body: false,
            },
        )
        .map(|ex| Replay { ex })
        .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Everything except the timelines: replay/ghost metadata, chunk
    /// inventory, warnings, sample counts. Small even for large replays.
    pub fn metadata_json(&self) -> String {
        self.ex.to_json(false).to_pretty()
    }

    pub fn ghost_count(&self) -> usize {
        self.ex.ghosts.len()
    }

    /// The map embedded in the replay, as raw `.Challenge.Gbx` bytes; empty
    /// when the file has none.
    pub fn map_bytes(&self) -> Vec<u8> {
        self.ex.embedded_map.clone().unwrap_or_default()
    }

    /// The map name from the replay's community XML, for download names.
    pub fn map_name(&self) -> Option<String> {
        self.ex
            .replay
            .xml_attributes
            .iter()
            .find(|(k, _)| k == "name")
            .map(|(_, v)| v.clone())
    }

    // ---- vehicle samples (20 Hz record data) --------------------------------

    /// Number of entity tracks in ghost `ghost`'s record data (the car, plus
    /// items and objects).
    pub fn track_count(&self, ghost: usize) -> usize {
        self.record(ghost).map(|r| r.lists.len()).unwrap_or(0)
    }

    /// Index of the car track, if the ghost has one.
    pub fn vehicle_track_index(&self, ghost: usize) -> Option<usize> {
        let record = self.record(ghost)?;
        record
            .lists
            .iter()
            .position(|l| l.class_id == Some(CLASS_CSCENE_VEHICLE_VIS))
    }

    /// Class id of track `track`; the car is `0x0A018000`.
    pub fn track_class_id(&self, ghost: usize, track: usize) -> u32 {
        self.track(ghost, track)
            .and_then(|l| l.class_id)
            .unwrap_or(0)
    }

    /// Number of decoded vehicle states in `track`. Samples whose payload did
    /// not decode are omitted, so every array below has exactly this length.
    pub fn track_sample_count(&self, ghost: usize, track: usize) -> usize {
        self.track(ghost, track)
            .map(|l| l.samples.iter().filter(|s| s.vehicle.is_some()).count())
            .unwrap_or(0)
    }

    fn each_vehicle<F: FnMut(&Sample, &VehicleSample)>(
        &self,
        ghost: usize,
        track: usize,
        mut f: F,
    ) {
        if let Some(list) = self.track(ghost, track) {
            for s in &list.samples {
                if let Some(v) = &s.vehicle {
                    f(s, v);
                }
            }
        }
    }

    /// Sample timestamps in milliseconds from the start of the run.
    pub fn track_times_ms(&self, ghost: usize, track: usize) -> Vec<f64> {
        let mut out = Vec::new();
        self.each_vehicle(ghost, track, |s, _| out.push(s.time_ms as f64));
        out
    }

    /// Sample positions as x, y, z triples in metres.
    pub fn track_positions(&self, ghost: usize, track: usize) -> Vec<f32> {
        let mut out = Vec::new();
        self.each_vehicle(ghost, track, |_, v| out.extend_from_slice(&v.position));
        out
    }

    /// Sample rotations as angle, axis heading, axis pitch triples in radians.
    pub fn track_rotations(&self, ghost: usize, track: usize) -> Vec<f32> {
        let mut out = Vec::new();
        self.each_vehicle(ghost, track, |_, v| {
            out.extend_from_slice(&[
                v.rotation_angle,
                v.rotation_axis_heading,
                v.rotation_axis_pitch,
            ])
        });
        out
    }

    /// Forward speed in metres per second.
    pub fn track_speeds_mps(&self, ghost: usize, track: usize) -> Vec<f32> {
        let mut out = Vec::new();
        self.each_vehicle(ghost, track, |_, v| out.push(v.speed));
        out
    }

    /// Recorded wheel steering, -1 .. 1.
    pub fn track_steer(&self, ghost: usize, track: usize) -> Vec<f32> {
        let mut out = Vec::new();
        self.each_vehicle(ghost, track, |_, v| out.push(v.steer));
        out
    }

    /// Throttle input of the recorded sample, 0 .. 1.
    pub fn track_gas(&self, ghost: usize, track: usize) -> Vec<f32> {
        let mut out = Vec::new();
        self.each_vehicle(ghost, track, |_, v| out.push(v.gas));
        out
    }

    /// Brake input of the recorded sample, 0 .. 1.
    pub fn track_brake(&self, ghost: usize, track: usize) -> Vec<f32> {
        let mut out = Vec::new();
        self.each_vehicle(ghost, track, |_, v| out.push(v.brake));
        out
    }

    // ---- inputs (TM2020 bit-packed, 100 Hz) ---------------------------------

    /// Number of 10 ms ticks in ghost `ghost`'s input stream — the last decoded
    /// tick when the stream ended early, 0 for legacy control-entry inputs.
    pub fn input_ticks(&self, ghost: usize) -> u32 {
        match self.ex.ghosts.get(ghost).and_then(|g| g.inputs.as_ref()) {
            Some(GhostInputs::Trackmania2020 {
                ticks,
                events,
                truncated,
                ..
            }) => effective_ticks(*ticks, *truncated, events),
            _ => 0,
        }
    }

    /// The sparse input events (steer changes, pedals, horn, actions, respawns,
    /// mouse) as JSON, including their tick and time. This is the event list a
    /// TAS input file is built from.
    pub fn inputs_json(&self, ghost: usize) -> String {
        match self.ex.ghosts.get(ghost).and_then(|g| g.inputs.as_ref()) {
            Some(inputs) => inputs.to_json(true).to_pretty(),
            None => "null".to_string(),
        }
    }

    /// Per-tick steering byte, -127 .. 127, one entry per tick
    /// (`input_ticks()` long). `0` where no steering is in effect.
    pub fn input_steer(&self, ghost: usize) -> Vec<i8> {
        self.dense_inputs(ghost)
            .map(|(steer, _)| steer)
            .unwrap_or_default()
    }

    /// Per-tick input flags, one entry per tick: bit 0 accelerate, bit 1
    /// brake, bit 2 horn. Respawns are one-way events and stay in
    /// `inputs_json`.
    pub fn input_flags(&self, ghost: usize) -> Vec<u8> {
        self.dense_inputs(ghost)
            .map(|(_, flags)| flags)
            .unwrap_or_default()
    }

    // ---- text exports (same files the CLI writes) ---------------------------

    pub fn inputs_csv(&self) -> String {
        tabular::inputs_csv(&self.ex)
    }

    pub fn inputs_dense_csv(&self) -> String {
        tabular::inputs_csv_dense(&self.ex)
    }

    pub fn samples_csv(&self) -> String {
        tabular::samples_csv(&self.ex)
    }

    // ---- internals ----------------------------------------------------------

    fn track(&self, ghost: usize, track: usize) -> Option<&EntListElem> {
        self.record(ghost)?.lists.get(track)
    }

    fn record(&self, ghost: usize) -> Option<&RecordData> {
        self.ex.ghosts.get(ghost)?.record_data.as_ref()
    }

    /// Dense per-tick view, derived from the sparse events exactly the way the
    /// CLI's dense CSV is (`crates/trackmaniare/src/tabular.rs`): the state in
    /// effect at each tick, emitted only up to the last decoded tick when the
    /// stream was truncated.
    fn dense_inputs(&self, ghost: usize) -> Option<(Vec<i8>, Vec<u8>)> {
        let Some(GhostInputs::Trackmania2020 {
            ticks,
            events,
            truncated,
            start_offset_ms,
            ..
        }) = self.ex.ghosts.get(ghost)?.inputs.as_ref()
        else {
            return None;
        };
        let offset = start_offset_ms.unwrap_or(0);
        let ticks = effective_ticks(*ticks, *truncated, events);

        let mut steer = vec![0i8; ticks as usize];
        let mut flags = vec![0u8; ticks as usize];
        let mut tick_steer = 0i8;
        let mut tick_flags = 0u8;
        let mut i = 0usize;
        for tick in 0..ticks {
            let time = tick_time_ms(tick, offset);
            while i < events.len() && events[i].time_ms <= time {
                match &events[i].event {
                    Event::Steer(v) => tick_steer = *v,
                    Event::Accelerate(v) => set_flag(&mut tick_flags, 1, *v),
                    Event::Brake(v) => set_flag(&mut tick_flags, 2, *v),
                    Event::Horn(v) => set_flag(&mut tick_flags, 4, *v),
                    _ => {}
                }
                i += 1;
            }
            steer[tick as usize] = tick_steer;
            flags[tick as usize] = tick_flags;
        }
        Some((steer, flags))
    }
}

fn set_flag(flags: &mut u8, bit: u8, on: bool) {
    if on {
        *flags |= bit;
    } else {
        *flags &= !bit;
    }
}

/// The tick count to report: the declared one, or the last decoded tick when
/// the stream ended early (the trailing declared ticks carry no data).
fn effective_ticks(ticks: u32, truncated: bool, events: &[InputEvent]) -> u32 {
    if truncated {
        events.last().map(|e| e.tick.saturating_add(1)).unwrap_or(0)
    } else {
        ticks
    }
}
