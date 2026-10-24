# TrackmaniaRE

Extracts everything a Trackmania 2020 `.Replay.Gbx` (or standalone `.Ghost.Gbx`)
contains into JSON and CSV: replay metadata, ghost records, the car sample stream
and the decoded player inputs.

Trackmania 2020 replays do not store raw key presses like the older games do.
They store a bit-packed, tick-based input stream (100 Hz) in the ghost's
`0x0309201D` chunk and a 20 Hz stream of car states (position, rotation, speed,
steering, pedals, RPM, gear, ground contact) in the ghost's record data. This
tool decodes both.

## AI disclosure

**This project is fully vibecoded, slop if you wish.**
This project started as a personal curiosity about TM2020's replay format and
devolved into building a full parser as an easy-to-use utility. All code, docs and
even the rest of the README were built by an AI agent.

This means:

- Expect bugs / incomplete features (I could only provide the AI with my own replays).
- **This repo is not actively maintained**, any issues or PRs might be stale before I check back on the repo.
- **[GBX.NET](https://github.com/BigBang1112/gbx-net) is the most mature library if you want to interact with `.gbx` files.**

## Build

```sh
cargo build --release        # binaries at target/release/TrackmaniaRE and trackmaniare-bench
cargo test                   # 48 tests, incl. end-to-end replay fixtures
```

The repository is a cargo workspace. `crates/trackmaniare` is the extractor. It
has one dependency, `miniz_oxide` (pure-Rust zlib, used for the ghost record
blobs), and decompresses the GameBox body with a built-in LZO1X decoder.
`crates/trackmaniare-wasm` holds the WebAssembly binding for the web frontend
and is a workspace member only: plain cargo commands build the extractor (the
default member), while `--workspace` reaches the bindings. Its shim smoke test
brings `cargo test --workspace` to 49 tests.

Build the browser package with:

```sh
wasm-pack build crates/trackmaniare-wasm --target web
```

The package lands in `crates/trackmaniare-wasm/pkg/` (~216 KB raw, 83 KB
gzipped) with typed-array timelines (`Float32Array` positions/speeds, per-tick
input arrays) and the embedded map as bytes. The deployed frontend builds
through `npm run wasm` in `web/` instead, which also remaps local source paths
out of the module.

## Usage

```sh
# human readable summary of one replay, or of every replay in a folder
TrackmaniaRE "replays/Some.Replay.Gbx"
TrackmaniaRE "replays"

# full JSON (including timelines) on stdout
TrackmaniaRE --json --timelines "replays/Some.Replay.Gbx"

# write per-replay output files: JSON, inputs.csv, samples.csv, inputs.dense.csv
# and the embedded map as a .Challenge.Gbx
TrackmaniaRE --out-dir extracted --dense --map "replays"

# everything, including the decompressed body and every chunk payload
TrackmaniaRE --raw --out-dir extracted "replays/Some.Replay.Gbx"

# a standalone ghost file extracts the same way as a replay
TrackmaniaRE --json "replays/some.Ghost.gbx"

# chunk inventory of a file: what exists, what was decoded, what was skipped
cargo run --release --example chunks -- "replays/Some.Replay.Gbx"

# raw vehicle samples next to their decoded values (format inspection)
cargo run --release --example sample -- "replays/Some.Replay.Gbx"
```

| Option | Meaning |
| --- | --- |
| `-s, --summary` | human readable summary (default) |
| `-j, --json` | JSON instead of the summary |
| `--info` | container info and chunk inventory only |
| `-o, --out-dir DIR` | write per-replay files into DIR |
| `-t, --timelines` | include full input/sample timelines in the JSON |
| `-d, --dense` | also write a per-tick (10 ms) input timeline |
| `-m, --map` | write the embedded map as `<name>.Challenge.Gbx` |
| `-T, --tas` | write a TICK input script (`.tas.txt`) per ghost with TM2020 inputs |
| `-r, --raw` | also dump the decompressed body and all chunk payloads |
| `-q, --quiet` | suppress the per-file separator |
| `--` | treat every remaining argument as a path |

## Bench harness

`trackmaniare-bench` parses a whole collection and reports what failed, what
crashed and how fast it went. It scans directories recursively, so it finds
replays the main tool's top-level scan misses, and runs every parse under
`catch_unwind`: a replay that panics the parser gets written up and the run
continues. It exits non-zero when any file could not be read, could not be
parsed, panicked, or parsed only partially (the parser stopped inside the file),
so it works as a check in scripts.

```sh
# status + timing report over a collection
cargo run --release --bin trackmaniare-bench -- "replays"

# machine readable, with three parses per file for steadier numbers
cargo run --release --bin trackmaniare-bench -- -r 3 --json report.json --csv results.csv "replays"

# Windows binary (host needs the mingw-w64 toolchain, or build on Windows itself)
cargo build --release --target x86_64-pc-windows-gnu --bin trackmaniare-bench
# -> target/x86_64-pc-windows-gnu/release/trackmaniare-bench.exe
```

| Option | Meaning |
| --- | --- |
| `-r, --repeat N` | parse every file N times, report per-file parse medians (default 1) |
| `-j, --json FILE` | also write the full report as JSON |
| `-c, --csv FILE` | also write per-file results as CSV |
| `-v, --verbose` | print one line per file while the run proceeds |
| `-q, --quiet` | do not print failures as they are found |
| `--simulate-panic N` | panic while parsing the Nth file (self-test of the crash handling) |

The report carries counts by outcome (`ok`, warnings, errors, panics,
unreadable, and `partial` for files where parsing stopped early), input and
sample totals (including files whose input stream stores fewer ticks than its
archive declares; see Limitations), recorded race time, chunk dispositions,
read/parse/wall time, per-file parse latency percentiles, throughput and the
slowest replays. Two speeds are printed: end to end (file bytes over wall time,
reading included) and parse (file bytes over the sum of per-file parse medians,
reading excluded). A short input tail gets its own `note` line and never counts
as a failure.

## What is extracted

**Replay record.** Game version, exe build, title, map uid/name/author/author
zone, environment, mood, map type/style, display cost, player model, best time,
respawns, stunt score, validable, checkpoints, the raw community XML, the author
block, the map embedded in the replay (`0x03093002`), and, in merge/preview
exports, the combined record data (`0x0911F000`).

**Ghost.** Nickname, login, ghost uid, club tag, trigram, zone, player model
(id/collection/author), light trail colour, skin pack descriptors, race time,
respawns, stunt score, checkpoint times, ghost version, validation block (exe
version, exe checksum, OS/CPU kind, race settings XML, validation seed, title id
and checksum), wall-clock start/end, and the record data with all vehicle
samples.

**Inputs.** The TM2020 bit-packed stream is decoded into discrete events
(`time_ms`, `tick`, type, value): steer (−127…127), accelerate, brake, horn,
respawn, secondary respawn, action slots, jump, free look, camera 2, and, for
CharacterPilot runs, strafe/walk/vertical/horizontal plus mouse position.
Pre-TM2020 control-entry inputs (TMU/TMNF/TMUF/TM2 replays) are read as well.
`--tas` (and the web frontend's TAS panel) turns the decoded events into a TICK
input script built from verified keywords only: `seed`, `steer`, `accel`,
`brake`, `respawn`, `srespawn`. The script opens with `0 seed <value>` from the
recording's validation block, starts from the neutral state (steer 0, pedals
released, so a run that begins there writes nothing yet), and writes same-tick
pedal lines before steering, the way TICK's own exports read. Held respawn bits
collapse into one line, and events that have no verified keyword are left out
and counted.

**Vehicle samples.** 20 Hz states with position, rotation (angle, axis heading,
axis pitch), speed, velocity, steering, gas, brake, RPM, gear, turbo time,
suspension travel, per-wheel ground material, dirt/wetness and vehicle state.
Record data is a set of entity lists (car, items, objects). The car gets a new
list at every checkpoint, so a finished run has several; all of them are decoded
and the web frontend joins the car lists in time order. Some exports also carry
a sparse second car track over the same span (Trackmania.io downloads do); those
overlap the real run instead of continuing it, and the web frontend keeps the
denser recording. Raw extraction keeps every car state, including duplicates,
while item and object samples are reported separately as `other tracks`. Record
data written at replay level by merge/preview tooling is decoded the same way
(JSON: `replay.record_data`; `samples.csv` rows use ghost `-1`).

Nothing is thrown away silently: undecoded or deliberately skipped chunks are
listed in the `chunks` inventory (`parsed` / `skipped` / `unknown`) and anything
odd shows up in `warnings`.

## Output files

| File | Contents |
| --- | --- |
| `<stem>.json` | everything, including timelines when `-t` is given |
| `<stem>.inputs.csv` | one row per input change |
| `<stem>.inputs.dense.csv` | one row per 10 ms tick with the state in effect |
| `<stem>.samples.csv` | one row per car sample (ghost `-1` = replay-level record) |
| `<stem>.tas.txt` | TICK input script with `-T` (`.ghostN.tas.txt` for further ghosts) |
| `<stem>.<map name>.Challenge.Gbx` | the map embedded in the replay |
| `<stem>.body.bin` + `<stem>.chunks/` | with `--raw`: decompressed body and raw chunk payloads |

## Web frontend

`web/` is a static site that runs the extractor in the browser through
WebAssembly. Drop a replay on it and everything is parsed on your device: the
build has no network calls, no CDNs and no external fonts.

The app has a fixed shell: a compact header, a runs rail, a tabbed workspace and
a playback transport pinned to the bottom edge. The rail and transport stay
reachable on every tab, and files can still be dropped anywhere on the page.
Below 900 px the rail becomes a drawer opened from the header; the header's Open
files button is icon-only on phones, with an accessible name and tooltip.

When a file loads, the app selects the best run automatically: vehicle samples
first, then a finish time, then an input stream. A merged replay with a
replay-level record therefore opens on the record, not on the first ghost. The
rail groups runs by source file, lets you close a whole file and gives every run
that is not selected a Compare checkbox. A replay-level record is labelled
`nickname · record` so it is not mistaken for a second ghost.

Comparisons fill a bar above the workspace. The first added run is the reference
for speed, delta and split comparisons; any entry can become the reference with
its radio button, be removed with its close button, or be cleared as a set.
Selecting a run that is already compared swaps it into that comparison slot.
Warnings such as `different maps` stay visible in the bar and in the affected
panel.

The workspace has three tabs:

- Overview: the run's details in labelled groups, the split table (side by side
  when there is room), and the embedded map download.
