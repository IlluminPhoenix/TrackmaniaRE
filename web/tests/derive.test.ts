import { describe, expect, it } from "vitest";

import {
  AIR,
  airborneTime,
  buildDeltaSeries,
  buildRoll,
  buildSpeedSeries,
  carForward,
  cumulativeDistance,
  deriveRun,
  detectWallHits,
  GRIP,
  gripStates,
  interpolateAt,
  lastCheckpointIsFinish,
  rollAt,
  sampleIndexAt,
  samplesAt,
  SLIDE,
  slipAngleDeg,
  splitsFor,
  timeDeltaAtDistance,
} from "../src/lib/derive";
import type { Inputs, RunData, Samples } from "../src/lib/types";

function samples(partial: Partial<Samples> & { times: Float64Array }): Samples {
  const n = partial.times.length;
  return {
    times: partial.times,
    positions: partial.positions ?? new Float32Array(n * 3),
    velocities: partial.velocities ?? new Float32Array(n * 3),
    rotations: partial.rotations ?? new Float32Array(n * 3),
    speeds: partial.speeds ?? new Float32Array(n),
    sideSpeeds: partial.sideSpeeds ?? new Float32Array(n),
    steer: partial.steer ?? new Float32Array(n),
    gas: partial.gas ?? new Float32Array(n),
    brake: partial.brake ?? new Float32Array(n),
    rpm: partial.rpm ?? new Uint8Array(n),
    gear: partial.gear ?? new Float32Array(n),
    turbo: partial.turbo ?? new Uint8Array(n),
    isTurbo: partial.isTurbo ?? new Uint8Array(n),
    vehicleState: partial.vehicleState ?? new Uint8Array(n),
    groundMode: partial.groundMode ?? new Uint8Array(n),
    groundMaterial: partial.groundMaterial ?? new Uint8Array(n * 4),
  };
}

function inputs(events: { timeMs: number; type: string }[]): Inputs {
  return {
    format: "tm2020-bitpacked",
    version: 2,
    startOffsetMs: 0,
    truncated: false,
    ticks: 0,
    steer: new Int8Array(0),
    flags: new Uint8Array(0),
    events: events.map((event, index) => ({ ...event, tick: index })),
  };
}

/** A decoded 100 Hz stream whose ticks are `startOffsetMs + i * 10`. */
function inputStream(startOffsetMs: number, steer: number[], flags: number[]): Inputs {
  return {
    format: "tm2020-bitpacked",
    version: 4,
    startOffsetMs,
    truncated: false,
    ticks: steer.length,
    steer: Int8Array.from(steer),
    flags: Uint8Array.from(flags),
    events: [],
  };
}

function run(partial: Partial<RunData>): RunData {
  return {
    id: "run",
    fileName: "run.Replay.Gbx",
    fileSize: 1,
    ghostIndex: 0,
    isReplayRecord: false,
    nickname: "driver",
    login: null,
    raceTimeMs: null,
    respawns: null,
    checkpoints: [],
    warnings: [],
    mapUid: null,
    mapName: null,
    mapEnvironment: null,
    mapAuthor: null,
    samples: null,
    sampleCount: 0,
    sampleStartMs: null,
    sampleEndMs: null,
    inputs: null,
    map: null,
    tas: null,
    tasDropped: 0,
    color: "#fff",
    durationMs: 0,
    derived: null,
    ...partial,
  };
}

/** Positions along +x every 10 m starting at the origin. */
function straightPositions(steps: number): Float32Array {
  const out = new Float32Array(steps * 3);
  for (let i = 0; i < steps; i++) out[3 * i] = i * 10;
  return out;
}

describe("cumulativeDistance", () => {
  it("sums segment lengths", () => {
    const times = new Float64Array([0, 1000, 2000]);
    const positions = new Float32Array([0, 0, 0, 3, 4, 0, 7, 4, 0]);
    const distance = cumulativeDistance(times, positions);
    expect(Array.from(distance)).toEqual([0, 5, 9]);
  });

  it("ignores teleports faster than the segment limit", () => {
    const times = new Float64Array([0, 1000, 2000]);
    const positions = new Float32Array([0, 0, 0, 1000, 0, 0, 1010, 0, 0]);
    const distance = cumulativeDistance(times, positions);
    expect(Array.from(distance)).toEqual([0, 0, 10]);
  });

  it("keeps distance flat when timestamps repeat", () => {
    const times = new Float64Array([0, 0, 50]);
    const positions = new Float32Array([0, 0, 0, 1, 0, 0, 2, 0, 0]);
    const distance = cumulativeDistance(times, positions);
    expect(Array.from(distance)).toEqual([0, 0, 1]);
  });
});

