//! Bench harness over a replay collection: does every file parse, does
//! anything crash, and how fast does a parse run?
//!
//! ```text
//! trackmaniare-bench [OPTIONS] <PATH>...
//! ```
//!
//! PATH is a replay file, a ghost file, or a directory (searched recursively
//! for `*.Replay.Gbx` / `*.Ghost.Gbx`). Every parse runs under
//! `catch_unwind`, so one broken replay cannot take the run down: a panic is
//! recorded against its file and the run continues. The exit code is
//! non-zero when any file could not be read, could not be parsed, or
//! panicked.

use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Mutex;
use std::time::Instant;

use trackmaniare::json::Json;
use trackmaniare::model::{ChunkDisposition, Extraction, GhostInputs};
use trackmaniare::{ExtractOptions, parse_replay_with};

/// Message and location of the last panic, filled by the panic hook so the
/// bench can report where a parser panic happened, not only that it did.
static PANIC_INFO: Mutex<Option<String>> = Mutex::new(None);

struct Options {
    paths: Vec<PathBuf>,
    repeat: u32,
    json: Option<PathBuf>,
    csv: Option<PathBuf>,
    verbose: bool,
    quiet: bool,
    simulate_panic: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Ok,
    Warn,
    /// The parser gave up inside a node (unknown non-skippable chunk, EOF in
    /// an incomplete body, ...): the file was only decoded up to that point.
    Partial,
    Error,
    Panic,
    Unreadable,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Warn => "WARN",
            Status::Partial => "PARTIAL",
            Status::Error => "ERROR",
            Status::Panic => "PANIC",
            Status::Unreadable => "READ",
        }
    }

    fn failed(self) -> bool {
        matches!(
            self,
            Status::Partial | Status::Error | Status::Panic | Status::Unreadable
        )
    }
}

enum Attempt {
    Parsed(Box<Extraction>),
    Failed(Status, String),
}

struct FileResult {
    path: PathBuf,
    size: u64,
    status: Status,
    detail: String,
    warnings: Vec<String>,
    read_ms: f64,
    /// One entry per successful parse attempt (empty when parse failed).
    parse_ms: Vec<f64>,
    ghosts: usize,
    input_events: usize,
    /// Ghosts whose input stream decoded only up to a stop (old archive
    /// versions); the rest of the file is intact.
    truncated_inputs: usize,
    car_samples: usize,
    other_samples: usize,
    race_time_ms: Option<i64>,
    body_size: u64,
    body_uncompressed: u64,
    body_complete: bool,
    chunks_parsed: usize,
    chunks_skipped: usize,
    chunks_unknown: usize,
    map_bytes: u64,
}

impl FileResult {
    fn new(path: &Path) -> Self {
        FileResult {
            path: path.to_path_buf(),
            size: 0,
            status: Status::Ok,
            detail: String::new(),
            warnings: Vec::new(),
            read_ms: 0.0,
            parse_ms: Vec::new(),
            ghosts: 0,
            input_events: 0,
            truncated_inputs: 0,
            car_samples: 0,
            other_samples: 0,
            race_time_ms: None,
            body_size: 0,
            body_uncompressed: 0,
            body_complete: true,
            chunks_parsed: 0,
            chunks_skipped: 0,
            chunks_unknown: 0,
            map_bytes: 0,
        }
    }

