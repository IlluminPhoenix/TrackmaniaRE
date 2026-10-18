# TrackmaniaRE

Extracts everything a Trackmania 2020 `.Replay.Gbx` (or standalone `.Ghost.Gbx`)
contains — replay metadata, ghost records, the car sample stream and the decoded
player inputs — into JSON and CSV.

Trackmania 2020 replays do not store the raw key presses of older games, but they
do store a bit-packed, tick-based input stream (100 Hz) in the ghost's
`0x0309201D` chunk, plus a 20 Hz stream of car states (position, rotation, speed,
steering, pedals, RPM, gear, ground contact) in the ghost's record data. Both are
decoded here.

## AI disclosure

Much of this repository — the extractor, its tests, the WASM bindings and this
documentation — was written with AI coding assistants under human direction
and review. Format claims are cross-checked against the GBX.NET reference,
and behaviour changes land with tests.

## Build

```sh
cargo build --release        # binaries at target/release/TrackmaniaRE and trackmaniare-bench
cargo test                   # 39 tests, incl. end-to-end replay fixtures
```

The repository is a cargo workspace. `crates/trackmaniare` is the extractor —
one dependency, `miniz_oxide` (pure-Rust zlib, used for the ghost record
blobs); the GameBox body is decompressed with a built-in LZO1X decoder.
`crates/trackmaniare-wasm` is the WebAssembly binding used by the web frontend
and is a workspace member only: plain cargo commands build the extractor (the
default member), while `--workspace` reaches the bindings, whose shim smoke
test brings `cargo test --workspace` to 40 tests.

A browser package is built with:

```sh
wasm-pack build crates/trackmaniare-wasm --target web
```

which produces `crates/trackmaniare-wasm/pkg/` (~214 KB raw, 84 KB gzipped)
with typed-array timelines (`Float32Array` positions/speeds, per-tick input
arrays) and the embedded map as bytes.

## Usage

```sh
# human readable summary of one replay, or of every replay in a folder
TrackmaniaRE "My Replays/Some.Replay.Gbx"
TrackmaniaRE "My Replays"

# full JSON (including timelines) on stdout
TrackmaniaRE --json --timelines "My Replays/Some.Replay.Gbx"

# write per-replay output files: JSON, inputs.csv, samples.csv, inputs.dense.csv
# and the embedded map as a .Challenge.Gbx
TrackmaniaRE --out-dir extracted --dense --map "My Replays"

# everything, including the decompressed body and every chunk payload
TrackmaniaRE --raw --out-dir extracted "My Replays/Some.Replay.Gbx"

# a standalone ghost file extracts the same way as a replay
TrackmaniaRE --json "My Replays/some.Ghost.gbx"

# chunk inventory of a file: what exists, what was decoded, what was skipped
cargo run --release --example chunks -- "My Replays/Some.Replay.Gbx"

# raw vehicle samples next to their decoded values (format inspection)
cargo run --release --example sample -- "My Replays/Some.Replay.Gbx"
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
| `-r, --raw` | also dump the decompressed body and all chunk payloads |
| `-q, --quiet` | suppress the per-file separator |
| `--` | treat every remaining argument as a path |

## Bench harness

`trackmaniare-bench` parses a whole collection and reports what failed, what
crashed and how fast it went. It scans directories recursively (including
replays the main tool's top-level scan misses), runs every parse under
`catch_unwind` — a replay that panics the parser is written up and the run
continues — and exits non-zero when any file could not be read, could not be
parsed, panicked, or parsed only partially (the parser stopped inside the
file), so it works as a check in scripts.

```sh
# status + timing report over a collection
cargo run --release --bin trackmaniare-bench -- "My Replays"

# machine readable, with three parses per file for steadier numbers
cargo run --release --bin trackmaniare-bench -- -r 3 --json report.json --csv results.csv "My Replays"

