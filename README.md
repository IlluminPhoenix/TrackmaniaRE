# TrackmaniaRE

Extracts everything a Trackmania 2020 `.Replay.Gbx` contains — replay metadata,
ghost records, the car sample stream and the decoded player inputs — into JSON and
CSV.

Trackmania 2020 replays do not store the raw key presses of older games, but they
do store a bit-packed, tick-based input stream (100 Hz) in the ghost's
`0x0309201D` chunk, plus a 20 Hz stream of car states (position, rotation, speed,
steering, pedals, RPM, gear, ground contact) in the ghost's record data. Both are
decoded here.

## Build

```sh
cargo build --release        # binary at target/release/TrackmaniaRE
cargo test                   # 15 tests, incl. an end-to-end replay fixture
```

Only one dependency: `miniz_oxide` (pure-Rust zlib, used for the ghost record
blobs). The GameBox body is decompressed with a built-in LZO1X decoder.

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

## What is extracted

**Replay record** — game version, exe build, title, map uid/name/author/author
zone, environment, mood, map type/style, display cost, player model, best time,
respawns, stunt score, validable, checkpoints, the raw community XML, the
author block, and the map embedded in the replay (`0x03093002`).

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
the rest is reported separately as `other tracks`.

Nothing is thrown away silently: undecoded or deliberately skipped chunks are
listed in the `chunks` inventory (`parsed` / `skipped` / `unknown`) and anything
odd shows up in `warnings`.

## Output files

| File | Contents |
| --- | --- |
| `<stem>.json` | everything, including timelines when `-t` is given |
| `<stem>.inputs.csv` | one row per input change |
| `<stem>.inputs.dense.csv` | one row per 10 ms tick with the state in effect |
| `<stem>.samples.csv` | one row per car sample |
| `<stem>.<map name>.Challenge.Gbx` | the map embedded in the replay |
| `<stem>.body.bin` + `<stem>.chunks/` | with `--raw`: decompressed body and raw chunk payloads |

## How it works

A GameBox is a chunked archive: a header with class id, user data and a chunk
table, a reference table, and a body compressed with LZO1X. The body holds the
replay record (class `0x03093000`), which references ghost nodes (`0x03092000`),
which in turn reference the `CPlugEntRecordData` (`0x0911F000`) holding the car
samples, and carry the input stream in chunk `0x0309201D`. Chunks marked
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

Run over a 692 replay collection (654 MB, game builds from 2021 to 2025):

| Check | Result |
| --- | --- |
| Replays parsed | 692 / 692, no errors, no truncated bodies |
| Replays with a decoded input stream | 692 / 692 (530,027 input events) |
| Replays with car samples | 691 / 692 (601,600 car states) |
| Decoded speed vs. distance covered between samples | agrees on every replay |
| Warnings | 36 total: 16 media-clip nodes, 9 chunk resyncs, 6 short input streams, 5 other |

The single replay without car samples is an unfinished run (no race time) that
only carries inputs. Input steering versus recorded wheel steering is asserted
per sample in `cargo test` and holds for the fixture with a worst-case deviation
of 0.03 of full lock.

## Limitations

* Runs recorded before the 2020-07-20 game build can have a slightly different
  input layout; those decode partially and are reported with an
  `input stream ended early` warning.
* Inputs bound to the mouse are known to come out corrupted upstream of us.
* Cinematic replays reference media clip nodes, which are stepped over rather
  than decoded.
* This is a reader: it never writes `.Replay.Gbx` files.

## Repository layout

```
src/lzo.rs       LZO1X decompressor
src/reader.rs    binary reader, lookback strings ("Id") and identifiers
src/gbx.rs       GameBox container and chunk framing
src/replay.rs    replay record node
src/ghost.rs     ghost node, incl. validation and input chunks
src/record.rs    CPlugEntRecordData and vehicle sample decoding
src/inputs.rs    TM2020 bit-packed inputs, legacy control entries
src/model.rs     output model and JSON rendering
src/tabular.rs   CSV rendering and the text summary
src/main.rs      CLI
tests/           LZO vectors, input decoding, end-to-end replay fixture
examples/        chunks.rs (chunk inventory), sample.rs (raw sample inspection)
```