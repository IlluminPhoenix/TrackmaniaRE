# AGENTS.md

Rust workspace (stable Rust, edition 2024) for a Trackmania 2020 `.Replay.Gbx` and
`.Ghost.Gbx` extractor: a CLI plus wasm-bindgen bindings for a planned web frontend.
`README.md` is the source of truth for behaviour, extraction scope and limitations;
keep it accurate when behaviour changes. Write human-facing docs with the `unslop`
skill when it is installed.

## Layout

- `crates/trackmaniare/` - parser, `TrackmaniaRE` CLI and `trackmaniare-bench` (default workspace member).
- `crates/trackmaniare-wasm/` - wasm bindings for the web frontend, built with wasm-pack. `wasm-bindgen` is pinned exactly, so the crate and the wasm-pack CLI move together.
- `web/` - frontend, not started.

## Before committing

CI (`.github/workflows/ci.yml`) gates fmt, clippy with `-D warnings`, the workspace
tests and the wasm build; make those pass locally. Tests run on the small fixtures
in `crates/trackmaniare/tests/fixtures/` and must not depend on a local replay
collection. For parser changes, run the bench harness over a replay folder when one
is available (`cargo run --release --bin trackmaniare-bench -- <replay-dir>`,
non-zero exit on any failure).

## Invariants

- Parse failures are warnings, never fatal; skippable chunks are stepped over so a hostile or odd file cannot abort a run.
- The README Limitations section lists known quirks (mouse inputs, short input tails, non-recursive scans); read it before changing behaviour around one.
- The README `## AI disclosure` section is human-written and agents may edit it, but keep its blunt tone: it deliberately turns away AI-hostile readers.