# Windows binary (host needs the mingw-w64 toolchain, or build on Windows itself)
cargo build --release --target x86_64-pc-windows-gnu --bin trackmaniare-bench
# -> target/x86_64-pc-windows-gnu/release/trackmaniare-bench.exe
```

| Option | Meaning |
| --- | --- |
| `-r, --repeat N` | parse every file N times, report per-file medians (default 1) |
| `-j, --json FILE` | also write the full report as JSON |
| `-c, --csv FILE` | also write per-file results as CSV |
| `-v, --verbose` | print one line per file while the run proceeds |
| `-q, --quiet` | do not print failures as they are found |
| `--simulate-panic N` | panic while parsing the Nth file (self-test of the crash handling) |

The report carries counts by outcome (`ok`, warnings, errors, panics,
unreadable, and `partial` for files where parsing stopped early), input and
sample totals (including files whose input stream stores fewer ticks than its
archive declares — see Limitations), recorded race time, chunk dispositions,
read/parse/wall time, per-file latency percentiles, throughput and the
slowest replays. A short input tail gets its own `note` line and never counts
as a failure.

## What is extracted

**Replay record** — game version, exe build, title, map uid/name/author/author
zone, environment, mood, map type/style, display cost, player model, best time,
respawns, stunt score, validable, checkpoints, the raw community XML, the
author block, the map embedded in the replay (`0x03093002`), and — in
merge/preview exports — the combined record data (`0x0911F000`).

**Ghost** — nickname, login, ghost uid, club tag, trigram, zone, player model
(id/collection/author), light trail colour, skin pack descriptors, race time,
respawns, stunt score, checkpoint times, ghost version, validation block (exe
version, exe checksum, OS/CPU kind, race settings XML, validation seed, title id
and checksum), wall-clock start/end, and the record data with all vehicle
samples.

**Inputs** — the TM2020 bit-packed stream is decoded into discrete events
(`time_ms`, `tick`, type, value): steer (−127…127), accelerate, brake, horn,
respawn, secondary respawn, action slots, jump, free look, camera 2, and — for
CharacterPilot runs — strafe/walk/vertical/horizontal plus mouse position.
Pre-TM2020 control-entry inputs (TMU/TMNF/TMUF/TM2 replays) are read as well.

**Vehicle samples** — 20 Hz states with position, rotation (angle, axis heading,
axis pitch), speed, velocity, steering, gas, brake, RPM, gear, turbo time,
suspension travel, per-wheel ground material, dirt/wetness and vehicle state.
The record data holds one entity list per track — car, items, objects — and all
of them are decoded; the sample count reported per replay is the car states,
the rest is reported separately as `other tracks`. Record data written at
replay level by merge/preview tooling is decoded the same way (JSON:
`replay.record_data`; `samples.csv` rows use ghost `-1`).

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
| `<stem>.<map name>.Challenge.Gbx` | the map embedded in the replay |
| `<stem>.body.bin` + `<stem>.chunks/` | with `--raw`: decompressed body and raw chunk payloads |

## How it works

A GameBox is a chunked archive: a header with class id, user data and a chunk
table, a reference table, and a body compressed with LZO1X. The body holds the
replay record (class `0x03093000`), which references ghost nodes (`0x03092000`),
which in turn reference the `CPlugEntRecordData` (`0x0911F000`) holding the car
samples, and carry the input stream in chunk `0x0309201D`. Merge/preview
tooling writes one combined `0x0911F000` directly in the replay record instead;
it is parsed in place and the chunk walk continues after it. Chunks marked
"skippable" declare their size, which is what makes a lenient parser possible:
if a chunk cannot be interpreted, its payload is stepped over instead of
aborting the whole node.

Format references used while writing this:

* [Mania Tech Wiki: GBX](https://wiki.xaseco.org/wiki/GBX) — container, chunk
  framing, lookback strings, primitives.
* [GBX.NET](https://github.com/BigBang1112/gbx-net) — class/chunk catalogues
  (`.chunkl` files), the `CPlugEntRecordData` layout and the TM2020 input bit
  layout; see its wiki page
  [Extracting input data from ghosts](https://github.com/BigBang1112/gbx-net/wiki/Extracting-input-data-from-ghosts).
* [pygbx](https://pypi.org/project/pygbx/) — cross-check for chunk walking.

The LZO decoder was validated byte-for-byte against the reference LZO
implementation, and the input decoder is cross-checked against the vehicle
samples: for every run, the decoded input steering and the recorded wheel
steering agree.

## Measured on a collection

Run over a 692 replay collection (654 MB, game builds from 2021 to 2025), plus
one standalone `.Ghost.gbx` that the directory scan now picks up as well:

| Check | Result |
| --- | --- |
| Replays parsed | 692 / 692, no errors, no truncated bodies |
| Replays with a decoded input stream | 692 / 692 (530,027 input events) |
| Replays with car samples | 691 / 692 (601,600 car states) |
| Decoded speed vs. distance covered between samples | median deviation 0.2%, 90th percentile 2.7% (respawn jumps are the only large outliers); asserted within tolerance in `cargo test` |
| Warnings | 36 total: 16 auxiliary (media-clip) nodes, 13 chunk resyncs, 6 short input tails, 1 other |

The single replay without car samples is an unfinished run (no race time) that
only carries inputs. Input steering versus recorded wheel steering is asserted
per sample in `cargo test` and holds for the fixture with a worst-case deviation
of 0.0039 of full lock — exactly one steer quantisation step (1/255); the test
bound is 0.01.

## Limitations

* Directory scans are **non-recursive** and match `.Replay.Gbx` / `.Ghost.Gbx`
  case-insensitively: replays that live in subdirectories are not found, so
  passing a collection that contains them undercounts (the measurement above
  therefore covers the 692 replays in the top level only).
* Some 2020/2021 recordings declare a few more input ticks than the stream
  stores — the old game builds wrote the trailing ticks short (up to ~150 ms,
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

## Repository layout

```
crates/trackmaniare/src/lzo.rs       LZO1X decompressor
crates/trackmaniare/src/reader.rs    binary reader, lookback strings ("Id") and identifiers
crates/trackmaniare/src/gbx.rs       GameBox container and chunk framing
crates/trackmaniare/src/replay.rs    replay record node
crates/trackmaniare/src/ghost.rs     ghost node, incl. validation and input chunks
crates/trackmaniare/src/record.rs    CPlugEntRecordData and vehicle sample decoding
crates/trackmaniare/src/inputs.rs    TM2020 bit-packed inputs, legacy control entries
crates/trackmaniare/src/model.rs     output model and JSON rendering
crates/trackmaniare/src/tabular.rs   CSV rendering and the text summary
crates/trackmaniare/src/main.rs      CLI
crates/trackmaniare/src/bin/         bench harness (status, crashes, timings)
crates/trackmaniare/tests/           LZO vectors, input decoding, end-to-end replay fixture,
                                     hostile-input regressions, CLI behaviour
crates/trackmaniare/examples/        chunks.rs (chunk inventory), sample.rs (raw sample inspection)
crates/trackmaniare-wasm/src/lib.rs  wasm-bindgen API: metadata JSON, typed-array timelines,
                                     map bytes, CSV exports
```