    fn fill_from(&mut self, ex: &Extraction) {
        self.status = if ex.warnings.iter().any(|w| is_stopped_warning(w)) {
            // Parsing stopped mid-node: everything past that point is missing,
            // which is not the same as "parsed with notes".
            self.detail = ex
                .warnings
                .iter()
                .find(|w| is_stopped_warning(w))
                .cloned()
                .unwrap_or_default();
            Status::Partial
        } else if ex.warnings.is_empty() {
            Status::Ok
        } else {
            Status::Warn
        };
        self.warnings = ex.warnings.clone();
        self.ghosts = ex.ghosts.len();
        self.input_events = ex
            .ghosts
            .iter()
            .map(|g| g.inputs.as_ref().map_or(0, |i| i.event_count()))
            .sum();
        self.truncated_inputs = ex
            .ghosts
            .iter()
            .filter(|g| {
                matches!(
                    &g.inputs,
                    Some(GhostInputs::Trackmania2020 {
                        truncated: true,
                        ..
                    })
                )
            })
            .count();
        self.car_samples = ex
            .replay
            .record_data
            .as_ref()
            .map_or(0, |r| r.vehicle_sample_count())
            + ex.ghosts.iter().map(|g| g.sample_count()).sum::<usize>();
        self.other_samples = ex
            .replay
            .record_data
            .as_ref()
            .map_or(0, |r| r.other_sample_count())
            + ex.ghosts
                .iter()
                .map(|g| g.other_sample_count())
                .sum::<usize>();
        self.race_time_ms = ex
            .ghosts
            .iter()
            .find_map(|g| g.race_time_ms)
            .map(i64::from)
            .or(ex.replay.time_ms.map(i64::from));
        self.body_size = ex.container.body_size as u64;
        self.body_uncompressed = ex.container.body_uncompressed_size as u64;
        self.body_complete = ex.container.body_complete;
        self.map_bytes = ex.replay.embedded_map_size.unwrap_or(0) as u64;
        for c in &ex.chunks {
            match c.disposition {
                ChunkDisposition::Parsed => self.chunks_parsed += 1,
                ChunkDisposition::Skipped => self.chunks_skipped += 1,
                ChunkDisposition::Unknown => self.chunks_unknown += 1,
            }
        }
    }

    fn median_ms(&self) -> f64 {
        median(&self.parse_ms)
    }

    /// One line for a file that needs attention, printed as it is found.
    fn issue_line(&self) -> String {
        match self.status {
            Status::Warn => format!(
                "WARN   {}: {} warning{}",
                self.path.display(),
                self.warnings.len(),
                if self.warnings.len() == 1 { "" } else { "s" }
            ),
            _ => format!(
                "{:<7} {}: {}",
                self.status.label(),
                self.path.display(),
                self.detail
            ),
        }
    }
}

#[derive(Default)]
struct PerFile {
    min_ms: f64,
    median_ms: f64,
    mean_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
}

#[derive(Default)]
struct Summary {
    files: usize,
    bytes: u64,
    ok: usize,
    warn: usize,
    partial: usize,
    error: usize,
    panic: usize,
    unreadable: usize,
    warnings: usize,
    files_with_warnings: usize,
    files_with_inputs: usize,
    input_events: u64,
    files_with_truncated_inputs: usize,
    files_with_samples: usize,
    car_samples: u64,
    other_samples: u64,
    files_with_time: usize,
    race_time_ms: i64,
    map_bytes: u64,
    chunks_parsed: u64,
    chunks_skipped: u64,
    chunks_unknown: u64,
    read_ms: f64,
    parse_ms: f64,
    wall_ms: f64,
    per_file: PerFile,
    parse_throughput_mib_s: f64,
    end_to_end_throughput_mib_s: f64,
}

impl Summary {
    fn from(results: &[FileResult], wall_ms: f64) -> Self {
        let mut s = Summary {
            files: results.len(),
            wall_ms,
            ..Summary::default()
        };
        let mut medians = Vec::with_capacity(results.len());
        let mut steady_ms = 0.0;
        let mut steady_bytes = 0u64;
        for r in results {
            s.bytes += r.size;
            match r.status {
                Status::Ok => s.ok += 1,
                Status::Warn => s.warn += 1,
                Status::Partial => s.partial += 1,
                Status::Error => s.error += 1,
                Status::Panic => s.panic += 1,
                Status::Unreadable => s.unreadable += 1,
            }
            s.warnings += r.warnings.len();
            if !r.warnings.is_empty() {
                s.files_with_warnings += 1;
            }
            if r.input_events > 0 {
                s.files_with_inputs += 1;
                s.input_events += r.input_events as u64;
            }
            if r.truncated_inputs > 0 {
                s.files_with_truncated_inputs += 1;
            }
            if r.car_samples > 0 {
                s.files_with_samples += 1;
            }
            s.car_samples += r.car_samples as u64;
            s.other_samples += r.other_samples as u64;
            if let Some(t) = r.race_time_ms {
                s.files_with_time += 1;
                s.race_time_ms += t;
            }
            s.map_bytes += r.map_bytes;
            s.chunks_parsed += r.chunks_parsed as u64;
            s.chunks_skipped += r.chunks_skipped as u64;
            s.chunks_unknown += r.chunks_unknown as u64;
            s.read_ms += r.read_ms;
            s.parse_ms += r.parse_ms.iter().sum::<f64>();
            if !r.parse_ms.is_empty() {
                let median = r.median_ms();
                medians.push(median);
                steady_ms += median;
                steady_bytes += r.size;
            }
        }
        medians.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if !medians.is_empty() {
            let mean = medians.iter().sum::<f64>() / medians.len() as f64;
            s.per_file = PerFile {
                min_ms: medians[0],
                median_ms: percentile(&medians, 50.0),
                mean_ms: mean,
                p95_ms: percentile(&medians, 95.0),
                p99_ms: percentile(&medians, 99.0),
                max_ms: medians[medians.len() - 1],
            };
        }
        if steady_ms > 0.0 {
            s.parse_throughput_mib_s =
                steady_bytes as f64 / (1024.0 * 1024.0) / (steady_ms / 1000.0);
        }
        if wall_ms > 0.0 {
            s.end_to_end_throughput_mib_s = s.bytes as f64 / (1024.0 * 1024.0) / (wall_ms / 1000.0);
        }
        s
    }

