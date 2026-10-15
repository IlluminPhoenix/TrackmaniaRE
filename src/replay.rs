//! Replay record node (`CGameCtnReplayRecord`, class 0x03093000).

use crate::error::Result;
use crate::gbx;
use crate::ghost;
use crate::model::{ChunkDisposition, ChunkRecord, Extraction, ReplayInfo};
use crate::reader::Reader;

pub const CLASS_CGAME_CTN_REPLAY_RECORD: u32 = 0x0309_3000;

/// Chunks of the replay record that the game writes with a SKIP marker and that
/// we deliberately step over (interfaces scripts, anchored objects, item skins,
/// bonuses, ...).
const REPLAY_SKIPPABLE: [u32; 19] = [
    0x0309_3007,
    0x0309_3008,
    0x0309_300F,
    0x0309_3013,
    0x0309_301A,
    0x0309_301B,
    0x0309_301C,
    0x0309_301D,
    0x0309_301E,
    0x0309_301F,
    0x0309_3020,
    0x0309_3021,
    0x0309_3022,
    0x0309_3023,
    0x0309_3025,
    0x0309_3026,
    0x0309_3027,
    0x0309_3028,
    0x0309_3029,
];

/// Parses the replay record node.
pub fn parse_replay_record(
    r: &mut Reader<'_>,
    ex: &mut Extraction,
    keep_embedded_map: bool,
) -> Result<ReplayInfo> {
    let mut info = ReplayInfo::default();
    let mut read_nodes: Vec<usize> = Vec::new();

    while r.remaining() >= 4 {
        let chunk = match gbx::read_chunk(r) {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(e) => {
                ex.warnings.push(format!("replay: {e}"));
                break;
            }
        };
        let size = chunk.skippable_size;
        let data_start = chunk.data_start;
        ex.chunks.push(ChunkRecord {
            node: "replay".into(),
            id: chunk.id,
            offset: data_start,
            size,
            disposition: ChunkDisposition::Parsed,
        });

        match chunk.id {
            // Skippable chunks declare their payload size, so a parse failure
            // can always be recovered from by skipping the payload.
            0x0309_3000 => guarded(r, ex, size, data_start, &mut info, &|sub, info| {
                parse_version_chunk(sub, info)
            })?,
            0x0309_3001 => guarded(r, ex, size, data_start, &mut info, &|sub, info| {
                let xml = sub.string()?;
                info.xml_attributes = parse_xml_attributes(&xml);
                info.community_xml = Some(xml);
                Ok(())
            })?,
            0x0309_3002 => {
                let len = r.u32()? as usize;
                let data = r.take(len, "embedded track")?;
                info.embedded_map_size = Some(len);
                if keep_embedded_map {
                    ex.embedded_map = Some(data.to_vec());
                }
            }
            0x0309_3014 => {
                let _ignored = r.u32()?;
                let count = r.u32()?;
                info.ghost_count = Some(count);
                for _ in 0..count.min(4096) {
                    let index = r.i32()?;
                    if index >= 0 && !read_nodes.contains(&(index as usize)) {
                        read_nodes.push(index as usize);
                        let class_id = r.u32()?;
                        read_referenced_node(r, index as usize, class_id, &mut read_nodes, ex)?;
                    }
                }
                let _ignored2 = r.u32()?;
                let extras = r.u32()?;
                for _ in 0..extras.min(4096) {
                    r.skip(8)?;
                }
            }
            0x0309_3015 => {
                let index = r.i32()?;
                if index >= 0 && !read_nodes.contains(&(index as usize)) {
                    read_nodes.push(index as usize);
                    let class_id = r.u32()?;
                    read_referenced_node(r, index as usize, class_id, &mut read_nodes, ex)?;
                }
            }
            0x0309_3018 => guarded(r, ex, size, data_start, &mut info, &|sub, info| {
                parse_author_chunk(sub, info)
            })?,
            0x0309_3024 => guarded(r, ex, size, data_start, &mut info, &|sub, _| {
                // Record data descriptor: version followed by two ids.
                let _version = sub.i32()?;
                let _a = sub.i32()?;
                let _b = sub.i32()?;
                Ok(())
            })?,
            _ => match size {
                Some(n) => {
                    r.skip(n)?;
                    if let Some(last) = ex.chunks.last_mut() {
                        last.disposition = if REPLAY_SKIPPABLE.contains(&chunk.id) {
                            ChunkDisposition::Skipped
                        } else {
                            ChunkDisposition::Unknown
                        };
                    }
                }
                None => {
                    if let Some(last) = ex.chunks.last_mut() {
                        last.disposition = ChunkDisposition::Unknown;
                    }
                    match gbx::resync_to_next_chunk(r, 1024)? {
                        Some(skipped) => ex.warnings.push(format!(
                            "replay: unknown non-skippable chunk 0x{:08X}, skipped {skipped} bytes to resynchronise",
                            chunk.id
                        )),
                        None => {
                            ex.warnings.push(format!(
                                "replay: unknown non-skippable chunk 0x{:08X}, replay parsing stopped",
                                chunk.id
                            ));
                            break;
                        }
                    }
                }
            },
        }
    }

    Ok(info)
}