describe("detectWallHits", () => {
  it("finds a hard sideways speed loss", () => {
    const times = new Float64Array([0, 50, 100]);
    const velocities = new Float32Array([20, 0, 0, 6, 0, 0, 6, 0, 0]);
    const hits = detectWallHits(times, velocities);
    expect(hits).toHaveLength(1);
    expect(hits[0].index).toBe(1);
    expect(hits[0].timeMs).toBe(50);
  });

  it("ignores gentle braking in a straight line", () => {
    const times = new Float64Array([0, 50, 100]);
    const velocities = new Float32Array([20, 0, 0, 14.5, 0, 0, 14, 0, 0]);
    expect(detectWallHits(times, velocities)).toHaveLength(0);
  });

  it("ignores a direction change that keeps its speed", () => {
    const times = new Float64Array([0, 50]);
    const velocities = new Float32Array([20, 0, 0, 0, 0, 19]);
    expect(detectWallHits(times, velocities)).toHaveLength(0);
  });

  it("merges impacts within 250 ms and keeps the hardest", () => {
    const times = new Float64Array([0, 50, 100, 150]);
    const velocities = new Float32Array([
      20, 0, 0, // 20 m/s
      6, 0, 0, // drop 14 -> hit
      6, 0, 0, // no drop
      0, 0, 0, // drop 6, cos 1 -> filtered out
    ]);
    const hits = detectWallHits(times, velocities);
    expect(hits).toHaveLength(1);
    expect(hits[0].index).toBe(1);
    expect(hits[0].drop).toBeCloseTo(14, 5);
  });

  it("treats a respawn-speed teleport as noise", () => {
    const times = new Float64Array([0, 50]);
    const velocities = new Float32Array([60, 0, 0, 0, 0, 0]);
    // 60 m/s lost in 50 ms is 1200 m/s^2: a teleport, not a wall.
    expect(detectWallHits(times, velocities)).toHaveLength(0);
  });
});

describe("airborneTime", () => {
  it("counts samples flagged as airborne", () => {
    const data = samples({
      times: new Float64Array([0, 50, 100, 150, 200]),
      groundMode: new Uint8Array([3, 0, 0, 1, 3]),
    });
    const { flags, totalMs } = airborneTime(data);
    expect(Array.from(flags)).toEqual([0, 1, 1, 0, 0]);
    expect(totalMs).toBe(100);
  });
});

