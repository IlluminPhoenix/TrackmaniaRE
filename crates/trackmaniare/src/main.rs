//! Trackmania replay extractor.
//!
//! Usage:
//!   TrackmaniaRE [OPTIONS] <PATH>...
//!
//! PATH may be a replay file, a ghost file, or a directory (searched
//! non-recursively for `*.Replay.Gbx` / `*.Ghost.Gbx`).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use trackmaniare::tabular;
use trackmaniare::{ExtractOptions, parse_replay_with};

struct Options {
    paths: Vec<PathBuf>,
    out_dir: Option<PathBuf>,
    mode: Mode,
    timelines: bool,
    keep_map: bool,
    dense: bool,
    raw: bool,
    quiet: bool,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Mode {
    Summary,
    Json,
    Info,
}

fn main() -> ExitCode {
    let mut opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("{}", usage());
            return ExitCode::FAILURE;
        }
    };

    if opts.paths.is_empty() {
        eprintln!("{}", usage());
        return ExitCode::FAILURE;
    }

    let mut files = Vec::new();
    for path in &opts.paths {
        match collect(path) {
            Ok(list) => files.extend(list),
            Err(e) => {
                eprintln!("error: {}: {e}", path.display());
                opts.quiet = false;
            }
        }
    }
    files.sort();
    files.dedup();

    if files.is_empty() {
        eprintln!("error: no .Replay.Gbx or .Ghost.Gbx files found");
        return ExitCode::FAILURE;
    }

    if let Some(dir) = &opts.out_dir
        && let Err(e) = fs::create_dir_all(dir)
    {
        eprintln!("error: cannot create {}: {e}", dir.display());
        return ExitCode::FAILURE;
    }

    // Replays with the same file name in different directories would write to
    // the same outputs, so give every input its own stem up front.
    let stems = unique_stems(&files);

    let mut failures = 0usize;
    for (file, stem) in files.iter().zip(&stems) {
        let data = match fs::read(file) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("error: {}: {e}", file.display());
                failures += 1;
                continue;
            }
        };
        let keep_map = opts.out_dir.is_some() && opts.keep_map;
        let name = file
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        match parse_replay_with(
            &data,
            &name,
            ExtractOptions {
                keep_embedded_map: keep_map,
                keep_body: opts.raw,
            },
        ) {
            Ok(ex) => {
                match opts.mode {
                    Mode::Summary => {
                        if files.len() > 1 && !opts.quiet {
                            println!("=== {}", file.display());
                        }
                        print!("{}", tabular::summary_text(&ex));
                    }
                    Mode::Json => {
                        let value = ex.to_json(opts.timelines);
                        match &opts.out_dir {
                            Some(dir) => {
                                if let Err(e) = write_outputs(
                                    dir,
                                    &ex,
                                    stem,
                                    opts.timelines,
                                    opts.dense,
                                    opts.raw,
                                ) {
                                    eprintln!("error: writing outputs for {}: {e}", file.display());
                                    failures += 1;
                                    continue;
                                }
                            }
                            None => print!("{}", value.to_pretty()),
                        }
                    }
                    Mode::Info => {
                        let value = ex.to_json(false);
                        match &opts.out_dir {
                            Some(dir) => {
                                if let Err(e) = write_file(
                                    &dir.join(format!("{stem}.json")),
                                    &value.to_pretty(),
                                ) {
                                    eprintln!("error: {e}");
                                    failures += 1;
                                }
                            }
                            None => print!("{}", value.to_pretty()),
                        }
                    }
                }
                if !opts.quiet && matches!(opts.mode, Mode::Summary) && files.len() > 1 {
                    println!();
                }
            }
            Err(e) => {
                eprintln!("error: {}: {e}", file.display());
                failures += 1;
            }
        }
    }

    if failures > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// One output stem per input file, made unique so replays sharing a file name
/// cannot silently overwrite each other's outputs.
fn unique_stems(files: &[PathBuf]) -> Vec<String> {
    let mut used = HashSet::new();
    let mut out = Vec::with_capacity(files.len());
    for file in files {
        let base = file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "replay".into());
        let stem = if used.insert(base.clone()) {
            base
        } else {
            let mut n = 2;
            loop {
                let candidate = format!("{base}-{n}");
                if used.insert(candidate.clone()) {
                    break candidate;
                }
                n += 1;
            }
        };
        out.push(stem);
    }
    out
}