/// Reads a node referenced from the replay record. Ghost nodes are parsed in
/// full; anything else (media clips and their groups, which cinematic replays
/// reference) is stepped over.
fn read_referenced_node(
    r: &mut Reader<'_>,
    index: usize,
    class_id: u32,
    read_nodes: &mut Vec<usize>,
    ex: &mut Extraction,
) -> Result<()> {
    const GHOST_CLASSES: [u32; 3] = [0x0309_2000, 0x2401_B000, 0x0303_F005];
    if GHOST_CLASSES.contains(&class_id) {
        let g = ghost::parse_ghost(r, index, ex)?;
        ex.ghosts.push(g);
        return Ok(());
    }

    ex.warnings.push(format!(
        "note: skipped auxiliary node #{index} of class 0x{class_id:08X}"
    ));
    // The node's chunk list ends with the end-of-node marker; scan for it on a
    // 4-byte boundary instead of guessing the layout of media clip chunks.
    let start = r.pos();
    let limit = r.len().min(start + 4 * 1024 * 1024);
    let mut found = None;
    let mut at = start;
    while at + 4 <= limit {
        if r.data_at(at) == gbx::FACADE.to_le_bytes() {
            // Only accept the marker when the stream continues sensibly after
            // it, otherwise it was data that happened to look like one.
            let after = at + 4;
            let plausible = after + 4 > r.len() || {
                let next = u32::from_le_bytes(r.data_at(after));
                next == gbx::FACADE || next >> 24 <= 0x0A
            };
            if plausible {
                found = Some(after);
                break;
            }
        }
        at += 4;
    }
    let skipped = match found {
        Some(end) => {
            let n = end - start;
            r.set_pos(end);
            n
        }
        None => {
            r.set_pos(start);
            ex.warnings
                .push(format!("node #{index}: end-of-node marker not found"));
            return Ok(());
        }
    };
    ex.chunks.push(ChunkRecord {
        node: format!("node#{index}"),
        id: class_id,
        offset: start,
        size: Some(skipped),
        disposition: ChunkDisposition::Skipped,
    });
    let _ = read_nodes;
    Ok(())
}

