//! Reads vehicle timelines from the wasm shim and joins the checkpoint
//! segments of one run into a single series.
//!
//! TM2020 starts a new car entity list at every checkpoint, so a completed run
//! normally has several `CSceneVehicleVis` tracks. They are consecutive
//! segments of the same run, in time order; taking only the first one truncates
//! the path (and every chart built from it) at the first checkpoint.

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
 * Reads every car track of `ghost` and joins their timelines in the order the
 * caller passes them (which is record-list order, i.e. time order). Tracks
 * with no decoded samples are ignored. Returns null when none has samples.
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
  if (parts.length === 1) return parts[0];
  return {
    times: concat(parts.map((part) => part.times)),
    positions: concat(parts.map((part) => part.positions)),
    velocities: concat(parts.map((part) => part.velocities)),
    rotations: concat(parts.map((part) => part.rotations)),
    speeds: concat(parts.map((part) => part.speeds)),
    sideSpeeds: concat(parts.map((part) => part.sideSpeeds)),
    steer: concat(parts.map((part) => part.steer)),
    gas: concat(parts.map((part) => part.gas)),
    brake: concat(parts.map((part) => part.brake)),
    rpm: concat(parts.map((part) => part.rpm)),
    gear: concat(parts.map((part) => part.gear)),
    turbo: concat(parts.map((part) => part.turbo)),
    isTurbo: concat(parts.map((part) => part.isTurbo)),
    vehicleState: concat(parts.map((part) => part.vehicleState)),
    groundMode: concat(parts.map((part) => part.groundMode)),
    groundMaterial: concat(parts.map((part) => part.groundMaterial)),
  };
}
