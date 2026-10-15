//! Ghost node parsing (`CGameCtnGhost`, class 0x03092000).

use crate::error::Result;
use crate::gbx;
use crate::inputs;
use crate::model::{ChunkDisposition, ChunkRecord, Extraction, GhostInfo, GhostInputs};
use crate::reader::{Fileref, Ident, Reader};
use crate::record;

pub const CLASS_CGAME_CTN_GHOST: u32 = 0x0309_2000;

/// Ghost chunks that are written with a SKIP marker and documented as "ignore"
/// (anticheat blobs, match separators, pixel arrays, ...). They carry nothing
/// we need, so they are stepped over on purpose rather than being unknown.
const GHOST_IGNORED: [u32; 10] = [
    0x0303_F007,
    0x0309_201B,
    0x0309_201F,
    0x0309_2021,
    0x0309_2022,
    0x0309_2023,
    0x0309_2024,
    0x0309_2027,
    0x0309_202B,
    0x0309_202E,
];

/// Parses one ghost node (the reader is positioned at the node's first chunk).
pub fn parse_ghost(
    r: &mut Reader<'_>,
    node_index: usize,
    ex: &mut Extraction,
) -> Result<GhostInfo> {
    let mut info = GhostInfo {
        node_index,
        ..Default::default()
    };
    let node_label = format!("ghost#{node_index}");

    while r.remaining() >= 4 {
        let chunk = match gbx::read_chunk(r)? {
            Some(c) => c,
            None => break,
        };
        let size = chunk.skippable_size;
        let data_start = chunk.data_start;
        ex.chunks.push(ChunkRecord {
            node: node_label.clone(),
            id: chunk.id,
            offset: data_start,
            size,
            disposition: ChunkDisposition::Parsed,
        });

        match chunk.id {
            // Skippable chunks declare their payload size, so a parse failure
            // can always be recovered from by skipping the payload.
            0x0309_2000 => parse_guarded(
                r,
                ex,
                size,
                data_start,
                &node_label,
                &mut info,
                &|sub, info| parse_identity(sub, info).map(|_| Vec::new()),
            )?,
            0x0303_F005 | 0x0303_F006 => {
                if chunk.id == 0x0303_F006 {
                    let _is_replaying = r.u32()?;
                }
                let uncompressed = r.u32()? as usize;
                let compressed = r.u32()? as usize;
                let data = r.take(compressed, "ghost record data")?;
                match record::parse_ghost_record_blob(data, uncompressed) {
                    Ok(blob) => info.legacy_record_blob = Some(blob),
                    Err(e) => ex.warnings.push(format!("{node_label} record data: {e}")),
                }
            }
            0x0309_2005 => info.race_time_ms = Some(r.i32()?),
            0x0309_2008 => info.respawns = Some(r.i32()?),
            0x0309_200A => info.stunts_score = Some(r.i32()?),
            0x0309_200B => {
                let n = r.u32()?;
                for _ in 0..n.min(4096) {
                    let time = r.i32()?;
                    let stunts = r.i32()?;
                    info.checkpoints.push((time, stunts));
                }
            }
            0x0309_200C => {
                let _ = r.u32()?;
            }
            0x0309_200E => info.ghost_uid = Some(r.id_string()?),
            0x0309_200F => info.login = Some(r.string()?),
            0x0309_2010 => info.validate_challenge_uid = Some(r.id_string()?),
            0x0309_2011 | 0x0309_2019 | 0x0309_2025 => parse_guarded(
                r,
                ex,
                size,
                data_start,
                &node_label,
                &mut info,
                &|sub: &mut Reader<'_>, info: &mut GhostInfo| {
                    parse_validation(sub, chunk.id, info).map(|_| Vec::new())
                },
            )?,
            0x0309_2013 => {
                let _ = r.u32()?;
                let _ = r.u32()?;
            }
            0x0309_2014 => info.ghost_version = Some(r.i32()?),
            0x0309_2017 => {
                let n = r.u32()?;
                for _ in 0..n.min(4096) {
                    let f = r.fileref()?;
                    info.skin_pack_descs.push(f);
                }
                info.nickname = Some(r.string()?);
                info.avatar_name = Some(r.string()?);
            }
            0x0309_2018 => {
                let ident: Ident = r.ident()?;
                if info.player_model.is_none() {
                    info.player_model = Some(ident);
                }
            }
            0x0309_201A => {
                let _ = r.u32()?;
            }
            0x0309_201C => {
                r.skip(32)?;
            }
            0x0309_2026 => {
                r.skip(16)?;
            }
            0x0309_2028 => {
                if info.validation.events_duration_ms.unwrap_or(0) != 0 {
                    info.validation.title_id = Some(r.string()?);
                    info.validation.title_checksum = Some(hex(r.take(32, "title checksum")?));
                }
            }
            0x0309_2029 => {
                let _version = r.u32()?;
                let _ = r.i32()?;
                let _ = r.string()?;
                let _ = r.string()?;
                let _ = r.string()?;
            }
            0x0309_202A => {
                let _ = r.i32()?;
                let _ = r.i32()?;
            }
            0x0309_202C => {
                let version = r.u32()?;
                if version >= 1 {
                    info.walltime_start = r.nullable_unix_time32()?;
                    info.walltime_end = r.nullable_unix_time32()?;
                }
            }
            0x0309_202D => {
                parse_guarded(
                    r,
                    ex,
                    size,
                    data_start,
                    &node_label,
                    &mut info,
                    &|sub, info| parse_chunk_2d(sub, info).map(|_| Vec::new()),
                )?;
            }
            0x0309_201D => {
                parse_guarded(
                    r,
                    ex,
                    size,
                    data_start,
                    &node_label,
                    &mut info,
                    &|sub, info| parse_inputs_chunk(sub, info, &node_label),
                )?;
            }
            _ => match size {
                Some(n) => {
                    r.skip(n)?;
                    if let Some(last) = ex.chunks.last_mut() {
                        last.disposition = if GHOST_IGNORED.contains(&chunk.id) {
                            ChunkDisposition::Skipped
                        } else {
                            ChunkDisposition::Unknown
                        };
                    }
                }
                None => {
                    mark_unhandled(ex);
                    match gbx::resync_to_next_chunk(r, 1024)? {
                        Some(skipped) => ex.warnings.push(format!(
                            "{node_label}: unknown non-skippable chunk 0x{:08X}, skipped {skipped} bytes to resynchronise",
                            chunk.id
                        )),
                        None => {
                            ex.warnings.push(format!(
                                "{node_label}: unknown non-skippable chunk 0x{:08X}, ghost parsing stopped",
                                chunk.id
                            ));
                            break;
                        }
                    }
                }
            },
        }
    }

    if info.nickname.is_none() {
        info.nickname = info.login.clone();
    }

    Ok(info)
}

