//! CSV rendering of the extracted timelines.

use crate::inputs::{ControlEntry, Event, tick_time_ms};
use crate::model::{Extraction, GhostInputs};
use crate::record::RecordData;

/// Renders every input event of every ghost as CSV.
pub fn inputs_csv(ex: &Extraction) -> String {
    let mut out = String::from("ghost,time_ms,tick,type,value\n");
    for (gi, ghost) in ex.ghosts.iter().enumerate() {
        let Some(inputs) = &ghost.inputs else {
            continue;
        };
        match inputs {
            GhostInputs::Trackmania2020 { events, .. } => {
                for e in events {
                    out.push_str(&format!(
                        "{gi},{},{},{},{}\n",
                        e.time_ms,
                        e.tick,
                        event_name(&e.event),
                        event_value(&e.event)
                    ));
                }
            }
            GhostInputs::Legacy { entries, .. } => {
                for e in entries {
                    out.push_str(&format!(
                        "{gi},{},,{},{}\n",
                        e.time_ms,
                        csv_escape(&e.control),
                        legacy_value(e)
                    ));
                }
            }
        }
    }
    out
}

/// Renders a dense per-tick input timeline (one row per 10 ms tick) with the
/// state in effect at that tick, which is handy for plotting.
pub fn inputs_csv_dense(ex: &Extraction) -> String {
    let mut out =
        String::from("ghost,tick,time_ms,steer,accelerate,brake,horn,respawn,mouse_x,mouse_y\n");
    for (gi, ghost) in ex.ghosts.iter().enumerate() {
        let Some(GhostInputs::Trackmania2020 {
            ticks,
            events,
            start_offset_ms,
            truncated,
            ..
        }) = &ghost.inputs
        else {
            continue;
        };
        let offset = start_offset_ms.unwrap_or(0);
        // When the stream ended early the declared tick count is meaningless,
        // so only emit rows up to the last decoded tick.
        let ticks = if *truncated {
            events.last().map(|e| e.tick.saturating_add(1)).unwrap_or(0)
        } else {
            *ticks
        };
        let mut steer = 0i32;
        let mut accel = false;
        let mut brake = false;
        let mut horn = false;
        let mut mouse_x: Option<u16> = None;
        let mut mouse_y: Option<u16> = None;
        let mut i = 0usize;
        for tick in 0..ticks {
            let time = tick_time_ms(tick, offset);
            let mut respawn = false;
            while i < events.len() && events[i].time_ms <= time {
                match &events[i].event {
                    Event::Steer(v) => steer = *v as i32,
                    Event::Accelerate(v) => accel = *v,
                    Event::Brake(v) => brake = *v,
                    Event::Horn(v) => horn = *v,
                    Event::Respawn => respawn = true,
                    Event::Mouse(x, y) => {
                        mouse_x = Some(*x);
                        mouse_y = Some(*y);
                    }
                    _ => {}
                }
                i += 1;
            }
            out.push_str(&format!(
                "{gi},{tick},{time},{steer},{accel},{brake},{horn},{respawn},{},{}\n",
                mouse_x.map(|v| v.to_string()).unwrap_or_default(),
                mouse_y.map(|v| v.to_string()).unwrap_or_default()
            ));
        }
    }
    out
}

/// Renders the car sample stream of every ghost as CSV. A combined record
/// written at replay level (merge tooling) is rendered with ghost index -1.
pub fn samples_csv(ex: &Extraction) -> String {
    let mut out = String::from(
        "ghost,time_ms,x,y,z,speed_kmh,rot_angle,rot_heading,rot_pitch,steer,steer_byte,gas,brake,rpm,gear,gear_raw,turbo,is_turbo,state,ground_mode\n",
    );
    for (gi, ghost) in ex.ghosts.iter().enumerate() {
        if let Some(record) = &ghost.record_data {
            push_sample_rows(&mut out, gi as i32, record);
        }
    }
    if let Some(record) = &ex.replay.record_data {
        push_sample_rows(&mut out, -1, record);
    }
    out
}

fn push_sample_rows(out: &mut String, ghost: i32, record: &RecordData) {
    for list in &record.lists {
        for sample in &list.samples {
            let Some(v) = &sample.vehicle else { continue };
            out.push_str(&format!(
                "{ghost},{},{:.3},{:.3},{:.3},{:.2},{:.4},{:.4},{:.4},{:.4},{},{:.4},{:.4},{},{:.3},{},{},{},{},{}\n",
                sample.time_ms,
                v.position[0],
                v.position[1],
                v.position[2],
                v.speed * 3.6,
                v.rotation_angle,
                v.rotation_axis_heading,
                v.rotation_axis_pitch,
                v.steer,
                ((v.steer / 2.0 + 0.5) * 255.0).round(),
                v.gas,
                v.brake,
                v.rpm,
                v.gear,
                (v.gear * 4.0 + 1.0).round(),
                v.turbo_time,
                v.is_turbo,
                v.vehicle_state,
                v.ground_mode
            ));
        }
    }
}

