//! Prints the raw vehicle samples of the first ghost next to the decoded
//! values, for verifying the record layout against real replays.

use trackmaniare::parse_replay;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: sample <replay.Replay.Gbx>");
    let data = std::fs::read(&path).expect("read");
    let ex = parse_replay(&data, &path, false).expect("parse");

    for ghost in &ex.ghosts {
        let Some(record) = &ghost.record_data else {
            continue;
        };
        for list in &record.lists {
            if list.class_id != Some(0x0A01_8000) {
                continue;
            }
            println!(
                "list type={} samples={} record_len={}",
                list.type_index,
                list.samples.len(),
                list.samples.first().map(|s| s.raw.len()).unwrap_or(0)
            );
            for s in list.samples.iter().take(3) {
                println!(
                    "  t={} raw={}",
                    s.time_ms,
                    hex(&s.raw[..s.raw.len().min(80)])
                );
                if let Some(v) = &s.vehicle {
                    println!(
                        "     pos=({:.3},{:.3},{:.3}) speed={:.3} m/s steer={:.4} gas={:.3} brake={:.3} rpm={} gear={:.2}",
                        v.position[0],
                        v.position[1],
                        v.position[2],
                        v.speed,
                        v.steer,
                        v.gas,
                        v.brake,
                        v.rpm,
                        v.gear
                    );
                }
            }
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