/// Runs a chunk parser, falling back to skipping the payload when the chunk
/// carried a SKIP marker and the parse failed.
#[allow(clippy::too_many_arguments)]
fn parse_guarded(
    r: &mut Reader<'_>,
    ex: &mut Extraction,
    size: Option<usize>,
    data_start: usize,
    node_label: &str,
    info: &mut GhostInfo,
    parse: &dyn Fn(&mut Reader<'_>, &mut GhostInfo) -> Result<Vec<String>>,
) -> Result<()> {
    let outcome = match size {
        Some(n) => {
            let mut sub = r.sub(n, "chunk payload")?;
            let result = parse(&mut sub, info);
            r.set_ids(sub.ids().clone());
            result
        }
        None => parse(r, info),
    };
    match outcome {
        Ok(warnings) => ex.warnings.extend(warnings),
        Err(e) => match size {
            Some(n) => {
                r.set_pos(data_start);
                r.skip(n)?;
                mark_unhandled(ex);
                let id = ex.chunks.last().map(|c| c.id).unwrap_or(0);
                ex.warnings.push(format!(
                    "{node_label}: chunk 0x{id:08X} could not be parsed ({e})"
                ));
            }
            None => return Err(e),
        },
    }
    Ok(())
}

fn mark_unhandled(ex: &mut Extraction) {
    if let Some(last) = ex.chunks.last_mut() {
        last.disposition = ChunkDisposition::Unknown;
    }
}

/// Reads chunk `0x0309201D` and decodes the bit-packed TM2020 input stream.
/// Returns warnings rather than pushing them so the caller can apply its
/// skip-on-failure fallback.
fn parse_inputs_chunk(
    r: &mut Reader<'_>,
    info: &mut GhostInfo,
    node_label: &str,
) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    let chunk_version = r.u32()?;
    let list = inputs::parse_player_input_data(r, chunk_version)?;
    if list.is_empty() {
        warnings.push(format!(
            "{node_label}: chunk 0x0309201D contains no input data"
        ));
        return Ok(warnings);
    }
    for p in list {
        let (events, truncated) = inputs::decode_trackmania_inputs(&p, false);
        if truncated {
            warnings.push(format!(
                "{node_label}: input stream ended early (ticks declared {}, archive version {})",
                p.ticks, p.version
            ));
        }
        if info.inputs.is_none() {
            info.inputs = Some(GhostInputs::Trackmania2020 {
                version: p.version,
                start_offset_ms: p.start_offset_ms,
                ticks: p.ticks,
                truncated,
                events,
            });
        }
    }
    Ok(warnings)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn parse_chunk_2d(r: &mut Reader<'_>, info: &mut GhostInfo) -> Result<()> {
    let u01 = r.i32()?;
    if u01 >= 1 {
        // Inputs stored separately.
        return Ok(());
    }
    info.validation.exe_version = Some(r.string()?);
    info.validation.exe_checksum = Some(r.u32()?);
    info.validation.os_kind = Some(r.i32()?);
    info.validation.cpu_kind = Some(r.i32()?);
    info.walltime_start = r.nullable_unix_time32()?;
    info.walltime_end = r.nullable_unix_time32()?;
    info.validation.title_id = Some(r.string()?);
    info.validation.title_checksum = Some(hex(r.take(32, "title checksum")?));
    let _u02 = r.i32()?;
    let _u03 = r.i32()?;
    info.validation.validation_seed = Some(r.i32()?);
    let _u04 = r.i32()?;
    info.validation.race_settings_xml = Some(r.string()?);
    Ok(())
}

fn parse_validation(r: &mut Reader<'_>, chunk_id: u32, info: &mut GhostInfo) -> Result<()> {
    if chunk_id == 0x0309_2025 {
        let _version = r.u32()?;
    }
    let events_duration = r.i32()?;
    info.validation.events_duration_ms = Some(events_duration);
    if events_duration != 0 {
        let _u01 = r.i32()?;
        let (entries, names) = inputs::parse_legacy_inputs(r)?;
        if !entries.is_empty() {
            info.inputs = Some(GhostInputs::Legacy {
                control_names: names,
                entries,
            });
        }
        info.validation.exe_version = Some(r.string()?);
        info.validation.exe_checksum = Some(r.u32()?);
        info.validation.os_kind = Some(r.i32()?);
        info.validation.cpu_kind = Some(r.i32()?);
        info.validation.race_settings_xml = Some(r.string()?);
        if chunk_id == 0x0309_2019 || chunk_id == 0x0309_2025 {
            info.validation.validation_seed = Some(r.i32()?);
        }
        if chunk_id == 0x0309_2025 {
            info.validation.steering_wheel_sensitivity = Some(r.bool()?);
        }
    }
    Ok(())
}

/// Ghost chunk `0x03092000`: identity plus, for TM2/TM2020, the car record data.
fn parse_identity(r: &mut Reader<'_>, info: &mut GhostInfo) -> Result<()> {
    let version = r.u32()?;
    info.chunk_version = Some(version);

    let mut appearance_version: u32 = 0;
    if version >= 9 {
        appearance_version = r.u32()?;
        info.appearance_version = Some(appearance_version);
    }

    let ident = r.ident()?;
    info.player_model = Some(ident);
    info.light_trail_color = Some(r.vec3()?);

    let pack_count = r.u32()?;
    for _ in 0..pack_count.min(4096) {
        let f: Fileref = r.fileref()?;
        info.skin_pack_descs.push(f);
    }

    let has_badges = r.bool()?;
    info.has_badges = Some(has_badges);
    if has_badges {
        let badge_version = r.u32()?;
        let _color = r.vec3()?;
        if badge_version == 0 {
            let _ = r.u32()?;
            let _ = r.string()?;
        }
        let sticker_count = r.u32()?;
        for _ in 0..sticker_count.min(4096) {
            let _ = r.string()?;
            let _ = r.string()?;
        }
        let layer_count = r.u32()?;
        for _ in 0..layer_count.min(4096) {
            let _ = r.string()?;
        }
    }

    if appearance_version >= 1 {
        let _ = r.string()?;
    }

    info.nickname = Some(r.string()?);
    info.avatar_name = Some(r.string()?);

    if version >= 2 {
        info.recording_context = Some(r.string()?);
    }
    if version >= 4 {
        let _ = r.bool()?;
    }

    if version >= 5 {
        // The record-data node reference is preceded by one unknown field.
        let first = r.u32()?;
        let next = r.u32()?;
        let index: i32 = if next == record::CLASS_CPLUG_ENT_RECORD_DATA {
            first as i32
        } else if first == u32::MAX {
            return Ok(());
        } else {
            let _class_id = r.u32()?;
            next as i32
        };
        if index >= 0 {
            info.record_data = Some(record::parse_node(r)?);
        }
        let count = r.u32()?;
        for _ in 0..count.min(65536) {
            let _ = r.i32()?;
        }
    }

    if version >= 6 {
        info.trigram = Some(r.string()?);
    }
    if version >= 7 {
        info.zone = Some(r.string()?);
    }
    if version >= 8 {
        info.club_tag = Some(r.string()?);
    }

    Ok(())
}
