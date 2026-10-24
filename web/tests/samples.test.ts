import { describe, expect, it } from "vitest";

import { readSamples, type SampleTrackReader } from "../src/lib/samples";

/** Fake record tracks: each array is one track's sample times. */
function readerFor(trackTimes: readonly (readonly number[])[]): SampleTrackReader {
  const count = (track: number) => trackTimes[track]?.length ?? 0;
  return {
    track_times_ms: (_ghost, track) => Float64Array.from(trackTimes[track] ?? []),
    track_positions: (_ghost, track) =>
      Float32Array.from({ length: count(track) * 3 }, (_, i) => track * 100 + i),
    track_velocities: (_ghost, track) => new Float32Array(count(track) * 3),
    track_rotations: (_ghost, track) => new Float32Array(count(track) * 3),
    track_speeds_mps: (_ghost, track) => new Float32Array(count(track)),
    track_side_speeds: (_ghost, track) => new Float32Array(count(track)),
    track_steer: (_ghost, track) => new Float32Array(count(track)),
    track_gas: (_ghost, track) => new Float32Array(count(track)),
    track_brake: (_ghost, track) => new Float32Array(count(track)),
    track_rpms: (_ghost, track) => new Uint8Array(count(track)),
    track_gears: (_ghost, track) => new Float32Array(count(track)),
    track_turbo_times: (_ghost, track) => new Uint8Array(count(track)),
    track_is_turbo: (_ghost, track) => new Uint8Array(count(track)),
    track_vehicle_states: (_ghost, track) => new Uint8Array(count(track)),
    track_ground_modes: (_ghost, track) => new Uint8Array(count(track)),
    track_ground_materials: (_ghost, track) => new Uint8Array(count(track) * 4),
  };
}

describe("readSamples", () => {
  it("joins every checkpoint segment in time order", () => {
    const reader = readerFor([[0, 50], [], [200, 250, 300]]);
    const samples = readSamples(reader, 0, [0, 1, 2]);
    expect(samples).not.toBeNull();
    if (!samples) return;

    expect(Array.from(samples.times)).toEqual([0, 50, 200, 250, 300]);
    expect(samples.positions.length).toBe(5 * 3);
    expect(Array.from(samples.positions.slice(0, 8))).toEqual([0, 1, 2, 3, 4, 5, 200, 201]);
    expect(samples.speeds.length).toBe(5);
    expect(samples.rpm.length).toBe(5);
    expect(samples.groundMaterial.length).toBe(5 * 4);
  });

  it("sorts segments by time even when the record lists them out of order", () => {
    const reader = readerFor([[200, 250, 300], [0, 50]]);
    const samples = readSamples(reader, 0, [0, 1]);
    expect(samples?.times[0]).toBe(0);
    expect(Array.from(samples?.times ?? [])).toEqual([0, 50, 200, 250, 300]);
  });

  it("drops a sparse overlapping car track from another recording", () => {
    // Trackmania.io downloads: a 7-sample track next to the full 20 Hz
    // recording. The sparse track starts a little later and ends early, so a
    // plain concatenation would jump backwards and forward in time.
    const sparse = Array.from({ length: 7 }, (_, i) => 40 + i * 3046);
    const full = Array.from({ length: 403 }, (_, i) => 20 + i * 50);
    const samples = readSamples(readerFor([sparse, full]), 0, [0, 1]);
    expect(samples).not.toBeNull();
    if (!samples) return;

    expect(samples.times.length).toBe(403);
    expect(Array.from(samples.times.slice(0, 3))).toEqual([20, 70, 120]);
    expect(samples.times[samples.times.length - 1]).toBe(20120);
    // Track 1's positions start at 100, track 0's at 0.
    expect(Array.from(samples.positions.slice(0, 3))).toEqual([100, 101, 102]);
  });

  it("keeps a single-track run as-is", () => {
    const samples = readSamples(readerFor([[0, 50]]), 0, [0]);
    expect(samples?.times.length).toBe(2);
    expect(samples?.positions.length).toBe(6);
  });

  it("returns null when no track has samples", () => {
    expect(readSamples(readerFor([]), 0, [])).toBeNull();
    expect(readSamples(readerFor([[]]), 0, [0])).toBeNull();
  });
});