    fn failures(&self) -> usize {
        self.partial + self.error + self.panic + self.unreadable
    }
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("{}", usage());
            return ExitCode::FAILURE;
        }
    };
    install_panic_hook();
    run(&opts)
}

fn run(opts: &Options) -> ExitCode {
    let mut files = Vec::new();
    for path in &opts.paths {
        if let Err(e) = collect(path, &mut files) {
            eprintln!("error: {}: {e}", path.display());
        }
    }
    files.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    files.dedup();

    if files.is_empty() {
        eprintln!("error: no .Replay.Gbx or .Ghost.Gbx files found");
        return ExitCode::FAILURE;
    }

    let total = files.len();
    let started = Instant::now();
    let mut results = Vec::with_capacity(total);
    for (i, file) in files.iter().enumerate() {
        let result = measure(file, i + 1, opts);
        if opts.verbose {
            let time = if result.parse_ms.is_empty() {
                "-".to_string()
            } else {
                fmt_ms(result.median_ms())
            };
            println!(
                "[{:>4}/{total}] {:<7} {:>9}  {}",
                i + 1,
                result.status.label(),
                time,
                file.display()
            );
        } else if !opts.quiet && result.status.failed() {
            println!("{}", result.issue_line());
        }
        results.push(result);
    }
    let summary = Summary::from(&results, ms(started.elapsed()));

    print_report(opts, &results, &summary);

    let mut write_failed = false;
    if let Some(path) = &opts.json
        && let Err(e) = write_json_report(path, opts, &results, &summary)
    {
        eprintln!("error: cannot write {}: {e}", path.display());
        write_failed = true;
    }
    if let Some(path) = &opts.csv
        && let Err(e) = write_csv_report(path, &results)
    {
        eprintln!("error: cannot write {}: {e}", path.display());
        write_failed = true;
    }

    if write_failed || summary.failures() > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Reads, parses (optionally repeatedly) and measures one file.
fn measure(path: &Path, index: usize, opts: &Options) -> FileResult {
    let mut result = FileResult::new(path);

    let read_started = Instant::now();
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            result.status = Status::Unreadable;
            result.detail = e.to_string();
            result.read_ms = ms(read_started.elapsed());
            return result;
        }
    };
    result.read_ms = ms(read_started.elapsed());
    result.size = data.len() as u64;

    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let simulate = opts.simulate_panic == Some(index);

    let mut first: Option<Box<Extraction>> = None;
    let mut failed: Option<(Status, String)> = None;
    for _ in 0..opts.repeat {
        let (attempt, elapsed_ms) = parse_attempt(&data, &name, simulate);
        match attempt {
            Attempt::Parsed(ex) => {
                result.parse_ms.push(elapsed_ms);
                if first.is_none() {
                    first = Some(ex);
                }
            }
            Attempt::Failed(status, detail) => {
                failed = Some((status, detail));
                break;
            }
        }
    }

    if let Some((status, detail)) = failed {
        result.status = status;
        result.detail = detail;
    } else {
        let ex = first.expect("repeat >= 1 and no failure means a parse succeeded");
        result.fill_from(&ex);
    }
    result
}

