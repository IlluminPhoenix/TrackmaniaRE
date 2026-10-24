//! Values derived from a parsed run: cumulative distance, speed, airtime,
//! respawn and wall-hit markers, comparison deltas and splits.
//!
//! Everything here is pure and operates on plain typed arrays so it can be
//! unit tested without a browser.

import type { Derived, RunData, Samples, WallHit } from "./types";

/** Segment speeds above this are treated as respawns/teleports, not driving. */
const MAX_SEGMENT_SPEED = 150; // m/s

/** Cumulative distance along the sampled positions, in metres. */
export function cumulativeDistance(
  times: Float64Array,
  positions: Float32Array,
  maxSegmentSpeed = MAX_SEGMENT_SPEED,
): Float64Array {
  const n = times.length;
  const out = new Float64Array(n);
  let sum = 0;
  for (let i = 1; i < n; i++) {
    const dx = positions[3 * i] - positions[3 * i - 3];
    const dy = positions[3 * i + 1] - positions[3 * i - 2];
    const dz = positions[3 * i + 2] - positions[3 * i - 1];
    const dist = Math.sqrt(dx * dx + dy * dy + dz * dz);
    const dt = (times[i] - times[i - 1]) / 1000;
    if (dt > 0 && dist / dt <= maxSegmentSpeed) sum += dist;
    out[i] = sum;
  }
  return out;
}

/**
 * Wall-hit candidates: a sudden loss of horizontal speed in one 20 Hz step.
 * Approximate by design (hard landings and kerb strikes look similar), so the
 * UI labels these as guesses.
 */
export function detectWallHits(
  times: Float64Array,
  velocities: Float32Array,
  minDrop = 5,
  maxHits = 200,
): WallHit[] {
  const n = times.length;
  const raw: WallHit[] = [];
  for (let i = 1; i < n; i++) {
    const x0 = velocities[3 * i - 3];
    const z0 = velocities[3 * i - 1];
    const x1 = velocities[3 * i];
    const z1 = velocities[3 * i + 2];
    const h0 = Math.hypot(x0, z0);
    const h1 = Math.hypot(x1, z1);
    if (h0 < 3) continue; // too slow to tell braking from hitting a wall
    const drop = h0 - h1;
    if (drop < minDrop) continue;
    const dt = (times[i] - times[i - 1]) / 1000;
    if (dt <= 0) continue;
    // Ignore respawn teleports: the same step also moves the car far.
    const decel = drop / dt;
    if (decel > 400) continue;
    const cos = h0 > 0 && h1 > 0 ? (x0 * x1 + z0 * z1) / (h0 * h1) : 0;
    if (cos > 0.55 && drop < 12) continue; // straight-line braking
    raw.push({ timeMs: times[i], index: i, drop });
  }
  // Merge hits that belong to the same impact.
  const merged: WallHit[] = [];
  for (const hit of raw) {
    const last = merged[merged.length - 1];
    if (last && hit.timeMs - last.timeMs < 250) {
      if (hit.drop > last.drop) merged[merged.length - 1] = hit;
      continue;
    }
    merged.push(hit);
  }
  if (merged.length <= maxHits) return merged;
  return [...merged].sort((a, b) => b.drop - a.drop).slice(0, maxHits).sort((a, b) => a.timeMs - b.timeMs);
}

/** Per-sample duration in ms, clamped so teleports do not weigh in. */
function sampleDurationMs(times: Float64Array | Float32Array, i: number): number {
  const dt = i + 1 < times.length ? times[i + 1] - times[i] : 50;
  return dt > 0 && dt < 500 ? dt : 50;
}

/** Total airborne time from the ground mode stream (samples are 20 Hz). */
export function airborneTime(samples: Samples): { flags: Uint8Array; totalMs: number } {
  const n = samples.times.length;
  const flags = new Uint8Array(n);
  let totalMs = 0;
  for (let i = 0; i < n; i++) {
    if (samples.groundMode[i] === 0) {
      flags[i] = 1;
      totalMs += sampleDurationMs(samples.times, i);
    }
  }
  return { flags, totalMs };
}

