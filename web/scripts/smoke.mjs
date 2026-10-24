// Node smoke test: parse a real replay through the wasm module and the exact
// mapping code the browser uses, without a browser.
//
//   npm run wasm && npm run smoke
//
// Uses Node's built-in TypeScript type stripping to import `src/lib/parse.ts`
// (which is why the source imports carry explicit `.ts` extensions), so this
// checks the real names of the wasm getters and the metadata keys.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const wasmDir = join(root, "src/lib/wasm");
const fixtures = join(root, "..", "crates/trackmaniare/tests/fixtures");

const { initWasm, parseReplayBytes } = await import("../src/lib/parse.ts");

let failures = 0;

function check(name, condition, detail = "") {
  if (condition) {
    console.log(`  ok   ${name}`);
  } else {
    failures++;
    console.log(`  FAIL ${name}${detail ? ` (${detail})` : ""}`);
  }
}

const bytes = new Uint8Array(readFileSync(join(wasmDir, "trackmaniare_wasm_bg.wasm")));
await initWasm(bytes);

// --- tiny fixture: the file the Rust tests build scripted input from -------
{
  const data = readFileSync(join(fixtures, "tm2020-short.Replay.Gbx"));
  const runs = parseReplayBytes(new Uint8Array(data), "tm2020-short.Replay.Gbx");
  check("short: one run", runs.length === 1, `${runs.length}`);
  const run = runs[0];
  check("short: TAS script", run.tas === "0 steer 0\n0 accel 1\n0 brake 0\n", JSON.stringify(run.tas));
  check("short: dropped events counted", run.tasDropped === 7, `${run.tasDropped}`);
  check("short: input ticks", (run.inputs?.ticks ?? 0) > 0, `${run.inputs?.ticks}`);
  check("short: file name kept", run.fileName === "tm2020-short.Replay.Gbx");
  check("short: input events decoded", (run.inputs?.events.length ?? 0) > 3, `${run.inputs?.events.length}`);
}

// --- merged record fixture: samples, distance, inputs ---------------------
{
  const data = readFileSync(join(fixtures, "tm2020-merged-record.Replay.Gbx"));
  const runs = parseReplayBytes(new Uint8Array(data), "tm2020-merged-record.Replay.Gbx");
  check("merged: ghost plus replay-level record", runs.length === 2, `${runs.length}`);
  const ghostRun = runs[0];
  const recordRun = runs[1];
  check("merged: ghost has no samples", ghostRun.sampleCount === 0, `${ghostRun.sampleCount}`);
  check("merged: ghost input ticks", (ghostRun.inputs?.ticks ?? 0) > 0, `${ghostRun.inputs?.ticks}`);
  check("merged: TAS script has many lines", (ghostRun.tas?.split("\n").length ?? 0) > 100, `${ghostRun.tas?.split("\n").length}`);
  check("merged: record run keeps its own id", recordRun.id.endsWith("#merged"), recordRun.id);
  check("merged: record samples", recordRun.sampleCount > 100, `${recordRun.sampleCount}`);
  check("merged: record times ascend", ascending(recordRun.samples?.times ?? []) === true);
  check("merged: positions are finite", allFinite(recordRun.samples?.positions ?? []));
  check("merged: velocities are finite", allFinite(recordRun.samples?.velocities ?? []));
  check("merged: rotations are finite", allFinite(recordRun.samples?.rotations ?? []));
  check("merged: distance derived", (recordRun.derived?.totalDistance ?? 0) > 10, `${recordRun.derived?.totalDistance}`);
  check("merged: speed derived", (recordRun.derived?.topSpeedKmh ?? 0) > 1, `${recordRun.derived?.topSpeedKmh}`);
  check("merged: record has no inputs of its own", recordRun.inputs === null);
  check("merged: distance starts at zero", recordRun.derived?.distance[0] === 0, `${recordRun.derived?.distance[0]}`);

  const tasLines = (ghostRun.tas ?? "").trim().split("\n");
  const timePattern = /^-?\d+ (steer -?\d+|accel [01]|brake [01]|respawn|srespawn)$/;
  check(
    "merged: every TAS line matches the grammar",
    tasLines.every((line) => timePattern.test(line)),
    tasLines.find((line) => !timePattern.test(line)) ?? "",
  );
  const times = tasLines.map((line) => Number(line.split(" ")[0]));
  check("merged: TAS times ascend", times.every((t, i) => i === 0 || t >= times[i - 1]));
  console.log(
    `  info merged: ${recordRun.sampleCount} record samples, ${ghostRun.inputs?.ticks ?? 0} input ticks, ` +
      `${tasLines.length} TAS lines, ${(recordRun.derived?.totalDistance ?? 0).toFixed(1)} m`,
  );
}

function ascending(times) {
  for (let i = 1; i < times.length; i++) if (times[i] < times[i - 1]) return false;
  return true;
}

function allFinite(values) {
  for (let i = 0; i < values.length; i++) if (!Number.isFinite(values[i])) return false;
  return true;
}

if (failures) {
  console.error(`\n${failures} smoke check(s) failed`);
  process.exit(1);
}
console.log("\nsmoke ok");
