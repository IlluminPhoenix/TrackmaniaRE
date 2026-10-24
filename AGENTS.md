# AGENTS.md

Rust workspace (stable Rust, edition 2024) for a Trackmania 2020 `.Replay.Gbx` and
`.Ghost.Gbx` extractor: a CLI, wasm-bindgen bindings and the static browser
frontend in `web/`.
`README.md` is the source of truth for behaviour, extraction scope and limitations;
keep it accurate when behaviour changes. Write human-facing docs with the `unslop`
skill when it is installed.

## Layout

- `crates/trackmaniare/` - parser, `TrackmaniaRE` CLI and `trackmaniare-bench` (default workspace member).
- `crates/trackmaniare-wasm/` - wasm bindings for the web frontend, built with wasm-pack. `wasm-bindgen` is pinned exactly, so the crate and the wasm-pack CLI move together.
- `web/` - the frontend: vanilla TypeScript + Vite, no UI framework. Source imports carry explicit `.ts` extensions where Node's type stripping has to resolve them (the smoke test imports `src/lib/parse.ts` directly). The wasm build lands in the gitignored `web/src/lib/wasm/`.

## Building the UI

Source review and passing tests do not prove the UI works. For any frontend change,
render it, drive it, and look at the screenshots before you report the change.

A repeatable screenshot script belongs at `web/scripts/shots.mjs` with an
`npm run shots` entry. If it is missing, add it as part of the change instead of
skipping the visual pass. Until then, use Playwright or an equivalent headless
browser against `npm run dev`, or `npm run build && npm run preview`.

- Load `crates/trackmaniare/tests/fixtures/tm2020-short.Replay.Gbx` and
  `tm2020-merged-record.Replay.Gbx` through the real file input. The merged
  fixture matters because its first ghost has no vehicle samples, which is where
  selection and empty states break.
- Capture the empty state, a single sampled run, the merged replay with each run
  selected, a comparison, and every tab. Do this at 1440, 1024 and 390 px wide.
- Run the captures in the background. Do not ask the user for screenshots or
  review of intermediate states.
- Open every image and inspect it. Check for missing glyphs, clipped labels,
  horizontal overflow, blank or oversized empty panels, controls that scroll out
  of reach, and the 3D canvas rendering when WebGL is available.
- Exercise the same states with the keyboard: open a file, select a run, set the
  reference, switch tabs and control playback. A control that needs a mouse is a
  bug.
- Check the browser console for errors during each pass.

Keep generated screenshots out of git unless a doc needs them. Fix what you find,
rerun the matrix, and only then report the work. Visual checks supplement the
checks in "Before committing"; they do not replace them. Update `README.md` when
the layout or interaction model changes.

## Before committing

CI (`.github/workflows/ci.yml`) gates fmt, clippy with `-D warnings`, the workspace
tests, the wasm build, and the web checks. In `web/` that means `npm run wasm`,
`npm test`, `npm run smoke` and `npm run build`. Tests run
on the small fixtures in `crates/trackmaniare/tests/fixtures/` and must not depend on
a local replay collection. For parser changes, run the bench harness over a replay
folder when one is available (`cargo run --release --bin trackmaniare-bench -- <replay-dir>`,
non-zero exit on any failure).

## Invariants

- Parse failures are warnings, never fatal; skippable chunks are stepped over so a hostile or odd file cannot abort a run.
- The TAS writer (`crates/trackmaniare/src/tas.rs`) emits verified TICK keywords only (`seed`, `steer`, `accel`, `brake`, `respawn`, `srespawn`) and counts what it drops. `seed` opens the script as `0 seed <validation_seed>`, verified against TICK-exported input documents. Do not invent keywords for horn, actions or `flags`: a `flags` word is opaque to TICK's parser (any 32-bit value is accepted and nothing was mapped), and the other names were never verified.
- The README Limitations section lists known quirks (mouse inputs, short input tails, non-recursive scans); read it before changing behaviour around one.
- The README `## AI disclosure` section is human-written and agents may edit it, but keep its blunt tone: it deliberately turns away AI-hostile readers.