describe("grip", () => {
  it("rotates the local forward axis the way the 3D view does", () => {
    // Half a right angle stored means a right angle of rotation about the
    // heading/pitch axis: +Z turned 90 degrees about +X points to -Y.
    const out = new Float32Array(3);
    carForward(new Float32Array([Math.PI / 4, 0, 0]), 0, out);
    expect(out[0]).toBeCloseTo(0, 6);
    expect(out[1]).toBeCloseTo(-1, 6);
    expect(out[2]).toBeCloseTo(0, 6);
    // No rotation is the world +Z (zero heading, zero pitch).
    carForward(new Float32Array([0, 0, 0]), 0, out);
    expect(Array.from(out)).toEqual([0, 0, 1]);
  });

  it("measures the angle between heading and velocity", () => {
    const straight = samples({
      times: new Float64Array([0]),
      velocities: new Float32Array([0, 0, 20]),
      speeds: new Float32Array([20]),
    });
    expect(slipAngleDeg(straight, 0)).toBeCloseTo(0, 3);

    const sideways = samples({
      times: new Float64Array([0]),
      velocities: new Float32Array([20, 0, 0]),
      speeds: new Float32Array([20]),
    });
    expect(slipAngleDeg(sideways, 0)).toBeCloseTo(90, 3);

    const reverse = samples({
      times: new Float64Array([0]),
      velocities: new Float32Array([0, 0, -20]),
      speeds: new Float32Array([20]),
    });
    expect(slipAngleDeg(reverse, 0)).toBeCloseTo(180, 3);
  });

  it("classifies grip, sliding, slow and airborne samples", () => {
    const data = samples({
      times: new Float64Array([0, 50, 100, 150, 200, 250]),
      // Forward is +Z for every sample.
      groundMode: new Uint8Array([3, 3, 3, 3, 0, 0]),
      speeds: new Float32Array([10, 10, 10, 1.5, 10, 10]),
      velocities: new Float32Array([
        0, 0, 10, // straight
        0, 0, 10, // straight
        10, 0, 0, // 90 degrees sideways
        1.5, 0, 0, // sideways but too slow to tell
        0, 0, 10, // airborne
        0, 0, 10, // airborne
      ]),
    });
    const grip = gripStates(data);
    expect(Array.from(grip.states)).toEqual([GRIP, GRIP, SLIDE, GRIP, AIR, AIR]);
    expect(grip.gripMs).toBe(150);
    expect(grip.slideMs).toBe(50);
    expect(grip.airMs).toBe(100);
    expect(grip.maxSlipDeg).toBeCloseTo(90, 1);
  });

  it("reports the totals on the derived block", () => {
    const derived = deriveRun(
      run({
        samples: samples({
          times: new Float64Array([0, 50, 100]),
          groundMode: new Uint8Array([3, 3, 3]),
          speeds: new Float32Array([10, 10, 10]),
          velocities: new Float32Array([0, 0, 10, 10, 0, 0, 0, 0, 10]),
        }),
      }),
    );
    expect(derived?.slideMs).toBe(50);
    expect(derived?.gripStates).toHaveLength(3);
    expect(Array.from(derived!.gripStates)).toEqual([GRIP, SLIDE, GRIP]);
    expect(derived?.maxSlipDeg).toBeCloseTo(90, 1);
  });
});

describe("sample lookup", () => {
  const times = new Float64Array([0, 100, 200]);

  it("finds the last sample at or before a time", () => {
    expect(sampleIndexAt(times, -1)).toBe(-1);
    expect(sampleIndexAt(times, 0)).toBe(0);
    expect(sampleIndexAt(times, 99)).toBe(0);
    expect(sampleIndexAt(times, 100)).toBe(1);
    expect(sampleIndexAt(times, 1_000_000)).toBe(2);
  });

  it("interpolates linearly and refuses before the start", () => {
    expect(interpolateAt(times, new Float32Array([10, 20, 40]), -5)).toBeNull();
    expect(interpolateAt(times, new Float32Array([10, 20, 40]), 50)).toBe(15);
    expect(interpolateAt(times, new Float32Array([10, 20, 40]), 200)).toBe(40);
  });
});

describe("buildRoll", () => {
  it("expands the input stream and applies the chunk offset", () => {
    const roll = buildRoll(run({ inputs: inputStream(-1500, [0, -99, 40], [1, 3, 0]) }));
    expect(roll).not.toBeNull();
    expect(Array.from(roll!.times)).toEqual([-1500, -1490, -1480]);
    expect(Array.from(roll!.steer)).toEqual([0, -99, 40]);
    expect(Array.from(roll!.gas)).toEqual([1, 1, 0]);
    expect(Array.from(roll!.brake)).toEqual([0, 1, 0]);
    expect(roll!.fromInputs).toBe(true);
  });

  it("falls back to the sample stream", () => {
    const data = samples({
      times: new Float64Array([0, 1000]),
      steer: new Float32Array([10, 20]),
      gas: new Float32Array([1, 0]),
      brake: new Float32Array([0, 1]),
    });
    const roll = buildRoll(run({ samples: data }));
    expect(roll).not.toBeNull();
    expect(roll!.times).toBe(data.times);
    expect(Array.from(roll!.steer)).toEqual([10, 20]);
    expect(roll!.fromInputs).toBe(false);
  });

  it("is null without inputs and samples", () => {
    expect(buildRoll(run({}))).toBeNull();
  });
});