/// One timed parse attempt. The parser is expected never to panic; if it
/// does, the panic is caught, attributed to this file and stored for the
/// report instead of aborting the run.
fn parse_attempt(data: &[u8], name: &str, simulate_panic: bool) -> (Attempt, f64) {
    let started = Instant::now();
    let caught = panic::catch_unwind(AssertUnwindSafe(|| {
        if simulate_panic {
            panic!("simulated parser panic (--simulate-panic)");
        }
        parse_replay_with(
            data,
            name,
            ExtractOptions {
                keep_embedded_map: false,
                keep_body: false,
            },
        )
    }));
    let elapsed = ms(started.elapsed());
    let attempt = match caught {
        Ok(Ok(ex)) => Attempt::Parsed(Box::new(ex)),
        Ok(Err(e)) => Attempt::Failed(Status::Error, e.to_string()),
        Err(_) => Attempt::Failed(
            Status::Panic,
            take_panic_info().unwrap_or_else(|| "panic without a message".to_string()),
        ),
    };
    (attempt, elapsed)
}

fn install_panic_hook() {
    panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "non-string panic payload".to_string()
        };
        let location = info
            .location()
            .map(|l| format!(" ({}:{}:{})", l.file(), l.line(), l.column()))
            .unwrap_or_default();
        let text = format!("{payload}{location}");
        if let Ok(mut slot) = PANIC_INFO.lock() {
            *slot = Some(text.clone());
        }
        // Keep a trace on stderr too: if the bench itself panics, the custom
        // hook must not swallow the message.
        eprintln!("panic: {text}");
    }));
}

fn take_panic_info() -> Option<String> {
    PANIC_INFO.lock().ok().and_then(|mut slot| slot.take())
}

