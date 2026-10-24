//! Trackmania replay extraction library.

pub mod error;
pub mod gbx;
pub mod ghost;
pub mod inputs;
pub mod json;
pub mod lzo;
pub mod model;
pub mod reader;
pub mod record;
pub mod replay;
pub mod tabular;
pub mod tas;

use crate::error::Result;
use crate::gbx::Gbx;
use crate::model::{ContainerInfo, Extraction};
use crate::reader::Reader;

/// Controls how much of the file is kept in the result.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExtractOptions {
    /// Keep the map embedded in the replay (it can be a couple of megabytes).
    pub keep_embedded_map: bool,
    /// Keep the decompressed body for raw dumping.
    pub keep_body: bool,
}

/// Parses a `.Replay.Gbx` file.
pub fn parse_replay(data: &[u8], file_name: &str, keep_embedded_map: bool) -> Result<Extraction> {
    parse_replay_with(
        data,
        file_name,
        ExtractOptions {
            keep_embedded_map,
            keep_body: false,
        },
    )
}

/// Parses a `.Replay.Gbx` file with explicit options.
pub fn parse_replay_with(data: &[u8], file_name: &str, opts: ExtractOptions) -> Result<Extraction> {
    let keep_embedded_map = opts.keep_embedded_map;
    let gbx = Gbx::parse(data)?;

    let container = ContainerInfo {
        gbx_version: gbx.version,
        class_id: gbx.class_id,
        format: gbx.format,
        reference_table_compression: gbx.reference_table_compression,
        body_compression: gbx.body_compression,
        edit_state: gbx.edit_state,
        header_chunks: gbx
            .header_chunks
            .iter()
            .map(|c| (c.id, c.data.len()))
            .collect(),
        num_nodes: gbx.num_nodes,
        external_nodes: gbx.external_nodes.len(),
        body_size: gbx.body.len(),
        body_uncompressed_size: gbx.body_uncompressed_size,
        body_complete: gbx.body_complete,
    };

    let mut ex = Extraction {
        file: file_name.to_string(),
        file_size: data.len() as u64,
        container,
        replay: Default::default(),
        ghosts: Vec::new(),
        chunks: Vec::new(),
        warnings: Vec::new(),
        embedded_map: None,
        body: Vec::new(),
        // Header chunk payloads live in the user-data block, not in the body;
        // keep them for the raw dump only.
        header_chunk_data: if opts.keep_body {
            gbx.header_chunks
                .iter()
                .map(|c| (c.id, c.data.clone()))
                .collect()
        } else {
            Vec::new()
        },
    };

    if !gbx.body_complete {
        ex.warnings.push(format!(
            "body is incomplete: header expects {} bytes, file provided less",
            gbx.body_uncompressed_size
        ));
    }

    // Header chunks first (version, community XML, author).
    for hc in &gbx.header_chunks {
        let mut sub = Reader::new(&hc.data);
        match hc.id {
            0x0309_3000 => {
                if replay::parse_version_chunk(&mut sub, &mut ex.replay).is_ok() {
                    ex.chunks.push(model::ChunkRecord {
                        node: "header".into(),
                        id: hc.id,
                        offset: 0,
                        size: Some(hc.data.len()),
                        disposition: model::ChunkDisposition::Parsed,
                    });
                }
            }
            0x0309_3001 => {
                if let Ok(xml) = sub.string() {
                    ex.replay.xml_attributes = replay::parse_xml_attributes(&xml);
                    ex.replay.community_xml = Some(xml);
                    ex.chunks.push(model::ChunkRecord {
                        node: "header".into(),
                        id: hc.id,
                        offset: 0,
                        size: Some(hc.data.len()),
                        disposition: model::ChunkDisposition::Parsed,
                    });
                }
            }
            0x0309_3002 => {
                // Author info in the header.
                let _ = sub.u32();
                let _ = sub.u32();
                ex.replay.author_login = sub.string().ok();
                ex.replay.author_nickname = sub.string().ok();
                ex.replay.author_zone = sub.string().ok();
                ex.replay.author_extra_info = sub.string().ok();
            }
            _ => {
                ex.chunks.push(model::ChunkRecord {
                    node: "header".into(),
                    id: hc.id,
                    offset: 0,
                    size: Some(hc.data.len()),
                    disposition: model::ChunkDisposition::Skipped,
                });
            }
        }
    }

    // Body.
    let mut body = Reader::new(&gbx.body);
    if gbx.class_id == replay::CLASS_CGAME_CTN_REPLAY_RECORD {
        // A failure deep inside the stream must not throw away everything that
        // was already extracted, so the error becomes a warning.
        match replay::parse_replay_record(&mut body, &mut ex, keep_embedded_map) {
            Ok(info) => merge_replay(&mut ex.replay, info),
            Err(e) => ex
                .warnings
                .push(format!("replay parsing stopped early: {e}")),
        }
    } else if gbx.class_id == ghost::CLASS_CGAME_CTN_GHOST {
        // A standalone `.Ghost.Gbx` file wraps one ghost node in the body.
        match ghost::parse_ghost(&mut body, 0, &mut ex) {
            Ok(g) => ex.ghosts.push(g),
            Err(e) => ex
                .warnings
                .push(format!("ghost parsing stopped early: {e}")),
        }
    } else {
        ex.warnings.push(format!(
            "unexpected root class 0x{:08X}; only replay records are supported",
            gbx.class_id
        ));
    }

    if opts.keep_body {
        ex.body = gbx.body.clone();
    }

    Ok(ex)
}

/// Merges the values found in the body over the ones from the header chunks.
fn merge_replay(dst: &mut model::ReplayInfo, src: model::ReplayInfo) {
    dst.version = dst.version.or(src.version);
    dst.map_uid = dst.map_uid.clone().or(src.map_uid);
    dst.map_environment = dst.map_environment.clone().or(src.map_environment);
    dst.map_author = dst.map_author.clone().or(src.map_author);
    dst.time_ms = dst.time_ms.or(src.time_ms);
    dst.nickname = dst.nickname.clone().or(src.nickname);
    dst.driver_login = dst.driver_login.clone().or(src.driver_login);
    dst.title_uid = dst.title_uid.clone().or(src.title_uid);
    dst.ghost_count = dst.ghost_count.or(src.ghost_count);
    dst.embedded_map_size = dst.embedded_map_size.or(src.embedded_map_size);
    dst.author_title = dst.author_title.clone().or(src.author_title);
    dst.author_login = dst.author_login.clone().or(src.author_login);
    dst.author_nickname = dst.author_nickname.clone().or(src.author_nickname);
    dst.author_zone = dst.author_zone.clone().or(src.author_zone);
    dst.author_extra_info = dst.author_extra_info.clone().or(src.author_extra_info);
    dst.community_xml = dst.community_xml.clone().or(src.community_xml);
    dst.record_data = dst.record_data.take().or(src.record_data);
    if dst.xml_attributes.is_empty() {
        dst.xml_attributes = src.xml_attributes;
    }
}