/// Runs a chunk parser on a sub-reader when the chunk declared its size, so a
/// mis-parse can never desynchronise the surrounding stream.
fn guarded(
    r: &mut Reader<'_>,
    ex: &mut Extraction,
    size: Option<usize>,
    data_start: usize,
    info: &mut ReplayInfo,
    parse: &dyn Fn(&mut Reader<'_>, &mut ReplayInfo) -> Result<()>,
) -> Result<()> {
    let outcome = match size {
        Some(n) => {
            let mut sub = r.sub(n, "chunk payload")?;
            parse(&mut sub, info)
        }
        None => parse(r, info),
    };
    if let Err(e) = outcome {
        match size {
            Some(n) => {
                r.set_pos(data_start);
                r.skip(n)?;
                if let Some(last) = ex.chunks.last_mut() {
                    last.disposition = ChunkDisposition::Unknown;
                }
                let id = ex.chunks.last().map(|c| c.id).unwrap_or(0);
                ex.warnings.push(format!(
                    "replay: chunk 0x{id:08X} could not be parsed ({e})"
                ));
            }
            None => return Err(e),
        }
    }
    Ok(())
}

/// Ghost/replay chunk `0x03093018`: who saved the replay.
///
/// Two layouts exist in the wild: one starting with a title uid followed by
/// the title string, and one with two plain integers up front.
fn parse_author_chunk(r: &mut Reader<'_>, info: &mut ReplayInfo) -> Result<()> {
    let start = r.pos();
    let attempt = (|| -> Result<()> {
        let _title_uid = r.u32()?;
        info.author_title = Some(r.string()?);
        let _version = r.u32()?;
        info.author_login = Some(r.string()?);
        info.author_nickname = Some(r.string()?);
        info.author_zone = Some(r.string()?);
        info.author_extra_info = Some(r.string()?);
        Ok(())
    })();
    if attempt.is_ok() {
        return attempt;
    }
    r.set_pos(start);
    let _version = r.u32()?;
    let _author_version = r.u32()?;
    info.author_login = Some(r.string()?);
    info.author_nickname = Some(r.string()?);
    info.author_zone = Some(r.string()?);
    info.author_extra_info = Some(r.string()?);
    Ok(())
}

/// Header chunk `0x03093000` / body version chunk.
pub fn parse_version_chunk(r: &mut Reader<'_>, info: &mut ReplayInfo) -> Result<()> {
    let version = r.u32()?;
    info.version = Some(version);
    if version >= 2 && r.remaining() > 8 {
        let ident = r.ident()?;
        info.map_uid = Some(ident.id.as_text());
        info.map_environment = Some(ident.collection.as_text());
        info.map_author = Some(ident.author.clone());
        info.time_ms = Some(r.u32()?);
        info.nickname = Some(r.string()?);
        if version >= 6 {
            info.driver_login = Some(r.string()?);
            if r.remaining() >= 5 {
                let _ = r.u8()?;
                info.title_uid = Some(r.id_string()?);
            }
        }
    }
    Ok(())
}

/// Extracts `name="value"` pairs from the community XML block.
pub fn parse_xml_attributes(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = xml.chars().collect();
    let mut i = 0;
    while i + 3 < chars.len() {
        if chars[i] == ' ' && chars[i + 1].is_ascii_alphabetic() {
            let mut j = i + 1;
            while j < chars.len()
                && (chars[j].is_ascii_alphanumeric() || chars[j] == '_' || chars[j] == '-')
            {
                j += 1;
            }
            if j < chars.len() && chars[j] == '=' && j + 1 < chars.len() && chars[j + 1] == '"' {
                let name: String = chars[i + 1..j].iter().collect();
                let mut k = j + 2;
                let mut value = String::new();
                while k < chars.len() && chars[k] != '"' {
                    if chars[k] == '&' {
                        // Decode the handful of entities the engine emits.
                        let rest: String = chars[k..].iter().collect();
                        if let Some(stop) = rest.find(';') {
                            let entity = &rest[1..stop];
                            let decoded = match entity {
                                "quot" => Some('"'),
                                "apos" => Some('\''),
                                "lt" => Some('<'),
                                "gt" => Some('>'),
                                "amp" => Some('&'),
                                other => other
                                    .strip_prefix('#')
                                    .and_then(|d| d.parse::<u32>().ok())
                                    .and_then(char::from_u32),
                            };
                            if let Some(c) = decoded {
                                value.push(c);
                                k += stop + 1;
                                continue;
                            }
                        }
                    }
                    value.push(chars[k]);
                    k += 1;
                }
                if !name.is_empty() {
                    out.push((name, value));
                }
                i = k + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}