fn event_name(e: &Event) -> &'static str {
    match e {
        Event::Steer(_) => "steer",
        Event::Accelerate(_) => "accelerate",
        Event::Brake(_) => "brake",
        Event::Horn(_) => "horn",
        Event::Respawn => "respawn",
        Event::SecondaryRespawn => "secondary_respawn",
        Event::ActionSlot(_, _) => "action_slot",
        Event::Jump(_) => "jump",
        Event::FreeLook(_) => "free_look",
        Event::Camera2(_) => "camera2",
        Event::GunTrigger(_) => "gun_trigger",
        Event::Action(_) => "action",
        Event::Strafe(_) => "strafe",
        Event::Walk(_) => "walk",
        Event::Vertical(_) => "vertical",
        Event::Horizontal(_) => "horizontal",
        Event::Mouse(_, _) => "mouse",
    }
}

fn event_value(e: &Event) -> String {
    match e {
        Event::Steer(v) => format!("{v}"),
        Event::Mouse(x, y) => csv_escape(&format!("{x},{y}")),
        Event::ActionSlot(slot, v) => format!("{slot}={v}"),
        Event::Strafe(v) | Event::Walk(v) | Event::Vertical(v) | Event::Horizontal(v) => {
            format!("{v}")
        }
        Event::Respawn | Event::SecondaryRespawn => String::new(),
        other => format!("{}", bool_of(other)),
    }
}

fn bool_of(e: &Event) -> bool {
    match e {
        Event::Accelerate(v)
        | Event::Brake(v)
        | Event::Horn(v)
        | Event::Jump(v)
        | Event::FreeLook(v)
        | Event::Camera2(v)
        | Event::GunTrigger(v)
        | Event::Action(v) => *v,
        Event::ActionSlot(_, v) => *v,
        _ => false,
    }
}