describe("rollAt", () => {
  it("holds the last tick and flags times outside the stream", () => {
    const roll = buildRoll(run({ inputs: inputStream(-1500, [0, -99], [1, 0]) }));
    expect(roll).not.toBeNull();
    expect(rollAt(roll!, -1501).inRange).toBe(false);
    expect(rollAt(roll!, -1500)).toMatchObject({ steer: 0, gas: true, brake: false, inRange: true });
    expect(rollAt(roll!, -1000)).toMatchObject({ steer: -99, gas: false, inRange: true });
  });
});

describe("samplesAt", () => {
  it("interpolates speed and steps gear and rpm", () => {
    const data = run({
      samples: samples({
        times: new Float64Array([0, 1000]),
        speeds: new Float32Array([10, 20]),
        gear: new Float32Array([3, 4]),
        rpm: new Uint8Array([100, 200]),
      }),
    });
    const state = samplesAt(data, 500);
    expect(state.speedKmh).toBeCloseTo(15 * 3.6, 5);
    expect(state.gear).toBe(3);
    expect(state.rpm).toBe(100);
    expect(samplesAt(data, -1)).toEqual({ speedKmh: null, gear: null, rpm: null });
    expect(samplesAt(data, 600_000).gear).toBe(4);
  });

  it("returns nulls without samples", () => {
    expect(samplesAt(run({}), 0)).toEqual({ speedKmh: null, gear: null, rpm: null });
  });
});

describe("timeDeltaAtDistance", () => {
  it("reports time gained at equal distance", () => {
    const reference = {
      times: new Float64Array([0, 10_000, 20_000]),
      distance: new Float64Array([0, 100, 200]),
      totalDistance: 200,
    };
    const selected = {
      times: new Float64Array([0, 9_000, 20_000]),
      distance: new Float64Array([0, 100, 200]),
      totalDistance: 200,
    };
    expect(timeDeltaAtDistance(selected, reference)).toEqual([0, -1000, 0]);
  });

  it("interpolates between reference samples", () => {
    const reference = {
      times: new Float64Array([0, 10_000]),
      distance: new Float64Array([0, 100]),
      totalDistance: 100,
    };
    const selected = {
      times: new Float64Array([0, 5_500]),
      distance: new Float64Array([0, 55]),
      totalDistance: 55,
    };
    expect(timeDeltaAtDistance(selected, reference)).toEqual([0, 0]);
  });

  it("leaves null where the reference never got", () => {
    const reference = {
      times: new Float64Array([0, 10_000]),
      distance: new Float64Array([0, 100]),
      totalDistance: 100,
    };
    const selected = {
      times: new Float64Array([0, 12_000]),
      distance: new Float64Array([0, 300]),
      totalDistance: 300,
    };
    expect(timeDeltaAtDistance(selected, reference)).toEqual([0, null]);
  });
});

describe("buildSpeedSeries", () => {
  it("puts every run on the union of sample times", () => {
    const a = run({
      id: "a",
      samples: samples({ times: new Float64Array([0, 100]), speeds: new Float32Array([10, 20]) }),
    });
    const b = run({
      id: "b",
      samples: samples({ times: new Float64Array([50]), speeds: new Float32Array([5]) }),
    });
    const data = buildSpeedSeries([a, b]);
    expect(data.xs).toEqual([0, 0.05, 0.1]);
    expect(data.series[0].values.map((v) => Math.round((v ?? -1) * 10) / 10)).toEqual([36, 36, 72]);
    expect(data.series[1].values).toEqual([null, 18, null]);
  });
});

describe("deriveRun", () => {
  it("collects respawn times and speeds", () => {
    const derived = deriveRun(
      run({
        samples: samples({
          times: new Float64Array([0, 1000, 2000]),
          positions: straightPositions(3),
          speeds: new Float32Array([0, 10, 20]),
        }),
        inputs: inputs([
          { timeMs: 40, type: "steer" },
          { timeMs: 60, type: "respawn" },
          { timeMs: 70, type: "secondary_respawn" },
        ]),
      }),
    );
    expect(derived).not.toBeNull();
    expect(derived!.respawnTimes).toEqual([60, 70]);
    expect(derived!.distance[2]).toBeCloseTo(20, 5);
    expect(derived!.topSpeedKmh).toBeCloseTo(72, 5);
    expect(derived!.avgSpeedKmh).toBeCloseTo(36, 5);
  });

  it("returns null without samples", () => {
    expect(deriveRun(run({}))).toBeNull();
  });
});

