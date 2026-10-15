//! CSV rendering of the extracted timelines.

use crate::inputs::{ControlEntry, Event, InputEvent};
use crate::model::{Extraction, GhostInputs};

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
            ..
        }) = &ghost.inputs
        else {
            continue;
        };
        let offset = start_offset_ms.unwrap_or(0);
        let mut steer = 0i32;
        let mut accel = false;
        let mut brake = false;
        let mut horn = false;
        let mut mouse_x: Option<u16> = None;
        let mut mouse_y: Option<u16> = None;
        let mut i = 0usize;
        for tick in 0..*ticks {
            let time = (tick as i32) * 10 + offset;
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

/// Renders the car sample stream of every ghost as CSV.
pub fn samples_csv(ex: &Extraction) -> String {
    let mut out = String::from(
        "ghost,time_ms,x,y,z,speed_kmh,rot_angle,rot_heading,rot_pitch,steer,steer_byte,gas,brake,rpm,gear,gear_raw,turbo,is_turbo,state,ground_mode\n",
    );
    for (gi, ghost) in ex.ghosts.iter().enumerate() {
        let Some(record) = &ghost.record_data else {
            continue;
        };
        for list in &record.lists {
            for sample in &list.samples {
                let Some(v) = &sample.vehicle else { continue };
                out.push_str(&format!(
                    "{gi},{},{:.3},{:.3},{:.3},{:.2},{:.4},{:.4},{:.4},{:.4},{},{:.4},{:.4},{},{:.3},{},{},{},{},{}\n",
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
    out
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
        Event::Mouse(x, y) => format!("{x},{y}"),
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
    if !ex.warnings.is_empty() {
        s.push_str("warnings:\n");
        for w in &ex.warnings {
            s.push_str(&format!("  - {w}\n"));
        }
    }
    s
}

fn unix_to_string(v: i64) -> String {
    match v {
        0 => "0".into(),
        v => format!("unix:{v}"),
    }
}

fn _unused(_: &InputEvent) {}
