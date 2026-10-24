//! TICK TAS input script rendering.
//!
//! TICK — the Trackmania 2020 TAS client (<https://tmtas.exchange/tas-tools>) —
//! reads a text script of one action per line: `<time_ms> <action> <value>`,
//! or `<time_ms> respawn` for actions without a value. Pedals and steering
//! are states that hold until the next line for that action; respawns are
//! one-off events.
//!
//! The action vocabulary is small. This writer emits only what was verified
//! against TICK 2.1.0 itself (its data service's parser messages and diff
//! model, plus real scripts, including replay exports):
//!
//! | action     | value           | meaning                                  |
//! | ---------- | --------------- | ---------------------------------------- |
//! | `seed`     | integer         | map validation seed, pinned at time 0    |
//! | `steer`    | integer -127..127 | steering axis, held until changed      |
//! | `accel`    | `0` or `1`      | accelerate pedal                         |
//! | `brake`    | `0` or `1`      | brake pedal                              |
//! | `respawn`  | none            | respawn                                  |
//! | `srespawn` | none            | standing respawn (velocity reset)        |
//!
//! Everything else a TM2020 input stream can carry (horn, action slots, jump,
//! free look, camera, mouse) has no verified keyword, so those events are
//! dropped and counted in [`TasScript::dropped_events`]; the UI can say so
//! instead of silently writing a script that looks complete. The format also
//! has a `flags` action, but its bits are opaque (TICK's data service stores
//! the word as-is and accepts any 32-bit value), so nothing is guessed into
//! it.

use std::fmt::Write as _;

use crate::inputs::Event;
use crate::model::{Extraction, GhostInputs};

/// One ghost's inputs rendered as a TICK script.
#[derive(Debug, Clone, Default)]
pub struct TasScript {
    /// The script text, newline-terminated. Ready to paste into TICK.
    pub text: String,
    /// Number of lines written.
    pub lines: usize,
    /// Input events with no TICK representation that were left out.
    pub dropped_events: usize,
}

/// Renders ghost `ghost`'s inputs as a TICK script. `None` when the ghost has
/// no TM2020 input stream (legacy inputs, or no inputs at all); TICK only
/// drives Trackmania 2020.
pub fn tas_script(ex: &Extraction, ghost: usize) -> Option<TasScript> {
    let info = ex.ghosts.get(ghost)?;
    let inputs = info.inputs.as_ref()?;
    let GhostInputs::Trackmania2020 { events, .. } = inputs else {
        return None;
    };

    let mut script = TasScript::default();
    // A script restarts from the neutral state (steer 0, pedals released), so
    // pin the recording's validation seed on the first line: replayed inputs
    // then meet the same randomness the recording did. TICK itself writes
    // exports this way (`0 seed <value>` first).
    if let Some(seed) = info.validation.validation_seed {
        let _ = writeln!(script.text, "0 seed {seed}");
        script.lines += 1;
    }

    // The decoded stream only carries changes, so a field's first event is the
    // state the recording began in; if it already equals the neutral state
    // there is nothing to write. TICK lays same-tick writes out pedals first,
    // steering last, so lines are gathered and ordered per timestamp instead
    // of written in stream order.
    let mut steer = 0i8;
    let mut accel = false;
    let mut brake = false;

    struct Line {
        time_ms: i32,
        /// 0: respawn/pedals in stream order, 1: steer, written last.
        order: u8,
        text: String,
    }
    let mut lines: Vec<Line> = Vec::new();

    // The respawn bits stay set while the input is held; a run of set bits
    // in consecutive ticks is one press, so only the first tick of a run is
    // written.
    let mut respawn_tick: Option<u32> = None;
    let mut secondary_tick: Option<u32> = None;

    for event in events {
        match &event.event {
            Event::Steer(v) => {
                if *v != steer {
                    lines.push(Line {
                        time_ms: event.time_ms,
                        order: 1,
                        text: format!("{} steer {v}", event.time_ms),
                    });
                    steer = *v;
                }
            }
            Event::Accelerate(v) => {
                if *v != accel {
                    lines.push(Line {
                        time_ms: event.time_ms,
                        order: 0,
                        text: format!("{} accel {}", event.time_ms, u8::from(*v)),
                    });
                    accel = *v;
                }
            }
            Event::Brake(v) => {
                if *v != brake {
                    lines.push(Line {
                        time_ms: event.time_ms,
                        order: 0,
                        text: format!("{} brake {}", event.time_ms, u8::from(*v)),
                    });
                    brake = *v;
                }
            }
            Event::Respawn => {
                let held = respawn_tick == Some(event.tick.saturating_sub(1));
                respawn_tick = Some(event.tick);
                if !held {
                    lines.push(Line {
                        time_ms: event.time_ms,
                        order: 0,
                        text: format!("{} respawn", event.time_ms),
                    });
                }
            }
            Event::SecondaryRespawn => {
                let held = secondary_tick == Some(event.tick.saturating_sub(1));
                secondary_tick = Some(event.tick);
                if !held {
                    lines.push(Line {
                        time_ms: event.time_ms,
                        order: 0,
                        text: format!("{} srespawn", event.time_ms),
                    });
                }
            }
            _ => script.dropped_events += 1,
        }
    }

    lines.sort_by_key(|l| (l.time_ms, l.order));
    for line in lines {
        let _ = writeln!(script.text, "{}", line.text);
        script.lines += 1;
    }

    Some(script)
}