fn print_report(opts: &Options, results: &[FileResult], s: &Summary) {
    println!(
        "trackmaniare-bench {} ({} {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!(
        "paths      {}",
        opts.paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "files      {} ({}), {} repeat{} per file",
        group(s.files as u64),
        fmt_bytes(s.bytes),
        opts.repeat,
        if opts.repeat == 1 { "" } else { "s" }
    );
    println!();
    println!(
        "result     ok {}, warnings {}, partial {}, errors {}, panics {}, unreadable {}",
        s.ok, s.warn, s.partial, s.error, s.panic, s.unreadable
    );
    let truncated = if s.files_with_truncated_inputs > 0 {
        format!(
            ", {} file{} with a short input tail",
            s.files_with_truncated_inputs,
            if s.files_with_truncated_inputs == 1 {
                ""
            } else {
                "s"
            }
        )
    } else {
        String::new()
    };
    println!(
        "inputs     {}/{} files with an input stream ({} events{})",
        s.files_with_inputs,
        s.files,
        group(s.input_events),
        truncated
    );
    if s.files_with_truncated_inputs > 0 {
        println!(
            "note       a short input tail means the archive declares a few ticks more than it"
        );
        println!("           stores (old game builds); the stored part decodes fully");
    }
    println!(
        "samples    {}/{} files with car samples ({} states, {} item/object states)",
        s.files_with_samples,
        s.files,
        group(s.car_samples),
        group(s.other_samples)
    );
    println!(
        "races      {}/{} runs with a race time, {} recorded",
        s.files_with_time,
        s.files,
        fmt_race(s.race_time_ms)
    );
    println!(
        "maps       {} embedded, {} uncompressed",
        group(count_maps(results) as u64),
        fmt_bytes(s.map_bytes)
    );
    println!(
        "chunks     {} parsed, {} skipped, {} unknown",
        group(s.chunks_parsed),
        group(s.chunks_skipped),
        group(s.chunks_unknown)
    );
    println!(
        "warnings   {} in {} file{}",
        group(s.warnings as u64),
        s.files_with_warnings,
        if s.files_with_warnings == 1 { "" } else { "s" }
    );
    println!(
        "time       read {}, parse {}, wall {}",
        fmt_ms(s.read_ms),
        fmt_ms(s.parse_ms),
        fmt_ms(s.wall_ms)
    );
    if s.files > 0 {
        println!(
            "per file   parse min {}, median {}, p95 {}, p99 {}, max {}",
            fmt_ms(s.per_file.min_ms),
            fmt_ms(s.per_file.median_ms),
            fmt_ms(s.per_file.p95_ms),
            fmt_ms(s.per_file.p99_ms),
            fmt_ms(s.per_file.max_ms)
        );
        println!(
            "speed      parse {:.1} MiB/s (file bytes over the sum of per-file parse medians)",
            s.parse_throughput_mib_s
        );
        println!(
            "           end to end {:.1} MiB/s (file bytes over wall time)",
            s.end_to_end_throughput_mib_s
        );
    }

    let warned: Vec<&FileResult> = results
        .iter()
        .filter(|r| r.status == Status::Warn)
        .collect();
    if !warned.is_empty() {
        println!();
        println!("warnings ({} files):", warned.len());
        for r in &warned {
            println!(
                "  {}: {} warning{} - {}",
                r.path.display(),
                r.warnings.len(),
                if r.warnings.len() == 1 { "" } else { "s" },
                r.warnings.first().cloned().unwrap_or_default()
            );
        }
    }

    let failed: Vec<&FileResult> = results.iter().filter(|r| r.status.failed()).collect();
    if !failed.is_empty() {
        println!();
        println!("failures ({}):", failed.len());
        for r in &failed {
            println!(
                "  {:<7} {}: {}",
                r.status.label(),
                r.path.display(),
                r.detail
            );
        }
    }

    let mut timed: Vec<&FileResult> = results.iter().filter(|r| !r.parse_ms.is_empty()).collect();
    timed.sort_by(|a, b| {
        b.median_ms()
            .partial_cmp(&a.median_ms())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if timed.len() > 1 {
        println!();
        println!("slowest:");
        for (i, r) in timed.iter().take(5).enumerate() {
            println!(
                "  {}. {:>9}  {}",
                i + 1,
                fmt_ms(r.median_ms()),
                r.path.display()
            );
        }
    }

    println!();
    if s.failures() == 0 {
        if s.files_with_truncated_inputs > 0 {
            println!(
                "all {} files parsed fully, no errors and no panics ({} with a short input tail, see note)",
                group(s.files as u64),
                s.files_with_truncated_inputs
            );
        } else {
            println!(
                "all {} files parsed fully, no errors and no panics",
                group(s.files as u64)
            );
        }
    } else {
        println!(
            "{} of {} files failed or parsed only partially (see above)",
            group(s.failures() as u64),
            group(s.files as u64)
        );
    }
}

fn count_maps(results: &[FileResult]) -> usize {
    results.iter().filter(|r| r.map_bytes > 0).count()
}

fn write_json_report(
    path: &Path,
    opts: &Options,
    results: &[FileResult],
    s: &Summary,
) -> std::io::Result<()> {
    let mut root = Json::obj();
    root.set("tool", Json::str("trackmaniare-bench"));
    root.set("version", Json::str(env!("CARGO_PKG_VERSION")));
    root.set("host_os", Json::str(std::env::consts::OS));
    root.set("host_arch", Json::str(std::env::consts::ARCH));
    root.set("repeat", Json::UInt(opts.repeat as u64));
    root.set(
        "paths",
        Json::Arr(
            opts.paths
                .iter()
                .map(|p| Json::str(p.display().to_string()))
                .collect(),
        ),
    );

    let mut summary = Json::obj();
    summary.set("files", Json::UInt(s.files as u64));
    summary.set("bytes", Json::UInt(s.bytes));
    summary.set("ok", Json::UInt(s.ok as u64));
    summary.set("warnings_files", Json::UInt(s.files_with_warnings as u64));
    summary.set("warnings_total", Json::UInt(s.warnings as u64));
    summary.set("partial", Json::UInt(s.partial as u64));
    summary.set("errors", Json::UInt(s.error as u64));
    summary.set("panics", Json::UInt(s.panic as u64));
    summary.set("unreadable", Json::UInt(s.unreadable as u64));
    summary.set("files_with_inputs", Json::UInt(s.files_with_inputs as u64));
    summary.set("input_events", Json::UInt(s.input_events));
    summary.set(
        "files_with_truncated_inputs",
        Json::UInt(s.files_with_truncated_inputs as u64),
    );
    summary.set(
        "files_with_samples",
        Json::UInt(s.files_with_samples as u64),
    );
    summary.set("car_samples", Json::UInt(s.car_samples));
    summary.set("other_samples", Json::UInt(s.other_samples));
    summary.set("runs_with_time", Json::UInt(s.files_with_time as u64));
    summary.set("race_time_ms", Json::UInt(s.race_time_ms.max(0) as u64));
    summary.set("embedded_map_bytes", Json::UInt(s.map_bytes));
    summary.set("chunks", {
        let mut c = Json::obj();
        c.set("parsed", Json::UInt(s.chunks_parsed));
        c.set("skipped", Json::UInt(s.chunks_skipped));
        c.set("unknown", Json::UInt(s.chunks_unknown));
        c
    });
    summary.set("read_ms", Json::Float(s.read_ms));
    summary.set("parse_ms", Json::Float(s.parse_ms));
    summary.set("wall_ms", Json::Float(s.wall_ms));
    summary.set("per_file_ms", {
        let mut p = Json::obj();
        p.set("min", Json::Float(s.per_file.min_ms));
        p.set("median", Json::Float(s.per_file.median_ms));
        p.set("mean", Json::Float(s.per_file.mean_ms));
        p.set("p95", Json::Float(s.per_file.p95_ms));
        p.set("p99", Json::Float(s.per_file.p99_ms));
        p.set("max", Json::Float(s.per_file.max_ms));
        p
    });
    summary.set(
        "parse_throughput_mib_s",
        Json::Float(s.parse_throughput_mib_s),
    );
    summary.set(
        "end_to_end_throughput_mib_s",
        Json::Float(s.end_to_end_throughput_mib_s),
    );
    root.set("summary", summary);

    let files: Vec<Json> = results
        .iter()
        .map(|r| {
            let mut f = Json::obj();
            f.set("path", Json::str(r.path.display().to_string()));
            f.set("size", Json::UInt(r.size));
            f.set("status", Json::str(r.status.label().to_lowercase()));
            if !r.detail.is_empty() {
                f.set("detail", Json::str(r.detail.clone()));
            }
            if !r.warnings.is_empty() {
                f.set(
                    "warnings",
                    Json::Arr(r.warnings.iter().map(|w| Json::str(w.clone())).collect()),
                );
            }
            f.set("read_ms", Json::Float(r.read_ms));
            if !r.parse_ms.is_empty() {
                f.set("parse_ms", {
                    let mut p = Json::obj();
                    p.set(
                        "min",
                        Json::Float(r.parse_ms.iter().cloned().fold(f64::MAX, f64::min)),
                    );
                    p.set("median", Json::Float(r.median_ms()));
                    p.set(
                        "mean",
                        Json::Float(r.parse_ms.iter().sum::<f64>() / r.parse_ms.len() as f64),
                    );
                    p.set(
                        "max",
                        Json::Float(r.parse_ms.iter().cloned().fold(0.0, f64::max)),
                    );
                    p
                });
            }
            f.set("ghosts", Json::UInt(r.ghosts as u64));
            f.set("input_events", Json::UInt(r.input_events as u64));
            f.set("truncated_inputs", Json::UInt(r.truncated_inputs as u64));
            f.set("car_samples", Json::UInt(r.car_samples as u64));
            f.set("other_samples", Json::UInt(r.other_samples as u64));
            f.set("race_time_ms", Json::opt_int(r.race_time_ms));
            f.set("body_size", Json::UInt(r.body_size));
            f.set("body_uncompressed_size", Json::UInt(r.body_uncompressed));
            f.set("body_complete", Json::Bool(r.body_complete));
            f.set("embedded_map_bytes", Json::UInt(r.map_bytes));
            f.set("chunks_parsed", Json::UInt(r.chunks_parsed as u64));
            f.set("chunks_skipped", Json::UInt(r.chunks_skipped as u64));
            f.set("chunks_unknown", Json::UInt(r.chunks_unknown as u64));
            f
        })
        .collect();
    root.set("files", Json::Arr(files));

    write_file(path, &root.to_pretty())
}

fn write_csv_report(path: &Path, results: &[FileResult]) -> std::io::Result<()> {
    let mut out = String::new();
    out.push_str(
        "path,size_bytes,status,read_ms,parse_ms_min,parse_ms_median,parse_ms_mean,\
         ghosts,input_events,truncated_inputs,car_samples,other_samples,race_time_ms,body_complete,\
         warnings,detail\n",
    );
    for r in results {
        let (min, median, mean) = if r.parse_ms.is_empty() {
            (String::new(), String::new(), String::new())
        } else {
            (
                format!("{:.3}", r.parse_ms.iter().cloned().fold(f64::MAX, f64::min)),
                format!("{:.3}", r.median_ms()),
                format!(
                    "{:.3}",
                    r.parse_ms.iter().sum::<f64>() / r.parse_ms.len() as f64
                ),
            )
        };
        out.push_str(&format!(
            "{},{},{},{:.3},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            csv_field(&r.path.display().to_string()),
            r.size,
            r.status.label().to_lowercase(),
            r.read_ms,
            min,
            median,
            mean,
            r.ghosts,
            r.input_events,
            r.truncated_inputs,
            r.car_samples,
            r.other_samples,
            r.race_time_ms.map(|t| t.to_string()).unwrap_or_default(),
            r.body_complete,
            r.warnings.len(),
            csv_field(&r.detail),
        ));
    }
    write_file(path, &out)
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn write_file(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

/// Collects replay/ghost files from a file or directory (recursively).
/// Symlinked directories are not descended into, so a link loop cannot hang
/// the scan; symlinked files are still picked up.
fn collect(path: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_dir() {
        let mut stack = vec![path.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let p = entry.path();
                let file_type = entry.file_type()?;
                if file_type.is_dir() {
                    stack.push(p);
                } else if (file_type.is_file() || file_type.is_symlink())
                    && p.is_file()
                    && is_replay(&p)
                {
                    out.push(p);
                }
            }
        }
        Ok(())
    } else if path.is_file() {
        out.push(path.to_path_buf());
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no such file or directory",
        ))
    }
}

fn is_replay(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    name.ends_with(".replay.gbx") || name.ends_with(".ghost.gbx")
}

/// True for warnings that mean the parser stopped inside a node, so every
/// field after that point is missing. Input-stream truncation ("ended early",
/// old game builds) is not in this class: the file structure parsed.
fn is_stopped_warning(warning: &str) -> bool {
    warning.contains("parsing stopped")
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    percentile(&sorted, 50.0)
}

/// Linear-interpolation percentile over a sorted slice.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = p / 100.0 * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn fmt_ms(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.2} s", ms / 1000.0)
    } else if ms >= 10.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{ms:.2} ms")
    }
}