fn write_outputs(
    dir: &Path,
    ex: &trackmaniare::model::Extraction,
    stem: &str,
    timelines: bool,
    dense: bool,
    raw: bool,
) -> std::io::Result<()> {
    if raw && !ex.body.is_empty() {
        // Everything the parser did not decode is still available here: the
        // complete decompressed body plus every skippable chunk payload.
        write_bytes(&dir.join(format!("{stem}.body.bin")), &ex.body)?;
        let chunk_dir = dir.join(format!("{stem}.chunks"));
        fs::create_dir_all(&chunk_dir)?;
        for c in &ex.chunks {
            let Some(size) = c.size else { continue };
            let payload: &[u8] = if c.node == "header" {
                // Header chunk payloads live in the user-data block, not in
                // the body.
                match ex
                    .header_chunk_data
                    .iter()
                    .find(|(id, data)| *id == c.id && data.len() == size)
                {
                    Some((_, data)) => data,
                    None => continue,
                }
            } else {
                let start = c.offset + 8; // skip the chunk id and the SKIP marker
                if start + size > ex.body.len() {
                    continue;
                }
                &ex.body[start..start + size]
            };
            let name = format!("{:06}_{}_{:08X}.bin", c.offset, sanitize(&c.node), c.id);
            write_bytes(&chunk_dir.join(name), payload)?;
        }
    }

    write_file(
        &dir.join(format!("{stem}.json")),
        &ex.to_json(timelines).to_pretty(),
    )?;
    write_file(
        &dir.join(format!("{stem}.inputs.csv")),
        &tabular::inputs_csv(ex),
    )?;
    if dense {
        write_file(
            &dir.join(format!("{stem}.inputs.dense.csv")),
            &tabular::inputs_csv_dense(ex),
        )?;
    }
    write_file(
        &dir.join(format!("{stem}.samples.csv")),
        &tabular::samples_csv(ex),
    )?;

    if let Some(map) = &ex.embedded_map {
        let map_name = ex
            .replay
            .xml_attributes
            .iter()
            .find(|(k, _)| k == "name")
            .map(|(_, v)| sanitize(v))
            .unwrap_or_else(|| "map".into());
        write_bytes(&dir.join(format!("{stem}.{map_name}.Challenge.Gbx")), map)?;
    }
    Ok(())
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn write_file(path: &Path, contents: &str) -> std::io::Result<()> {
    fs::write(path, contents)
}

fn write_bytes(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    fs::write(path, contents)
}

fn collect(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    if path.is_dir() {
        let mut out = Vec::new();
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_file() {
                let name = p
                    .file_name()
                    .map(|s| s.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                if name.ends_with(".replay.gbx") || name.ends_with(".ghost.gbx") {
                    out.push(p);
                }
            }
        }
        Ok(out)
    } else if path.is_file() {
        Ok(vec![path.to_path_buf()])
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no such file or directory",
        ))
    }
}

fn usage() -> String {
    "TrackmaniaRE - extract replay data and inputs from Trackmania .Replay.Gbx files\n\
     \n\
     USAGE:\n    TrackmaniaRE [OPTIONS] <PATH>...\n\
     \n\
     ARGS:\n\
     \x20   <PATH>...    replay (.Replay.Gbx) or ghost (.Ghost.Gbx) file, or a\n\
     \x20                directory searched (non-recursively) for such files\n\
     \n\
     OPTIONS:\n\
     \x20   -s, --summary        human readable summary (default)\n\
     \x20   -j, --json           write JSON per replay (stdout, or into --out-dir)\n\
     \x20       --info            container + chunk inventory only\n\
     \x20   -o, --out-dir DIR    write per-replay outputs (JSON, inputs CSV, samples CSV, map)\n\
     \x20   -t, --timelines      include full input/sample timelines in the JSON\n\
     \x20   -m, --map            write the embedded map as <name>.Challenge.Gbx\n\
     \x20   -d, --dense          also write a per-tick (10 ms) input timeline CSV\n\
     \x20   -r, --raw            also dump the decompressed body and every chunk payload\n\
     \x20   -q, --quiet          suppress the per-file separator\n\
     \x20   --                   treat every remaining argument as a path\n\
     \x20   -h, --help           print this help\n"
        .to_string()
}

fn parse_args() -> Result<Options, String> {
    let mut paths = Vec::new();
    let mut out_dir: Option<PathBuf> = None;
    let mut mode: Option<Mode> = None;
    let mut timelines = false;
    let mut keep_map = false;
    let mut dense = false;
    let mut raw = false;
    let mut quiet = false;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "-h" | "--help" => {
                print!("{}", usage());
                std::process::exit(0);
            }
            "-s" | "--summary" => mode = Some(Mode::Summary),
            "-j" | "--json" => mode = Some(Mode::Json),
            "--info" => mode = Some(Mode::Info),
            "-t" | "--timelines" => timelines = true,
            "-d" | "--dense" => {
                dense = true;
                if out_dir.is_none() {
                    out_dir = Some(PathBuf::from("trackmaniare-out"));
                }
            }
            "-m" | "--map" => keep_map = true,
            "-r" | "--raw" => {
                raw = true;
                keep_map = true;
                timelines = true;
                if out_dir.is_none() {
                    out_dir = Some(PathBuf::from("trackmaniare-out"));
                }
            }
            "-q" | "--quiet" => quiet = true,
            "-o" | "--out-dir" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--out-dir needs a value".to_string())?;
                out_dir = Some(PathBuf::from(value));
            }
            other if other.starts_with("--out-dir=") => {
                out_dir = Some(PathBuf::from(&other["--out-dir=".len()..]));
            }
            "--" => {
                paths.extend(args[i + 1..].iter().map(PathBuf::from));
                break;
            }
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => paths.push(PathBuf::from(other)),
        }
        i += 1;
    }

    // An explicit mode always wins; otherwise `--out-dir` (also implied by
    // `--dense`/`--raw`) means JSON output. This makes the flag order
    // irrelevant.
    let mode = mode.unwrap_or(if out_dir.is_some() {
        Mode::Json
    } else {
        Mode::Summary
    });

    Ok(Options {
        paths,
        out_dir,
        mode,
        timelines,
        keep_map,
        dense,
        raw,
        quiet,
    })
}