/// Indices of the ghosts that have a TICK script, in ghost order.
pub fn ghosts_with_tas(ex: &Extraction) -> Vec<usize> {
    (0..ex.ghosts.len())
        .filter(|g| tas_script(ex, *g).is_some())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inputs::InputEvent;
    use crate::model::{GhostInfo, ValidationInfo};

    fn ghost_with(events: Vec<InputEvent>) -> GhostInfo {
        ghost_with_seed(events, None)
    }

    fn ghost_with_seed(events: Vec<InputEvent>, seed: Option<i32>) -> GhostInfo {
        GhostInfo {
            inputs: Some(GhostInputs::Trackmania2020 {
                version: 12,
                start_offset_ms: None,
                ticks: 100,
                truncated: false,
                events,
            }),
            validation: ValidationInfo {
                validation_seed: seed,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn at(tick: u32, event: Event) -> InputEvent {
        InputEvent {
            time_ms: tick as i32 * 10,
            tick,
            event,
        }
    }

    fn script_of(events: Vec<InputEvent>) -> TasScript {
        let mut ex = Extraction::default();
        ex.ghosts.push(ghost_with(events));
        tas_script(&ex, 0).unwrap()
    }

    #[test]
    fn renders_verified_actions_and_drops_the_rest() {
        let script = script_of(vec![
            at(0, Event::Accelerate(true)),
            at(0, Event::Steer(-127)),
            at(3, Event::Steer(0)),
            at(3, Event::Horn(true)),
            at(4, Event::ActionSlot(2, true)),
            at(5, Event::Horn(false)),
            at(6, Event::Brake(true)),
            at(6, Event::Respawn),
            at(7, Event::SecondaryRespawn),
            at(9, Event::Accelerate(false)),
        ]);
        assert_eq!(
            script.text,
            "0 accel 1\n\
             0 steer -127\n\
             30 steer 0\n\
             60 brake 1\n\
             60 respawn\n\
             70 srespawn\n\
             90 accel 0\n"
        );
        assert_eq!(script.lines, 7);
        assert_eq!(script.dropped_events, 3);
    }

    #[test]
    fn held_respawn_bits_render_once() {
        let script = script_of(vec![
            at(10, Event::Respawn),
            at(11, Event::Respawn),
            at(12, Event::Respawn),
            at(20, Event::Respawn),
            at(21, Event::SecondaryRespawn),
            at(22, Event::SecondaryRespawn),
        ]);
        assert_eq!(script.text, "100 respawn\n200 respawn\n210 srespawn\n");
        assert_eq!(script.dropped_events, 0);
    }

    #[test]
    fn seed_line_pins_the_validation_seed_first() {
        let script = script_of(vec![
            at(0, Event::Steer(0)),
            at(0, Event::Accelerate(true)),
            at(0, Event::Brake(false)),
            at(2, Event::Steer(-20)),
            at(2, Event::Brake(true)),
        ]);
        // No seed recorded: no seed line. Initial states that equal the
        // script's neutral defaults are not written; the initial `accel 1`
        // is. At the same timestamp pedals come before steering.
        assert_eq!(script.text, "0 accel 1\n20 brake 1\n20 steer -20\n");

        let mut ex = Extraction::default();
        ex.ghosts.push(ghost_with_seed(
            vec![at(0, Event::Accelerate(true)), at(0, Event::Steer(-127))],
            Some(28024833),
        ));
        let seeded = tas_script(&ex, 0).unwrap();
        assert_eq!(seeded.text, "0 seed 28024833\n0 accel 1\n0 steer -127\n");
        assert_eq!(seeded.lines, 3);
    }

    #[test]
    fn initial_steering_is_written_when_not_neutral() {
        // A run that begins mid-steer must say so: the neutral initial state
        // would not.
        let script = script_of(vec![at(0, Event::Steer(-127)), at(1, Event::Steer(0))]);
        assert_eq!(script.text, "0 steer -127\n10 steer 0\n");
    }

    #[test]
    fn legacy_and_missing_inputs_have_no_script() {
        let mut ex = Extraction::default();
        ex.ghosts.push(GhostInfo::default());
        assert!(tas_script(&ex, 0).is_none());
        assert!(tas_script(&ex, 5).is_none());
        assert!(ghosts_with_tas(&ex).is_empty());

        ex.ghosts.push(GhostInfo {
            inputs: Some(GhostInputs::Legacy {
                control_names: vec!["Accelerate".into()],
                entries: Vec::new(),
            }),
            ..Default::default()
        });
        assert!(tas_script(&ex, 1).is_none());
    }

    #[test]
    fn scripts_are_offered_per_ghost() {
        let mut ex = Extraction::default();
        ex.ghosts
            .push(ghost_with(vec![at(0, Event::Accelerate(true))]));
        ex.ghosts.push(GhostInfo::default());
        ex.ghosts.push(ghost_with(vec![at(0, Event::Brake(true))]));
        assert_eq!(ghosts_with_tas(&ex), vec![0, 2]);
        assert_eq!(tas_script(&ex, 2).unwrap().text, "0 brake 1\n");
    }
}
