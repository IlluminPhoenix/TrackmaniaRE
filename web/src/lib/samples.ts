//! Reads vehicle timelines from the wasm shim and joins the checkpoint
//! segments of one run into a single series.
//!
//! TM2020 starts a new car entity list at every checkpoint, so a completed run
//! normally has several `CSceneVehicleVis` tracks. They are consecutive
//! segments of the same run and must be joined in time order; taking only the
//! first one truncates the path (and every chart built from it) at the first
//! checkpoint.
//!
//! Some exports also carry a second, sparse car track that covers the same
//! part of the run. Trackmania.io downloads do this: the downloaded ghost has
//! a 7-sample track next to the full 20 Hz recording. Overlapping tracks are
//! therefore alternatives, not checkpoint segments, and the one with more
//! samples wins.

import type { Samples } from "./types.ts";

/**
 * The wasm `Replay` accessors used here. Kept structural so tests can supply
 * arrays without instantiating the parser.
 */
export interface SampleTrackReader {
  track_times_ms(ghost: number, track: number): Float64Array;
  track_positions(ghost: number, track: number): Float32Array;
  track_velocities(ghost: number, track: number): Float32Array;
  track_rotations(ghost: number, track: number): Float32Array;
  track_speeds_mps(ghost: number, track: number): Float32Array;
  track_side_speeds(ghost: number, track: number): Float32Array;
  track_steer(ghost: number, track: number): Float32Array;
  track_gas(ghost: number, track: number): Float32Array;
  track_brake(ghost: number, track: number): Float32Array;
  track_rpms(ghost: number, track: number): Uint8Array;
  track_gears(ghost: number, track: number): Float32Array;
  track_turbo_times(ghost: number, track: number): Uint8Array;
  track_is_turbo(ghost: number, track: number): Uint8Array;
  track_vehicle_states(ghost: number, track: number): Uint8Array;
  track_ground_modes(ghost: number, track: number): Uint8Array;
  track_ground_materials(ghost: number, track: number): Uint8Array;
}

type TypedArray = Float32Array | Float64Array | Uint8Array;

/** Concatenates typed arrays of the same type. */
function concat<T extends TypedArray>(parts: readonly T[]): T {
  if (parts.length === 1) return parts[0];
  let length = 0;
  for (const part of parts) length += part.length;
  const Constructor = parts[0].constructor as new (length: number) => T;
  const out = new Constructor(length);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function readTrack(reader: SampleTrackReader, ghost: number, track: number): Samples {
  return {
    times: reader.track_times_ms(ghost, track),
    positions: reader.track_positions(ghost, track),
    velocities: reader.track_velocities(ghost, track),
    rotations: reader.track_rotations(ghost, track),
    speeds: reader.track_speeds_mps(ghost, track),
    sideSpeeds: reader.track_side_speeds(ghost, track),
    steer: reader.track_steer(ghost, track),
    gas: reader.track_gas(ghost, track),
    brake: reader.track_brake(ghost, track),
    rpm: reader.track_rpms(ghost, track),
    gear: reader.track_gears(ghost, track),
    turbo: reader.track_turbo_times(ghost, track),
    isTurbo: reader.track_is_turbo(ghost, track),
    vehicleState: reader.track_vehicle_states(ghost, track),
    groundMode: reader.track_ground_modes(ghost, track),
    groundMaterial: reader.track_ground_materials(ghost, track),
  };
}

/**
 * Overlap tolerated between consecutive checkpoint segments. A new segment
 * starts at the checkpoint, usually a whole sample interval after the previous
 * one ended; anything that overlaps by more than a few samples is a second
 * recording, not a continuation.
 */
const MAX_SEGMENT_OVERLAP_MS = 250;

/**
 * Picks the tracks that belong to one run. Checkpoint segments are
 * non-overlapping, so each becomes its own group. Tracks whose time ranges
 * overlap belong to the same group, and only the track with the most samples
 * survives (a sparse recording loses against the full one). Groups are
 * returned in time order.
 */
function chooseSegments(parts: readonly Samples[]): Samples[] {
  const ordered = [...parts].sort(
    (a, b) => a.times[0] - b.times[0] || b.times.length - a.times.length,
  );
  const chosen: Samples[] = [];
  let group: Samples[] = [];
  let groupEnd = Number.NEGATIVE_INFINITY;

  const flush = () => {
    if (group.length === 0) return;
    const best = group.reduce((a, b) => (b.times.length > a.times.length ? b : a));
    chosen.push(best);
    group = [];
  };

  for (const part of ordered) {
    const start = part.times[0];
    const end = part.times[part.times.length - 1];
    if (group.length > 0 && start >= groupEnd - MAX_SEGMENT_OVERLAP_MS) {
      flush();
      groupEnd = Number.NEGATIVE_INFINITY;
    }
    group.push(part);
    groupEnd = Math.max(groupEnd, end);
  }
  flush();
  return chosen;
}

/**
 * Reads every car track of `ghost`, drops alternative recordings that overlap
 * the checkpoint segments, and joins the remaining timelines in time order.
 * Tracks with no decoded samples are ignored. Returns null when none has
 * samples.
 */
export function readSamples(
  reader: SampleTrackReader,
  ghost: number,
  tracks: readonly number[],
): Samples | null {
  const parts = tracks
    .map((track) => readTrack(reader, ghost, track))
    .filter((part) => part.times.length > 0);
  if (parts.length === 0) return null;
  const segments = chooseSegments(parts);
  if (segments.length === 1) return segments[0];
  return {
    times: concat(segments.map((part) => part.times)),
    positions: concat(segments.map((part) => part.positions)),
    velocities: concat(segments.map((part) => part.velocities)),
    rotations: concat(segments.map((part) => part.rotations)),
    speeds: concat(segments.map((part) => part.speeds)),
    sideSpeeds: concat(segments.map((part) => part.sideSpeeds)),
    steer: concat(segments.map((part) => part.steer)),
    gas: concat(segments.map((part) => part.gas)),
    brake: concat(segments.map((part) => part.brake)),
    rpm: concat(segments.map((part) => part.rpm)),
    gear: concat(segments.map((part) => part.gear)),
    turbo: concat(segments.map((part) => part.turbo)),
    isTurbo: concat(segments.map((part) => part.isTurbo)),
    vehicleState: concat(segments.map((part) => part.vehicleState)),
    groundMode: concat(segments.map((part) => part.groundMode)),
    groundMaterial: concat(segments.map((part) => part.groundMaterial)),
  };
}
