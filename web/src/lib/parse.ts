//! wasm facade: init once, parse a file into plain JavaScript runs, free the
//! wasm side immediately (the UI works on copies).

import init, { Replay } from "./wasm/trackmaniare_wasm.js";
import { deriveRun } from "./derive.ts";
import { readSamples } from "./samples.ts";
import type { Checkpoint, InputEventLite, Inputs, MapRef, RunData } from "./types";

let initPromise: Promise<void> | null = null;

/**
 * Loads and instantiates the wasm module. Safe to call repeatedly.
 *
 * The optional argument is what wasm-bindgen accepts as an init input
 * (`{ module_or_path }`, a URL or the raw bytes). The browser build calls it
 * without one and lets the glue resolve the `.wasm` next to the JS; the Node
 * smoke test passes the bytes because `fetch` cannot read `file:` URLs.
 */
export function initWasm(input?: Parameters<typeof init>[0]): Promise<void> {
  initPromise ??= init(input).then(() => undefined);
  return initPromise;
}

interface GhostMeta {
  nickname?: string | null;
  login?: string | null;
  race_time_ms?: number | null;
  respawns?: number | null;
  checkpoints?: { time_ms: number; stunts_score: number }[];
  record_data?: { vehicle_sample_count?: number; start_ms?: number | null; end_ms?: number | null } | null;
}

interface ReplayMeta {
  map_uid?: string | null;
  map_environment?: string | null;
  map_author?: string | null;
  time_ms?: number | null;
  nickname?: string | null;
  community_xml?: Record<string, string> | null;
  record_data?: { vehicle_sample_count?: number } | null;
}

interface Meta {
  file?: string;
  file_size?: number;
  replay?: ReplayMeta;
  ghosts?: GhostMeta[];
  warnings?: string[];
}

/** Negative values in ghost data mean "not recorded" (TM2020 writes -1). */
function recorded(value: number | null | undefined): number | null {
  return value === null || value === undefined || value < 0 ? null : value;
}

interface RawEvent {
  time_ms?: number;
  tick?: number;
  type?: string;
  value?: boolean | number;
  slot?: number;
  /** Legacy (TMU..TM2) control entries. */
  control?: string;
  onoff?: number;
}

interface InputsMeta {
  format?: string;
  version?: number;
  start_offset_ms?: number | null;
  ticks?: number;
  decoded_truncated?: boolean;
  events?: RawEvent[];
}

function readEvents(raw: RawEvent[] | undefined): InputEventLite[] {
  const out: InputEventLite[] = [];
  for (const e of raw ?? []) {
    const timeMs = e.time_ms ?? 0;
    if (e.control !== undefined) {
      out.push({
        timeMs,
        tick: Math.round(timeMs / 10),
        type: e.control.toLowerCase(),
        value: e.onoff === 1,
      });
    } else {
      const event: InputEventLite = { timeMs, tick: e.tick ?? Math.round(timeMs / 10), type: e.type ?? "unknown" };
      if (e.value !== undefined) event.value = e.value;
      if (e.slot !== undefined) event.slot = e.slot;
      out.push(event);
    }
  }
  return out;
}

function readInputs(replay: Replay, ghost: number): Inputs | null {
  const raw = replay.inputs_json(ghost);
  if (!raw || raw === "null") return null;
  const meta = JSON.parse(raw) as InputsMeta;
  return {
    format: meta.format ?? "unknown",
    version: meta.version ?? 0,
    startOffsetMs: meta.start_offset_ms ?? null,
    truncated: meta.decoded_truncated ?? false,
    ticks: replay.input_ticks(ghost),
    steer: replay.input_steer(ghost),
    flags: replay.input_flags(ghost),
    events: readEvents(meta.events),
  };
}

/**
 * Parses a `.Replay.Gbx` / `.Ghost.Gbx` and returns one entry per ghost.
 * Throws when the container cannot be read at all; every other problem is a
 * warning on the run.
 *
 * `color` is left empty: the caller assigns a chart colour when adding runs.
 */