- 3D path: the path of the recorded positions with a schematic car marker, icon
  controls (fit, follow, fullscreen), an orbit camera that can follow the car,
  and a fullscreen mode with a video-style playback bar. The live input display
  sits beside the canvas when there is room and below it on narrow screens.
- Inputs: the input piano roll, the speed and delta charts, and the TAS panel.

The active tab is remembered in `localStorage`.

What the panels do:

- Runs rail: the drop target, file groups, the selected run, per-run Compare
  toggles, parse warnings and recent files.
- Overview: distance, speed, airborne time and the recorded facts, the map
  download, and a split table with finish and checkpoint deltas against the
  reference.
- Live input display: the arrow keys fill with the current input state, while the
  gear and speed follow the car samples, and the bar is scaled to the run's top
  speed. The exact steering strength is shown as a signed share of full lock.
  It shows the 100 Hz stream, which begins during the pre-start hold, so the
  first second or so of it sits before the run timer starts. If the run has no
  vehicle samples, it says which inputs are still available.
- Input piano roll of the 100 Hz stream: steering waveform, gas and brake, with
  respawns and guessed wall hits marked. The readout under it follows the
  playhead. On a mouse, drag to scrub, wheel to zoom, Shift-drag to pan and
  double-click to fit; on a coarse pointer, drag to seek and use Fit or ± to
  zoom. The shaded stretch on the left is the pre-start hold, where the input
  stream runs before the run timer starts.