// ---- grip ------------------------------------------------------------------

/** Slip angle above which a grounded car counts as sliding, in degrees. */
export const SLIP_SLIDE_DEG = 15;

/** Below this speed the heading says nothing about grip, in m/s. */
export const SLIP_MIN_SPEED = 3;

/** Grip classification values (see `gripStates` and `Derived.gripStates`). */
export const GRIP = 0;
export const SLIDE = 1;
export const AIR = 2;

export interface GripStates {
  /** GRIP / SLIDE / AIR per sample. */
  states: Uint8Array;
  gripMs: number;
  slideMs: number;
  airMs: number;
  /** Largest slip angle seen while grounded and moving, degrees. */
  maxSlipDeg: number;
}

/**
 * Car forward axis in world space, written into `out`.
 *
 * The recorded rotation is an axis-angle pair, `axis` built from the heading
 * and pitch the same way the 3D view builds it, applied to the local forward
 * `+Z` (Rodrigues). Checked against six replays' velocity direction.
 */
export function carForward(rotations: Float32Array, i: number, out: Float32Array): void {
  let x = Math.cos(rotations[3 * i + 2]) * Math.cos(rotations[3 * i + 1]);
  let y = Math.cos(rotations[3 * i + 2]) * Math.sin(rotations[3 * i + 1]);
  let z = Math.sin(rotations[3 * i + 2]);
  const length = Math.hypot(x, y, z);
  if (length > 1e-6) {
    x /= length;
    y /= length;
    z /= length;
  } else {
    x = 0;
    y = 0;
    z = 1;
  }
  const angle = rotations[3 * i] * 2;
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  const oneMinusCos = 1 - cos;
  // Rodrigues on v = (0, 0, 1): k x v = (y, -x, 0), k . v = z.
  out[0] = y * sin + x * z * oneMinusCos;
  out[1] = -x * sin + y * z * oneMinusCos;
  out[2] = cos + z * z * oneMinusCos;
}

/**
 * Angle in degrees between the car's forward axis and its velocity. A car that
 * drives straight ahead keeps this near zero; it grows when the car slides
 * sideways relative to where it points.
 */
export function slipAngleDeg(samples: Samples, i: number, forward?: Float32Array): number {
  const vx = samples.velocities[3 * i];
  const vy = samples.velocities[3 * i + 1];
  const vz = samples.velocities[3 * i + 2];
  const speed = Math.hypot(vx, vy, vz);
  if (speed < 1e-6) return 0;
  const axis = forward ?? new Float32Array(3);
  carForward(samples.rotations, i, axis);
  const dot = (vx * axis[0] + vy * axis[1] + vz * axis[2]) / speed;
  return (Math.acos(Math.max(-1, Math.min(1, dot))) * 180) / Math.PI;
}

/**
 * Grip per sample: airborne while the ground mode says so, sliding while the
 * car is on the ground and its velocity points away from its heading by more
 * than {@link SLIP_SLIDE_DEG}. Too slow to tell counts as grip.
 */
export function gripStates(samples: Samples): GripStates {
  const n = samples.times.length;
  const { flags: airborne } = airborneTime(samples);
  const states = new Uint8Array(n);
  const axis = new Float32Array(3);
  let gripMs = 0;
  let slideMs = 0;
  let airMs = 0;
  let maxSlipDeg = 0;
  for (let i = 0; i < n; i++) {
    const dt = sampleDurationMs(samples.times, i);
    if (airborne[i]) {
      states[i] = AIR;
      airMs += dt;
      continue;
    }
    const speed = samples.speeds[i];
    const slip = speed >= SLIP_MIN_SPEED ? slipAngleDeg(samples, i, axis) : 0;
    if (slip > maxSlipDeg) maxSlipDeg = slip;
    if (slip > SLIP_SLIDE_DEG) {
      states[i] = SLIDE;
      slideMs += dt;
    } else {
      states[i] = GRIP;
      gripMs += dt;
    }
  }
  return { states, gripMs, slideMs, airMs, maxSlipDeg };
}

