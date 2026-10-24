//! Shared types for the web frontend.

/** One decoded input event (see `inputs.rs` / `event_to_json` in the extractor). */
export interface InputEventLite {
  timeMs: number;
  tick: number;
  type: string;
  /** boolean for on/off events, number for steer / action slot. */
  value?: boolean | number;
  /** Action slot event only. */
  slot?: number;
}

/** Per-sample vehicle timelines, copied out of wasm into plain typed arrays. */
export interface Samples {
  /** Sample times in ms since the run start. */
  times: Float64Array;
  /** x, y, z triples, metres. Y is up. */
  positions: Float32Array;
  /** x, y, z triples, m/s. */
  velocities: Float32Array;
  /** angle, axis heading, axis pitch triples (radians). */
  rotations: Float32Array;
  /** Forward speed, m/s. */
  speeds: Float32Array;
  /** Sideways speed, m/s. */
  sideSpeeds: Float32Array;
  steer: Float32Array;
  gas: Float32Array;
  brake: Float32Array;
  rpm: Uint8Array;
  gear: Float32Array;
  turbo: Uint8Array;
  isTurbo: Uint8Array;
  vehicleState: Uint8Array;
  /** 0 airborne, 1 low speed, 3 driving. */
  groundMode: Uint8Array;
  /** Four wheel material ids per sample. */
  groundMaterial: Uint8Array;
}

/** Per-tick input state plus the sparse event list. */
export interface Inputs {
  format: string;
  version: number;
  startOffsetMs: number | null;
  truncated: boolean;
  /** Number of 100 Hz ticks covered by `steer` / `flags`. */
  ticks: number;
  /** Per-tick steering byte, -127 .. 127. */
  steer: Int8Array;
  /** Per-tick flags: bit 0 accelerate, bit 1 brake, bit 2 horn. */
  flags: Uint8Array;
  events: InputEventLite[];
}

export interface Checkpoint {
  timeMs: number;
  stuntsScore: number;
}

export interface MapRef {
  bytes: Uint8Array;
  name: string;
  uid: string | null;
  environment: string | null;
}

/** Everything the UI needs about one ghost (one "run"). */
export interface RunData {
  id: string;
  fileName: string;
  fileSize: number;
  /** Index of the ghost inside the replay file. */
  ghostIndex: number;
  /** True for the combined record stored at replay level, which is not a ghost. */
  isReplayRecord: boolean;
  nickname: string;
  login: string | null;
  raceTimeMs: number | null;
  respawns: number | null;
  checkpoints: Checkpoint[];
  warnings: string[];
  mapUid: string | null;
  mapName: string | null;
  mapEnvironment: string | null;
  mapAuthor: string | null;
  samples: Samples | null;
  sampleCount: number;
  sampleStartMs: number | null;
  sampleEndMs: number | null;
  inputs: Inputs | null;
  map: MapRef | null;
  /** TICK input script, null when the ghost has no TM2020 input stream. */
  tas: string | null;
  tasDropped: number;
  color: string;
  /** Highest sample time, or the race time when there are no samples. */
  durationMs: number;
  derived: Derived | null;
}

export interface WallHit {
  timeMs: number;
  index: number;
  /** Speed lost over the sample, m/s. */
  drop: number;
}

/** Values derived once per run (see `derive.ts`). */
export interface Derived {
  /** Cumulative distance in metres, one entry per sample. */
  distance: Float64Array;
  totalDistance: number;
  speedKmh: Float32Array;
  /** 1 while airborne. */
  airborne: Uint8Array;
  airborneMs: number;
  /** Times of respawn input events. */
  respawnTimes: number[];
  wallHits: WallHit[];
  topSpeedKmh: number;
  avgSpeedKmh: number;
  durationMs: number;
  /** GRIP / SLIDE / AIR per sample. */
  gripStates: Uint8Array;
  slideMs: number;
  /** Largest slip angle while grounded, degrees. */
  maxSlipDeg: number;
}