fn legacy_value(e: &ControlEntry) -> String {
    format!("{}/{}", e.onoff, e.value)
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Compact human-readable timeline of a single replay (used for stdout).
pub fn summary_text(ex: &Extraction) -> String {
    let mut s = String::new();
    let attr = |key: &str| -> Option<&String> {
        ex.replay
            .xml_attributes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    };
    s.push_str(&format!("file:            {}\n", ex.file));
    s.push_str(&format!(
        "container:       class 0x{:08X}, body {} ({} bytes, {}), complete: {}\n",
        ex.container.class_id,
        ex.container.body_compression,
        ex.container.body_size,
        ex.container.body_uncompressed_size,
        ex.container.body_complete
    ));
    let map_name = attr("name").cloned().or_else(|| ex.replay.nickname.clone());
    s.push_str(&format!(
        "map:             {} by {} [{}]\n",
        map_name.clone().unwrap_or_else(|| "?".into()),
        attr("author").cloned().unwrap_or_else(|| "?".into()),
        ex.replay.map_uid.clone().unwrap_or_else(|| "?".into())
    ));
    if let Some(best) = attr("best") {
        s.push_str(&format!(
            "time:            {} ms{}\n",
            best,
            attr("respawns")
                .map(|r| format!(" (respawns {r})"))
                .unwrap_or_default()
        ));
    } else if let Some(t) = ex.replay.time_ms {
        s.push_str(&format!("time:            {t} ms\n"));
    }
    for (k, label) in [
        ("exever", "game version"),
        ("exebuild", "exe build"),
        ("title", "title"),
        ("envir", "environment"),
        ("mood", "mood"),
        ("maptype", "map type"),
        ("mapstyle", "map style"),
        ("displaycost", "display cost"),
        ("playermodel", "player model"),
    ] {
        if let Some(v) = attr(k) {
            s.push_str(&format!("{label:<17}{v}\n"));
        }
    }
    if let Some(size) = ex.replay.embedded_map_size {
        s.push_str(&format!("embedded map:    {size} bytes\n"));
    }
    s.push_str(&format!("ghosts:          {}\n", ex.ghosts.len()));
    for (i, g) in ex.ghosts.iter().enumerate() {
        s.push_str(&format!("  ghost {i}:\n"));
        s.push_str(&format!(
            "    nickname:     {}\n",
            g.nickname.clone().unwrap_or_else(|| "?".into())
        ));
        if let Some(login) = &g.login {
            s.push_str(&format!("    login:        {login}\n"));
        }
        s.push_str(&format!("    player model: {}\n", g.player_model_text()));
        if let Some(t) = g.race_time_ms {
            s.push_str(&format!("    race time:    {t} ms\n"));
        }
        if let Some(r) = g.respawns {
            s.push_str(&format!("    respawns:     {r}\n"));
        }
        if !g.checkpoints.is_empty() {
            let times: Vec<String> = g.checkpoints.iter().map(|(t, _)| t.to_string()).collect();
            s.push_str(&format!("    checkpoints:  {}\n", times.join(", ")));
        }
        if let (Some(a), Some(b)) = (g.walltime_start, g.walltime_end) {
            s.push_str(&format!(
                "    walltime:     {} .. {}\n",
                unix_to_string(a),
                unix_to_string(b)
            ));
        }
        if let Some(v) = &g.validation.exe_version {
            s.push_str(&format!("    exe version:  {v}\n"));
        }
        match &g.inputs {
            Some(GhostInputs::Trackmania2020 {
                version,
                ticks,
                events,
                truncated,
                ..
            }) => s.push_str(&format!(
                "    inputs:       TM2020 v{version}, {ticks} ticks, {} events{}\n",
                events.len(),
                if *truncated {
                    " (stream truncated)"
                } else {
                    ""
                }
            )),
            Some(GhostInputs::Legacy {
                entries,
                control_names,
            }) => s.push_str(&format!(
                "    inputs:       legacy control entries, {} events, {} controls\n",
                entries.len(),
                control_names.len()
            )),
            None => s.push_str("    inputs:       none found\n"),
        }
        s.push_str(&format!(
            "    samples:      {} vehicle states\n",
            g.sample_count()
        ));
    }
    if let Some(record) = &ex.replay.record_data {
        s.push_str(&format!(
            "replay record:   {} vehicle states, {} other states ({} .. {} ms)\n",
            record.vehicle_sample_count(),
            record.other_sample_count(),
            record.start_ms.unwrap_or(0),
            record.end_ms.unwrap_or(0)
        ));
    }
    if !ex.warnings.is_empty() {
        s.push_str("warnings:\n");
        for w in &ex.warnings {
            s.push_str(&format!("  - {w}\n"));
        }
    }
    s
}

/// Renders a unix timestamp as an UTC calendar date, without pulling in a
/// date library.
fn unix_to_string(v: i64) -> String {
    const SECS_PER_DAY: i64 = 86_400;
    let days = v.div_euclid(SECS_PER_DAY);
    let secs = v.rem_euclid(SECS_PER_DAY);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        secs / 3600,
        (secs / 60) % 60,
        secs % 60
    )
}

/// Days since the unix epoch to a proleptic Gregorian calendar date
/// (Howard Hinnant's `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inputs::InputEvent;
    use crate::model::GhostInfo;

    #[test]
    fn unix_timestamps_render_as_dates() {
        assert_eq!(unix_to_string(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(unix_to_string(1_701_234_567), "2023-11-29 05:09:27 UTC");
        assert_eq!(unix_to_string(951_782_400), "2000-02-29 00:00:00 UTC"); // leap day
    }

    #[test]
    fn dense_csv_is_bounded_by_decoded_ticks() {
        // A truncated stream that declares 4 billion ticks must not produce a
        // per-tick row for each of them.
        let mut ex = Extraction::default();
        let events = vec![
            InputEvent {
                time_ms: 0,
                tick: 0,
                event: Event::Accelerate(true),
            },
            InputEvent {
                time_ms: 20,
                tick: 2,
                event: Event::Brake(true),
            },
        ];
        ex.ghosts.push(GhostInfo {
            inputs: Some(GhostInputs::Trackmania2020 {
                version: 12,
                start_offset_ms: None,
                ticks: 4_000_000_000,
                truncated: true,
                events,
            }),
            ..Default::default()
        });
        let csv = inputs_csv_dense(&ex);
        assert_eq!(csv.lines().count(), 4, "header + 3 decoded ticks");
        assert!(csv.ends_with("0,2,20,0,true,true,false,false,,\n"));
    }

    #[test]
    fn mouse_events_are_csv_escaped() {
        assert_eq!(event_value(&Event::Mouse(12, 34)), "\"12,34\"");
    }
}