/** Builds the per-run derived block. Null when the run has no samples. */
export function deriveRun(run: RunData): Derived | null {
  const samples = run.samples;
  if (!samples || samples.times.length === 0) return null;
  const times = samples.times;
  const distance = cumulativeDistance(times, samples.positions);
  const { flags: airborne, totalMs: airborneMs } = airborneTime(samples);
  const respawnTimes =
    run.inputs?.events.filter((e) => e.type === "respawn" || e.type === "secondary_respawn").map((e) => e.timeMs) ?? [];
  const wallHits = detectWallHits(times, samples.velocities);
  const speedKmh = new Float32Array(samples.speeds.length);
  let top = 0;
  let sum = 0;
  for (let i = 0; i < speedKmh.length; i++) {
    const kmh = samples.speeds[i] * 3.6;
    speedKmh[i] = kmh;
    if (kmh > top) top = kmh;
    sum += kmh;
  }
  const durationMs = Math.max(times[times.length - 1], run.raceTimeMs ?? 0, run.sampleEndMs ?? 0);
  const grip = gripStates(samples);
  return {
    distance,
    totalDistance: distance[distance.length - 1],
    speedKmh,
    airborne,
    airborneMs,
    respawnTimes,
    wallHits,
    topSpeedKmh: top,
    avgSpeedKmh: speedKmh.length ? sum / speedKmh.length : 0,
    durationMs,
    gripStates: grip.states,
    slideMs: grip.slideMs,
    maxSlipDeg: grip.maxSlipDeg,
  };
}

/** Index of the last sample at or before `t`; -1 when `t` is before the first. */
export function sampleIndexAt(times: Float64Array | Float32Array, t: number): number {
  if (times.length === 0) return -1;
  if (t < times[0]) return -1;
  if (t >= times[times.length - 1]) return times.length - 1;
  let lo = 0;
  let hi = times.length - 1;
  while (lo + 1 < hi) {
    const mid = (lo + hi) >> 1;
    if (times[mid] <= t) lo = mid;
    else hi = mid;
  }
  return lo;
}

// ---- input state over time ------------------------------------------------

export interface RollData {
  times: Float64Array | Float32Array;
  steer: Float64Array | Float32Array;
  gas: Float64Array | Float32Array;
  brake: Float64Array | Float32Array;
  /** True when this is the 100 Hz input stream, false for the 20 Hz samples. */
  fromInputs: boolean;
}

/**
 * The input state over time: the 100 Hz decoded stream when the run has one,
 * else the 20 Hz sample stream.
 *
 * Stream times start at the chunk's start offset, which is typically around
 * -1500 ms: TM2020 records inputs during the pre-start hold, so early ticks sit
 * before t=0 and line up with the input events (the TAS script, respawns).
 */
export function buildRoll(run: RunData): RollData | null {
  const inputs = run.inputs;
  if (inputs && inputs.ticks > 0) {
    const offset = inputs.startOffsetMs ?? 0;
    const n = inputs.ticks;
    const times = new Float64Array(n);
    const steer = new Float64Array(n);
    const gas = new Float64Array(n);
    const brake = new Float64Array(n);
    for (let i = 0; i < n; i++) {
      times[i] = offset + i * 10;
      steer[i] = inputs.steer[i];
      gas[i] = inputs.flags[i] & 1 ? 1 : 0;
      brake[i] = inputs.flags[i] & 2 ? 1 : 0;
    }
    return { times, steer, gas, brake, fromInputs: true };
  }
  const samples = run.samples;
  if (samples && samples.times.length > 0) {
    return { times: samples.times, steer: samples.steer, gas: samples.gas, brake: samples.brake, fromInputs: false };
  }
  return null;
}

