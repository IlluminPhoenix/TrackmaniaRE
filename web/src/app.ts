//! Application wiring shared by `main.ts` and the panels.

import type { Store } from "./lib/store";
import type { RunData } from "./lib/types";

export interface AppState {
  runs: RunData[];
  selectedId: string | null;
  /** Runs drawn next to the selection. The first entry is the reference. */
  compareIds: string[];
  /** Bumped when IndexedDB gained a recent run, so the runs panel reloads its list. */
  recentsVersion: number;
  status: string;
  error: string | null;
}

export interface ClockState {
  timeMs: number;
  playing: boolean;
  rate: number;
}

export interface Actions {
  loadFiles(files: FileList | File[]): Promise<void>;
  /** Parses bytes that are already in memory (IndexedDB restore or drop). */
  loadBytes(fileName: string, bytes: Uint8Array, options?: { persist?: boolean }): Promise<void>;
  select(id: string): void;
  toggleCompare(id: string): void;
  /** Removes every comparison run, leaving the selected run alone. */
  clearCompare(): void;
  /** Moves a comparison run to the front, making it the delta/split reference. */
  makeReference(id: string): void;
  removeRun(id: string): void;
  /** Closes every run that came from one file. */
  removeFile(fileName: string): void;
  seek(ms: number): void;
  nudge(deltaMs: number): void;
  setPlaying(playing: boolean): void;
  togglePlay(): void;
  setRate(rate: number): void;
  setStatus(status: string, error?: string | null): void;
}

export interface App {
  state: Store<AppState>;
  clock: Store<ClockState>;
  actions: Actions;
}

/** Chart colours, in assignment order. */
export const PALETTE = [
  "#4cc9f0",
  "#f72585",
  "#f9c74f",
  "#43aa8b",
  "#b5179e",
  "#4d908e",
  "#f8961e",
  "#577590",
  "#90be6d",
  "#e5989b",
];

export function colorFor(index: number): string {
  return PALETTE[index % PALETTE.length];
}

/**
 * `count` colours no existing run uses yet. Falls back to reusing palette
 * entries when there are more runs than colours.
 */
export function assignColors(existing: RunData[], count: number): string[] {
  const used = new Set(existing.map((run) => run.color));
  const out: string[] = [];
  let index = 0;
  while (out.length < count && index < PALETTE.length * 2) {
    const color = colorFor(index++);
    if (used.has(color)) continue;
    used.add(color);
    out.push(color);
  }
  while (out.length < count) out.push(colorFor(index++));
  return out;
}

export function selectedRun(app: App): RunData | null {
  const { runs, selectedId } = app.state.get();
  return runs.find((r) => r.id === selectedId) ?? null;
}

export function compareRuns(app: App): RunData[] {
  const { runs, compareIds } = app.state.get();
  return compareIds.map((id) => runs.find((r) => r.id === id)).filter((r): r is RunData => Boolean(r));
}

/** First comparison run: the reference for deltas and splits. */
export function referenceRun(app: App): RunData | null {
  const selected = selectedRun(app);
  return compareRuns(app).find((run) => run.id !== selected?.id) ?? null;
}

/** All runs shown in the 3D view: the selection first, then the references. */
export function visibleRuns(app: App): { run: RunData; isReference: boolean }[] {
  const selected = selectedRun(app);
  const refs = compareRuns(app).filter((r) => r.id !== selected?.id);
  return [
    ...(selected ? [{ run: selected, isReference: false }] : []),
    ...refs.map((run) => ({ run, isReference: true })),
  ];
}

/**
 * The run worth selecting after a load. Prefer one that can drive every panel:
 * samples and a finish time, then an input stream, then a recorded finish.
 */
export function bestRun(runs: RunData[]): RunData | null {
  let best: RunData | null = null;
  let bestScore = -1;
  for (const run of runs) {
    const score =
      (run.samples ? 4 : 0) +
      (run.raceTimeMs !== null ? 2 : 0) +
      (run.inputs ? 1 : 0) -
      Math.min(run.ghostIndex, 5) * 0.01;
    if (score > bestScore) {
      best = run;
      bestScore = score;
    }
  }
  return best;
}

/**
 * Name used in lists, the transport and the compare bar. A replay-level record
 * is not a ghost, so it must not inherit a ghost-number suffix.
 */
export function runDisplayName(run: RunData): string {
  if (run.isReplayRecord) {
    return run.nickname === "(merged record)" ? run.nickname : `${run.nickname} · record`;
  }
  return run.ghostIndex > 0 ? `${run.nickname} · ghost ${run.ghostIndex + 1}` : run.nickname;
}

/** Name plus its role in the current view: selected, reference or compare N. */
export function runRoleLabel(run: RunData, app: App): string {
  const { selectedId, compareIds } = app.state.get();
  const name = runDisplayName(run);
  if (run.id === selectedId) return name;
  const index = compareIds.indexOf(run.id);
  if (index === 0) return `${name} (reference)`;
  if (index > 0) return `${name} (compare ${index + 1})`;
  return name;
}

export function runLabel(run: RunData): string {
  const time = run.raceTimeMs !== null ? ` – ${formatRaceTime(run.raceTimeMs)}` : "";
  return `${run.nickname}${time}`;
}

function formatRaceTime(ms: number): string {
  const total = Math.round(ms);
  const m = Math.floor(total / 60_000);
  const s = Math.floor((total % 60_000) / 1000);
  const millis = total % 1000;
  return `${m}:${String(s).padStart(2, "0")}.${String(millis).padStart(3, "0")}`;
}