fn fmt_bytes(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2} GiB", b / (1024.0 * 1024.0 * 1024.0))
    } else if b >= 1024.0 * 1024.0 {
        format!("{:.1} MiB", b / (1024.0 * 1024.0))
    } else if b >= 1024.0 {
        format!("{:.1} KiB", b / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn fmt_race(ms: i64) -> String {
    let ms = ms.max(0);
    let hours = ms / 3_600_000;
    let minutes = (ms / 60_000) % 60;
    let seconds = (ms / 1000) % 60;
    let millis = ms % 1000;
    format!("{hours}:{minutes:02}:{seconds:02}.{millis:03}")
}

fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn usage() -> String {
    "trackmaniare-bench - parse a replay collection, report failures, crashes and timings\n\
     \n\
     USAGE:\n    trackmaniare-bench [OPTIONS] <PATH>...\n\
     \n\
     ARGS:\n\
     \x20   <PATH>...    one or more replay (.Replay.Gbx) / ghost (.Ghost.Gbx)\n\
     \x20                files or directories, searched recursively\n\
     \n\
     OPTIONS:\n\
     \x20   -r, --repeat N      parse every file N times and report per-file parse medians (default 1)\n\
     \x20   -j, --json FILE     also write the full report as JSON\n\
     \x20   -c, --csv FILE      also write per-file results as CSV\n\
     \x20   -v, --verbose       print a line per file while the run proceeds\n\
     \x20   -q, --quiet         do not print failures as they are found\n\
     \x20       --simulate-panic N  panic while parsing the Nth file (self-test of the crash handling)\n\
     \x20   -h, --help          print this help\n\
     \n\
     EXIT CODE:\n\
     \x20   0 when every file was read and parsed in full (warnings are fine),\n\
     \x20   1 when any file could not be read, could not be parsed, panicked or\n\
     \x20   parsed only partially (parsing stopped inside the file)\n"
        .to_string()
}

fn parse_args() -> Result<Options, String> {
    let mut paths = Vec::new();
    let mut repeat = 1u32;
    let mut json = None;
    let mut csv = None;
    let mut verbose = false;
    let mut quiet = false;
    let mut simulate_panic = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "-h" | "--help" => {
                print!("{}", usage());
                std::process::exit(0);
            }
            "-r" | "--repeat" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--repeat needs a value".to_string())?;
                repeat = parse_repeat(value)?;
            }
            other if other.starts_with("--repeat=") => {
                repeat = parse_repeat(&other["--repeat=".len()..])?;
            }
            "-j" | "--json" => {
                i += 1;
                json = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--json needs a value".to_string())?,
                ));
            }
            other if other.starts_with("--json=") => {
                json = Some(PathBuf::from(&other["--json=".len()..]));
            }
            "-c" | "--csv" => {
                i += 1;
                csv = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| "--csv needs a value".to_string())?,
                ));
            }
            other if other.starts_with("--csv=") => {
                csv = Some(PathBuf::from(&other["--csv=".len()..]));
            }
            "-v" | "--verbose" => verbose = true,
            "-q" | "--quiet" => quiet = true,
            "--simulate-panic" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--simulate-panic needs a value".to_string())?;
                let n = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --simulate-panic value {value:?}"))?;
                if n == 0 {
                    return Err("--simulate-panic counts files from 1".to_string());
                }
                simulate_panic = Some(n);
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

    if paths.is_empty() {
        return Err("no input paths given".to_string());
    }
    Ok(Options {
        paths,
        repeat,
        json,
        csv,
        verbose,
        quiet,
        simulate_panic,
    })
}