/**
 * `buildRoll` for the run most recently asked about, so the piano roll and the
 * live display share one copy of a long input stream. A single slot is enough:
 * both panels only ever ask for the selected run.
 */
const rollCache: { key: string; roll: RollData | null } = { key: "", roll: null };

export function rollKey(run: RunData): string {
  return `${run.id}:${run.inputs?.ticks ?? 0}:${run.sampleCount}`;
}

export function cachedRoll(run: RunData): RollData | null {
  const key = rollKey(run);
  if (rollCache.key !== key) rollCache.roll = buildRoll(run);
  rollCache.key = key;
  return rollCache.roll;
}

export interface RollState {
  steer: number;
  gas: boolean;
  brake: boolean;
  /** False when `t` is outside the recorded stream. */
  inRange: boolean;
}

/** Input state at `t`, holding the last tick at or before it. */
export function rollAt(roll: RollData, t: number): RollState {
  const i = sampleIndexAt(roll.times, t);
  if (i < 0) return { steer: 0, gas: false, brake: false, inRange: false };
  return {
    steer: roll.steer[i] ?? 0,
    gas: (roll.gas[i] ?? 0) >= 0.5,
    brake: (roll.brake[i] ?? 0) >= 0.5,
    inRange: true,
  };
}

export interface SampleState {
  speedKmh: number | null;
  gear: number | null;
  rpm: number | null;
}

/** Car state at `t` from the 20 Hz samples; nulls when the run has none. */
export function samplesAt(run: RunData, t: number): SampleState {
  const samples = run.samples;
  if (!samples || samples.times.length === 0) return { speedKmh: null, gear: null, rpm: null };
  const i = sampleIndexAt(samples.times, t);
  if (i < 0) return { speedKmh: null, gear: null, rpm: null };
  const speed = interpolateAt(samples.times, samples.speeds, t);
  return {
    speedKmh: speed === null ? null : speed * 3.6,
    gear: samples.gear[i] ?? null,
    rpm: samples.rpm[i] ?? null,
  };
}

export interface Track {
  times: Float64Array;
  distance: Float64Array;
  totalDistance: number;
}

export function trackOf(run: RunData): Track | null {
  if (!run.derived || !run.samples) return null;
  return { times: run.samples.times, distance: run.derived.distance, totalDistance: run.derived.totalDistance };
}

/**
 * Time delta in ms between a run and a reference at equal distance.
 * Positive means the run is slower (it reached the same distance later).
 * `null` where the run covers more distance than the reference ever did.
 */
export function timeDeltaAtDistance(run: Track, reference: Track): (number | null)[] {
  const out: (number | null)[] = new Array(run.times.length).fill(null);
  const refTimes = reference.times;
  const refDist = reference.distance;
  if (refTimes.length < 2) return out;
  let j = 1; // walking pointer: both distance arrays are non-decreasing
  for (let i = 0; i < run.times.length; i++) {
    const d = run.distance[i];
    if (d > reference.totalDistance + 0.5) break;
    while (j < refDist.length - 1 && refDist[j] < d) j++;
    while (j > 1 && refDist[j - 1] >= d) j--;
    const d0 = refDist[j - 1];
    const d1 = refDist[j];
    let tRef: number;
    if (d <= d0) {
      // Same distance held for a while (standstill): first time it was reached.
      let k = j - 1;
      while (k > 0 && refDist[k - 1] === d0) k--;
      tRef = refTimes[k];
    } else if (d1 === d0) {
      tRef = refTimes[j];
    } else {
      const alpha = (d - d0) / (d1 - d0);
      tRef = refTimes[j - 1] + alpha * (refTimes[j] - refTimes[j - 1]);
    }
    out[i] = run.times[i] - tRef;
  }
  return out;
}

export interface SpeedSeries {
  /** Time grid in seconds. */
  xs: number[];
  series: { run: RunData; values: (number | null)[] }[];
}

