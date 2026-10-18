//! Prints the chunk inventory of a replay: every chunk id, its offset, whether
//! the payload size is declared, and whether this tool understood it.
//!
//! Useful when adding support for a chunk that a new game version introduced.
//!
//! ```text
//! cargo run --release --example chunks -- "My Replays/some.Replay.Gbx"
//! ```

use trackmaniare::parse_replay;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!("usage: chunks <replay.Replay.Gbx>");
            std::process::exit(2);
        }
    };
    let data = std::fs::read(&path).expect("read replay");
    let ex = parse_replay(&data, &path, false).expect("parse replay");

    println!("{} ({} bytes)", ex.file, ex.file_size);
    println!(
        "class 0x{:08X}, gbx v{}, body {} ({}/{} bytes){}",
        ex.container.class_id,
        ex.container.gbx_version,
        if ex.container.body_compression == 'C' {
            "LZO"
        } else {
            "raw"
        },
        ex.container.body_size,
        ex.container.body_uncompressed_size,
        if ex.container.body_complete {
            ""
        } else {
            " [INCOMPLETE]"
        }
    );
    println!();
    println!(
        "{:<10} {:<12} {:>9} {:>9}  disposition",
        "node", "chunk", "offset", "size"
    );
    for c in &ex.chunks {
        println!(
            "{:<10} 0x{:08X} {:>9} {:>9}  {}",
            c.node,
            c.id,
            c.offset,
            c.size.map(|s| s.to_string()).unwrap_or_else(|| "-".into()),
            c.disposition.as_str()
        );
    }
    println!();
    for (i, ghost) in ex.ghosts.iter().enumerate() {
        let inputs = ghost
            .inputs
            .as_ref()
            .map(|i| format!("{} input events", i.event_count()))
            .unwrap_or_else(|| "no inputs".into());
        println!(
            "ghost {i}: {} | {} samples | {inputs}",
            ghost.nickname.clone().unwrap_or_default(),
            ghost.sample_count()
        );
    }
    if !ex.warnings.is_empty() {
        println!();
        println!("warnings:");
        for w in &ex.warnings {
            println!("  - {w}");
        }
    }
}