- TAS panel with the TICK script, ready to copy or download.
- Playback transport (0.1x to 4x) that drives the live display, the piano roll,
  the charts and the car marker. It names the selected run and shows its finish
  time plus its sample or tick count.
- Speed chart for the selected run and every compared run, a delta chart at
  equal distance against the reference, and the split table described above. A
  run's checkpoint list ends with the finish line, so the last entry is shown as
  the finish instead of being listed twice.
- 3D path of the recorded positions for the selected run and compared runs.
  The selected run's path is coloured by speed, steering, pedals or grip;
  compared runs stay dimmed in their run colour. Map geometry is not rendered,
  only the path, a grid at the lowest point and a schematic car marker at its
  real size (4.0 m × 1.9 m). A small dot above the car marks the playhead when a
  fitted whole-track view makes the car itself too small to see. The grip mode
  colours each sample green while the car grips, amber while it slides and blue
  while it is airborne (see Limitations). Speed colouring adapts to the visible
  runs: the 2nd–98th percentile speed range, rounded outwards to 10 km/h, is
  mapped to the full ramp so a fast map does not collapse into one red line.
  The 3D legend shows that km/h range. Values outside it clamp to the endpoint
  colours. The legend names the selected run and the reference, and omits
  compared runs that have no samples.
- Recent runs stay in IndexedDB (10 runs / 100 MB, clearable), so a reload does
  not need the file again.