describe("buildDeltaSeries", () => {
  it("converts the delta to seconds", () => {
    const reference = run({
      raceTimeMs: 20_000,
      samples: samples({ times: new Float64Array([0, 10_000]), positions: straightPositions(2) }),
    });
    reference.derived = deriveRun(reference);
    const selected = run({
      raceTimeMs: 19_000,
      samples: samples({ times: new Float64Array([0, 9_000]), positions: straightPositions(2) }),
    });
    selected.derived = deriveRun(selected);
    const delta = buildDeltaSeries(selected, reference);
    expect(delta).not.toBeNull();
    expect(delta!.xs).toEqual([0, 9]);
    expect(delta!.values).toEqual([0, -1_000 / 1_000]);
  });
});

describe("splitsFor", () => {
  it("compares checkpoint index against the reference", () => {
    const reference = run({
      raceTimeMs: 30_000,
      checkpoints: [
        { timeMs: 10_000, stuntsScore: 0 },
        { timeMs: 20_000, stuntsScore: 0 },
      ],
    });
    const selected = run({
      raceTimeMs: 29_500,
      checkpoints: [
        { timeMs: 9_800, stuntsScore: 0 },
        { timeMs: 20_100, stuntsScore: 0 },
      ],
    });
    const rows = splitsFor(selected, reference);
    expect(rows.map((row) => row.label)).toEqual(["CP 1", "CP 2", "Finish"]);
    expect(rows.map((row) => row.deltaMs)).toEqual([-200, 100, -500]);
  });

  it("reports no deltas without a reference", () => {
    const selected = run({ raceTimeMs: 1000, checkpoints: [{ timeMs: 500, stuntsScore: 0 }] });
    const rows = splitsFor(selected, null);
    expect(rows.every((row) => row.deltaMs === null)).toBe(true);
  });

  it("shows the last checkpoint entry as the finish, once", () => {
    const selected = run({
      raceTimeMs: 30_000,
      checkpoints: [
        { timeMs: 10_000, stuntsScore: 0 },
        { timeMs: 20_000, stuntsScore: 0 },
        { timeMs: 30_000, stuntsScore: 0 },
      ],
    });
    expect(lastCheckpointIsFinish(selected)).toBe(true);
    const rows = splitsFor(selected, null);
    expect(rows.map((row) => row.label)).toEqual(["CP 1", "CP 2", "Finish"]);
    expect(rows.map((row) => row.timeMs)).toEqual([10_000, 20_000, 30_000]);
  });

  it("keeps a last checkpoint that is not the finish as its own row", () => {
    const selected = run({
      raceTimeMs: 30_000,
      checkpoints: [{ timeMs: 10_000, stuntsScore: 0 }],
    });
    expect(lastCheckpointIsFinish(selected)).toBe(false);
    expect(splitsFor(selected, null).map((row) => row.label)).toEqual(["CP 1", "Finish"]);
  });

  it("falls back to the end of the samples without a race time", () => {
    const selected = run({
      checkpoints: [{ timeMs: 10_000, stuntsScore: 0 }],
      samples: samples({ times: new Float64Array([0, 20_000]), positions: straightPositions(2) }),
    });
    selected.derived = deriveRun(selected);
    const rows = splitsFor(selected, null);
    expect(rows.map((row) => row.label)).toEqual(["CP 1", "Finish"]);
    expect(rows[1].timeMs).toBe(20_000);
  });

  it("compares the finish against the reference race time", () => {
    const reference = run({
      raceTimeMs: 30_000,
      checkpoints: [
        { timeMs: 10_000, stuntsScore: 0 },
        { timeMs: 30_000, stuntsScore: 0 },
      ],
    });
    const selected = run({
      raceTimeMs: 29_000,
      checkpoints: [
        { timeMs: 10_500, stuntsScore: 0 },
        { timeMs: 29_000, stuntsScore: 0 },
      ],
    });
    const rows = splitsFor(selected, reference);
    expect(rows.map((row) => row.label)).toEqual(["CP 1", "Finish"]);
    expect(rows.map((row) => row.deltaMs)).toEqual([500, -1000]);
  });
});
