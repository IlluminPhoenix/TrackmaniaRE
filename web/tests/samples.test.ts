import { describe, expect, it } from "vitest";

import { readSamples, type SampleTrackReader } from "../src/lib/samples";

/** Sizes of the three fake record tracks: 2 car samples, an empty item, 3 car. */
const lengths = [2, 0, 3];

function count(track: number): number {
  return lengths[track] ?? 0;
}

const reader: SampleTrackReader = {
  track_times_ms: (_ghost, track) =>
    Float64Array.from({ length: count(track) }, (_, i) => track * 100 + i * 50),
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

describe("readSamples", () => {
  it("joins every checkpoint segment in track order", () => {
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

  it("keeps a single-track run as-is", () => {
    const samples = readSamples(reader, 0, [0]);
    expect(samples?.times.length).toBe(2);
    expect(samples?.positions.length).toBe(6);
  });

  it("returns null when no track has samples", () => {
    expect(readSamples(reader, 0, [])).toBeNull();
    expect(readSamples(reader, 0, [1])).toBeNull();
  });
});