fn parse_repeat(value: &str) -> Result<u32, String> {
    match value.parse::<u32>() {
        Ok(n) if n >= 1 => Ok(n),
        _ => Err(format!(
            "--repeat must be a positive integer, got {value:?}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_warnings_are_recognised() {
        for warning in [
            "replay: unknown non-skippable chunk 0x0911F000, replay parsing stopped",
            "replay parsing stopped early: unexpected end of data while reading chunk payload",
            "ghost#1: unknown non-skippable chunk 0x0, ghost parsing stopped",
            "ghost parsing stopped early: invalid data",
        ] {
            assert!(is_stopped_warning(warning), "{warning}");
        }
        for warning in [
            "note: skipped auxiliary node #3 of class 0x03079000",
            "ghost#1: input stream ended early (1579 of 1594 ticks stored, archive version 12)",
            "body is incomplete: header expects 19628 bytes, file provided less",
        ] {
            assert!(!is_stopped_warning(warning), "{warning}");
        }
    }

    #[test]
    fn group_separates_thousands() {
        assert_eq!(group(0), "0");
        assert_eq!(group(999), "999");
        assert_eq!(group(1_000), "1,000");
        assert_eq!(group(2_255), "2,255");
        assert_eq!(group(1_234_567), "1,234,567");
    }

    #[test]
    fn speeds_distinguish_parse_time_from_wall_time() {
        // A synthetic file whose read dominates: 1 MiB, 300 ms read,
        // 100 ms parse, in a 400 ms run. Parsing alone runs at 10 MiB/s,
        // the whole run at 2.5 MiB/s. Both rates must show up separately.
        let mut r = FileResult::new(Path::new("a.Replay.Gbx"));
        r.size = 1024 * 1024;
        r.read_ms = 300.0;
        r.parse_ms = vec![100.0];
        let s = Summary::from(&[r], 400.0);

        assert!(
            (s.parse_throughput_mib_s - 10.0).abs() < 1e-9,
            "1 MiB in 100 ms is 10 MiB/s of parsing, got {}",
            s.parse_throughput_mib_s
        );
        assert!(
            (s.end_to_end_throughput_mib_s - 2.5).abs() < 1e-9,
            "1 MiB in a 400 ms run is 2.5 MiB/s end to end, got {}",
            s.end_to_end_throughput_mib_s
        );
    }
}