Keyboard: `Space` toggles playback, the arrow keys step 0.1 s, Shift plus the
arrow keys step 1 s, `Home` and `End` jump to the recorded ends, `F` opens the
3D fullscreen player when a path is available, `?` opens the shortcut list and
`Esc` closes the dialog, the runs drawer or fullscreen. Shortcuts are ignored
while a text field or other control has keyboard focus; the piano roll owns its
arrow and zoom keys.

Load and comparison status appears in dismissible toasts that hide on their own
and stay clear of the tabs and transport. Parse warnings stay attached to the
run instead.

Building the site needs Node 22.22 or newer; CI uses 24. If a command dies with
a `SyntaxError` from inside `node_modules`, an older Node is on your PATH, so
check `node -v` first.

```sh
cd web
npm install
npm run wasm     # build crates/trackmaniare-wasm into src/lib/wasm/ (gitignored)
npm run dev      # http://localhost:5173
npm run check    # type check only
npm test         # unit + jsdom integration tests
npm run smoke    # parses the Rust fixtures in Node through the wasm build
npm run build    # type check + production build in web/dist/
npm run shots    # Playwright screenshot pass into web/.shots/ (gitignored)
```

`npm run wasm` runs through `web/scripts/wasm-build.mjs`, which remaps Rust
source paths so the published `.wasm` does not contain the builder's home
directory.

`npm run build` writes a fully self-contained `web/dist/`: static files plus one
`.wasm`. Serve it over HTTP (ES modules and `fetch` do not work from `file://`).
`.github/workflows/pages.yml` builds and publishes it to GitHub Pages on every
push to `main`; set the repository's Pages source to "GitHub Actions" once.

`npm run shots` starts its own Vite server, loads the Rust fixtures through the
real file input and writes viewport-sized screenshots for 1440, 1024 and 390 px.
It needs Playwright's Chromium; on a machine without the
download, point `SHOTS_CHROMIUM` at a Chromium binary. `SHOTS_BASE_URL` reuses a
running dev server and `SHOTS_OUT` changes the output directory. The screenshot
workflow and the design rules are in `web/DESIGN.md`.

## How it works

A GameBox is a chunked archive: a header with class id, user data and a chunk
table, a reference table, and a body compressed with LZO1X. The body holds the
replay record (class `0x03093000`), which references ghost nodes (`0x03092000`).
The ghost nodes carry the input stream in chunk `0x0309201D` and reference the
`CPlugEntRecordData` (`0x0911F000`) that holds the car samples. Merge/preview
tooling writes one combined `0x0911F000` directly in the replay record instead;
the parser reads it there and continues the chunk walk after it. Chunks marked
"skippable" declare their size, which is what makes a lenient parser possible:
if a chunk cannot be interpreted, its payload is stepped over instead of
aborting the whole node.