export function parseReplayBytes(bytes: Uint8Array, fileName: string): RunData[] {
  const replay = new Replay(bytes, fileName);
  try {
    const meta = JSON.parse(replay.metadata_json()) as Meta;
    const fileWarnings = meta.warnings ?? [];
    const mapBytes = replay.map_bytes();
    const replayMeta = meta.replay ?? {};
    const mapName = replay.map_name() ?? replayMeta.community_xml?.name ?? null;
    const map: MapRef | null =
      mapBytes.length > 0
        ? {
            bytes: mapBytes,
            name: mapName ?? "map",
            uid: replayMeta.map_uid ?? null,
            environment: replayMeta.map_environment ?? null,
          }
        : null;

    const runs: RunData[] = [];
    const ghosts = meta.ghosts ?? [];
    for (let g = 0; g < ghosts.length; g++) {
      const ghostMeta = ghosts[g] ?? {};
      const tracks = Array.from(replay.vehicle_track_indices(g));
      const samples = readSamples(replay, g, tracks);
      const inputs = readInputs(replay, g);
      const tas = inputs?.format === "tm2020-bitpacked" ? (replay.tas_inputs(g) ?? null) : null;
      const checkpoints: Checkpoint[] = (ghostMeta.checkpoints ?? []).map((cp) => ({
        timeMs: cp.time_ms,
        stuntsScore: cp.stunts_score,
      }));
      const raceTimeMs = recorded(ghostMeta.race_time_ms);
      const sampleEndMs = ghostMeta.record_data?.end_ms ?? null;
      const durationMs = Math.max(
        samples ? samples.times[samples.times.length - 1] : 0,
        raceTimeMs ?? 0,
        sampleEndMs ?? 0,
      );
      const run: RunData = {
        id: `${fileName}#${g}`,
        fileName,
        fileSize: meta.file_size ?? bytes.length,
        ghostIndex: g,
        isReplayRecord: false,
        nickname: ghostMeta.nickname ?? "(unnamed)",
        login: ghostMeta.login ?? null,
        raceTimeMs,
        respawns: recorded(ghostMeta.respawns),
        checkpoints,
        warnings: fileWarnings,
        mapUid: replayMeta.map_uid ?? null,
        mapName,
        mapEnvironment: replayMeta.map_environment ?? null,
        mapAuthor: replayMeta.map_author ?? null,
        samples,
        sampleCount: samples ? samples.times.length : 0,
        sampleStartMs: ghostMeta.record_data?.start_ms ?? null,
        sampleEndMs,
        inputs,
        map,
        tas,
        tasDropped: tas ? replay.tas_dropped_events(g) : 0,
        color: "",
        durationMs,
        derived: null,
      };
      run.derived = deriveRun(run);
      runs.push(run);
    }

    // Merge tooling sometimes writes a single record for the whole replay
    // instead of one per ghost; the extractor reports its samples as ghost -1.
    // Give it its own entry so the path is usable and never mis-attributed.
    if (replay.has_replay_record()) {
      const holder = ghosts.length;
      const tracks = Array.from(replay.vehicle_track_indices(holder));
      const samples = readSamples(replay, holder, tracks);
      if (samples) {
        const run: RunData = {
          id: `${fileName}#merged`,
          fileName,
          fileSize: meta.file_size ?? bytes.length,
          ghostIndex: holder,
          isReplayRecord: true,
          nickname: replayMeta.nickname ?? "(merged record)",
          login: null,
          raceTimeMs: recorded(replayMeta.time_ms),
          respawns: null,
          checkpoints: [],
          warnings: fileWarnings,
          mapUid: replayMeta.map_uid ?? null,
          mapName,
          mapEnvironment: replayMeta.map_environment ?? null,
          mapAuthor: replayMeta.map_author ?? null,
          samples,
          sampleCount: samples.times.length,
          sampleStartMs: samples.times[0],
          sampleEndMs: samples.times[samples.times.length - 1],
          inputs: null,
          map,
          tas: null,
          tasDropped: 0,
          color: "",
          durationMs: Math.max(samples.times[samples.times.length - 1], recorded(replayMeta.time_ms) ?? 0),
          derived: null,
        };
        run.derived = deriveRun(run);
        runs.push(run);
      }
    }
    return runs;
  } finally {
    replay.free();
  }
}