/** Speed of every run on the union of their sample times, km/h. */
export function buildSpeedSeries(runs: RunData[]): SpeedSeries {
  const withSamples = runs.filter((r) => r.samples && r.samples.times.length > 0);
  const xsMs = new Set<number>();
  for (const run of withSamples) {
    for (const t of run.samples!.times) xsMs.add(t);
  }
  const sorted = [...xsMs].sort((a, b) => a - b);
  const series = withSamples.map((run) => {
    const s = run.samples!;
    const values: (number | null)[] = new Array(sorted.length).fill(null);
    let i = 0;
    for (let k = 0; k < sorted.length; k++) {
      const t = sorted[k];
      if (t < s.times[0] || t > s.times[s.times.length - 1]) continue;
      while (i + 1 < s.times.length && s.times[i + 1] <= t) i++;
      values[k] = s.speeds[i] * 3.6;
    }
    return { run, values };
  });
  return { xs: sorted.map((t) => t / 1000), series };
}

export interface DeltaSeries {
  /** Selected run's sample times in seconds. */
  xs: number[];
  /** Delta in seconds, null where the reference cannot reach. */
  values: (number | null)[];
}

export function buildDeltaSeries(run: RunData, reference: RunData): DeltaSeries | null {
  const a = trackOf(run);
  const b = trackOf(reference);
  if (!a || !b) return null;
  const deltas = timeDeltaAtDistance(a, b);
  return { xs: [...a.times].map((t) => t / 1000), values: deltas.map((d) => (d === null ? null : d / 1000)) };
}

export interface SplitRow {
  label: string;
  timeMs: number;
  deltaMs: number | null;
}

/**
 * True when the last checkpoint entry is the finish line. TM2020 stores it
 * there, and its time matches `race_time_ms` in every replay of the test
 * corpus, so a run with N entries has N-1 checkpoints and one finish.
 */
export function lastCheckpointIsFinish(run: RunData): boolean {
  const last = run.checkpoints[run.checkpoints.length - 1];
  return last !== undefined && run.raceTimeMs !== null && Math.abs(last.timeMs - run.raceTimeMs) <= 1;
}

/**
 * Checkpoint and finish rows, delta against another run when given.
 *
 * The finish line is the last checkpoint entry, so it is shown as "Finish"
 * instead of being listed twice. Without checkpoints the finish falls back to
 * the race time or the end of the samples.
 */
export function splitsFor(run: RunData, reference: RunData | null): SplitRow[] {
  const rows: SplitRow[] = [];
  const refCps = reference?.checkpoints ?? [];
  const cps = run.checkpoints;
  const raceTime = run.raceTimeMs;
  const lastIndex = cps.length - 1;
  const lastIsFinish = lastCheckpointIsFinish(run);
  const checkpointCount = lastIsFinish ? lastIndex : cps.length;
  for (let index = 0; index < checkpointCount; index++) {
    const cp = cps[index];
    const ref = refCps[index];
    rows.push({
      label: `CP ${index + 1}`,
      timeMs: cp.timeMs,
      deltaMs: reference ? (ref ? cp.timeMs - ref.timeMs : null) : null,
    });
  }
  const finish = lastIsFinish ? cps[lastIndex].timeMs : (raceTime ?? run.derived?.durationMs ?? null);
  if (finish !== null) {
    const refFinish = reference?.raceTimeMs ?? null;
    rows.push({
      label: "Finish",
      timeMs: finish,
      deltaMs: reference && refFinish !== null ? finish - refFinish : null,
    });
  }
  return rows;
}

/** Linear interpolation of a scalar track at time `t`. */
export function interpolateAt(times: Float64Array, values: Float32Array, t: number): number | null {
  const i = sampleIndexAt(times, t);
  if (i < 0) return null;
  if (i + 1 >= times.length) return values[values.length - 1];
  const t0 = times[i];
  const t1 = times[i + 1];
  const alpha = t1 > t0 ? (t - t0) / (t1 - t0) : 0;
  return values[i] + (values[i + 1] - values[i]) * alpha;
}
