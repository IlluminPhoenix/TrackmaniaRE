#!/usr/bin/env node
// Build the wasm package used by the web frontend.
//
// Rust embeds source paths in panic locations. Those paths include the
// builder's home directory and the cargo registry, so remap them before
// wasm-pack runs. The published `.wasm` must not name the machine that built
// it.

import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(webRoot, "..");
const crateRoot = resolve(repoRoot, "crates/trackmaniare-wasm");
// wasm-pack resolves --out-dir against the crate directory, not the cwd.
const outDir = process.env.WASM_OUT_DIR ?? "../../web/src/lib/wasm";

const remaps = [
  `--remap-path-prefix=${homedir()}=/build`,
  `--remap-path-prefix=${repoRoot}=/workspace`,
];
const rustflags = [process.env.RUSTFLAGS, ...remaps].filter(Boolean).join(" ");

const wasmPack = process.platform === "win32" ? "wasm-pack.cmd" : "wasm-pack";
const args = [
  "build",
  "../crates/trackmaniare-wasm",
  "--target",
  "web",
  "--out-dir",
  outDir,
];

const child = spawn(wasmPack, args, {
  cwd: webRoot,
  env: { ...process.env, RUSTFLAGS: rustflags },
  stdio: "inherit",
});

child.on("error", (error) => {
  console.error(`Could not run ${wasmPack}: ${error.message}`);
  process.exitCode = 1;
});

child.on("exit", async (code, signal) => {
  if (signal) {
    console.error(`${wasmPack} stopped with ${signal}`);
    process.exitCode = 1;
    return;
  }
  if (code !== 0) {
    process.exitCode = code ?? 1;
    return;
  }

  const wasmPath = resolve(crateRoot, outDir, "trackmaniare_wasm_bg.wasm");
  try {
    const bytes = await readFile(wasmPath);
    const leaks = [homedir(), repoRoot].filter(
      (prefix) => prefix.length > 1 && bytes.includes(`${prefix}/`),
    );
    if (leaks.length) {
      console.error(`Build still contains a local path (${leaks.join(", ")}) in ${wasmPath}`);
      process.exitCode = 1;
      return;
    }
  } catch (error) {
    console.error(`Could not check ${wasmPath}: ${error.message}`);
    process.exitCode = 1;
    return;
  }
  process.exitCode = 0;
});