Format references used while writing this:

* [Mania Tech Wiki: GBX](https://wiki.xaseco.org/wiki/GBX): container, chunk
  framing, lookback strings, primitives.
* [GBX.NET](https://github.com/BigBang1112/gbx-net): class/chunk catalogues
  (`.chunkl` files), the `CPlugEntRecordData` layout and the TM2020 input bit
  layout; see its wiki page
  [Extracting input data from ghosts](https://github.com/BigBang1112/gbx-net/wiki/Extracting-input-data-from-ghosts).
* [pygbx](https://pypi.org/project/pygbx/): cross-check for chunk walking.

The LZO decoder was validated byte-for-byte against the reference LZO
implementation. The input decoder is cross-checked against the vehicle samples,
and the decoded input steering agrees with the recorded wheel steering on every
run.

## Measured on a collection

Run over a 692 replay collection (634 MB, game builds from 2021 to 2026), plus
one standalone `.Ghost.gbx` that the directory scan now picks up as well:

| Check | Result |
| --- | --- |
| Replays parsed | 693 / 693, no errors, no truncated bodies |
| Replays with a decoded input stream | 693 / 693 (530,461 input events) |
| Replays with car samples | 692 / 693 (602,050 car states) |
| Decoded speed vs. distance covered between samples | median deviation 0.2%, 90th percentile 2.7% (respawn jumps are the only large outliers); asserted within tolerance in `cargo test` |
| Warnings | 36 total: 16 auxiliary (media-clip) nodes, 13 chunk resyncs, 6 short input tails, 1 other |

The single replay without car samples is an unfinished run (no race time) that
only carries inputs. Input steering versus recorded wheel steering is asserted
per sample in `cargo test` and holds for the fixture with a worst-case deviation
of 0.0039 of full lock, exactly one steer quantisation step (1/255); the test
bound is 0.01.

Running `--tas` over the same collection writes a TICK script for all 693 ghosts,
525,979 lines in total. Every line matches the format, times never go backwards,
steering stays inside −127…127, and 675 respawns plus one secondary respawn come
through. 3,906 events (0.7% of the decoded input events: horn, action slots,
jump, free look, camera, mouse) are dropped because they have no verified TICK
keyword. The web UI reports that count per ghost.

## Limitations

* Directory scans are **non-recursive** and match `.Replay.Gbx` / `.Ghost.Gbx`
  case-insensitively: replays that live in subdirectories are not found, so
  passing a collection that contains them undercounts (the measurement above
  therefore covers the 692 replays in the top level only).
* Some 2020/2021 recordings declare a few more input ticks than the stream
  stores: the old game builds wrote the trailing ticks short (up to ~150 ms,
  at the very end of the recording, usually after the finish). The stored
  ticks decode completely and agree with the recorded vehicle steering; the
  difference is reported as
  `input stream ended early (N of M ticks stored, archive version 12)` and
  never counts as a partial parse.
* Inputs bound to the mouse are known to come out corrupted upstream of us.
* Input states that the GBX.NET reference rejects (`float-based` input kinds
  0xC/0xD, corrupted strafe/walk values) stop decoding at that tick and are
  reported as warnings instead of being emitted as plausible-looking events.
* Cinematic replays reference media clip nodes, which are stepped over rather
  than decoded.
* This is a reader: it never writes `.Replay.Gbx` files.

Browser frontend, on top of the parser's own limits:

* The TAS script writes the verified control set only (`seed`, `steer`,
  `accel`, `brake`, `respawn`, `srespawn`). Events with no verified TICK keyword
  (horn, action slots, jump, free look, camera, mouse) are dropped and counted
  in the UI. `flags` words are never written.
* The input stream starts when the recording does, typically around −1500 ms
  during the pre-start hold, so the live display and the piano roll show a short
  negative stretch that the transport itself cannot reach (it starts at 0).
* Wall-hit markers are a guess: a horizontal speed loss of 5 m/s or more in one
  20 ms sample that is not a straight-line brake. Hard landings and kerb strikes
  can qualify, so the UI labels them approximate.
* The delta chart compares time at equal distance. That holds while both runs
  follow the same route; after a respawn or a shortcut the two lines diverge and
  the numbers stop meaning anything. The panel says so but still draws them.
* Replays that store their record at replay level (merge and preview tooling)
  show up as an extra run labelled `nickname · record`, because the file never
  says which ghost the merged samples belong to.
* Parsing runs on the main thread. A two-hour replay (~150k samples) blocks the
  tab while it decodes.
* The 3D path view needs WebGL. Every other panel still works without it.
* The car in the 3D view is a schematic: 4.0 m × 1.9 m, boxes, wheels and
  headlights, not the game model. At that size it is only a few pixels wide in a
  fitted whole-track view, which is what the dot above it is for.
* The grip colouring is a heuristic: the car is sliding when its velocity
  points more than 15° away from its heading, and airborne when the ground mode
  says so. Below 3 m/s the heading is meaningless and counts as grip, and a
  reverse gear or a spin looks the same to it as a slide. The threshold is a
  judgement call: across 30 sampled replays 3% of grounded, moving samples sit
  above 15° (1% above 20°), and drift maps sit highest.
* Recent runs live in IndexedDB, capped at 10 runs / 100 MB, with a clear
  button. They stay in the browser.

## Repository layout

```
crates/trackmaniare/src/lzo.rs       LZO1X decompressor
crates/trackmaniare/src/reader.rs    binary reader, lookback strings ("Id") and identifiers
crates/trackmaniare/src/gbx.rs       GameBox container and chunk framing
crates/trackmaniare/src/replay.rs    replay record node
crates/trackmaniare/src/ghost.rs     ghost node, incl. validation and input chunks
crates/trackmaniare/src/record.rs    CPlugEntRecordData and vehicle sample decoding
crates/trackmaniare/src/inputs.rs    TM2020 bit-packed inputs, legacy control entries
crates/trackmaniare/src/tas.rs       TICK input script writer (verified keywords only)
crates/trackmaniare/src/model.rs     output model and JSON rendering
crates/trackmaniare/src/tabular.rs   CSV rendering and the text summary
crates/trackmaniare/src/main.rs      CLI
crates/trackmaniare/src/bin/         bench harness (status, crashes, timings)
crates/trackmaniare/tests/           LZO vectors, input decoding, end-to-end replay fixture,
                                     hostile-input regressions, CLI behaviour
crates/trackmaniare/examples/        chunks.rs (chunk inventory), sample.rs (raw sample inspection)
crates/trackmaniare-wasm/src/lib.rs  wasm-bindgen API: metadata JSON, typed-array timelines,
                                     map bytes, CSV exports, TAS script
web/index.html                       fixed shell: header, runs rail/drawer, tabs, panels
web/src/main.ts                      load/drop, playback loop, tabs, drawer, toasts, keys
web/src/app.ts                       state store, selection and comparison helpers
web/src/lib/parse.ts                 wasm facade: replay bytes in, plain JS runs out
web/src/lib/derive.ts                distance, speed, deltas and splits (unit tested)
web/src/panels/                      runs, details, TAS, transport, live inputs, piano roll,
                                     charts, splits, 3D path
web/src/styles/                      tokens, base, components, shell and panel CSS
web/tests/                           vitest unit tests and a jsdom integration test
web/scripts/smoke.mjs                Node smoke test over the Rust replay fixtures
web/scripts/wasm-build.mjs           wasm-pack wrapper with local paths remapped
web/scripts/shots.mjs                Playwright screenshot pass (`npm run shots`)
web/DESIGN.md                        UI standard and screenshot review checklist
```
