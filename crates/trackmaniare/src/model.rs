//! Output model: everything this tool can extract from a replay.

use crate::inputs::{ControlEntry, InputEvent};
use crate::json::Json;
use crate::reader::{Fileref, Ident};
use crate::record::{EntListElem, RecordData, VehicleSample};

/// What the extractor did with a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkDisposition {
    /// The chunk payload was interpreted.
    Parsed,
    /// The chunk declared its size and was stepped over on purpose.
    Skipped,
    /// The chunk was not understood.
    Unknown,
}

impl ChunkDisposition {
    pub fn as_str(self) -> &'static str {
        match self {
            ChunkDisposition::Parsed => "parsed",
            ChunkDisposition::Skipped => "skipped",
            ChunkDisposition::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkRecord {
    pub node: String,
    pub id: u32,
    pub offset: usize,
    pub size: Option<usize>,
    pub disposition: ChunkDisposition,
}

#[derive(Debug, Clone, Default)]
pub struct ContainerInfo {
    pub gbx_version: u16,
    pub class_id: u32,
    pub format: char,
    pub reference_table_compression: char,
    pub body_compression: char,
    pub edit_state: char,
    pub header_chunks: Vec<(u32, usize)>,
    pub num_nodes: u32,
    pub external_nodes: usize,
    pub body_size: usize,
    pub body_uncompressed_size: usize,
    pub body_complete: bool,
}

impl ContainerInfo {
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        o.set("gbx_version", Json::UInt(self.gbx_version as u64));
        o.set("class_id", Json::str(format!("0x{:08X}", self.class_id)));
        o.set("format", Json::str(self.format.to_string()));
        o.set(
            "reference_table_compression",
            Json::str(self.reference_table_compression.to_string()),
        );
        o.set(
            "body_compression",
            Json::str(self.body_compression.to_string()),
        );
        o.set("edit_state", Json::str(self.edit_state.to_string()));
        o.set(
            "header_chunks",
            Json::Arr(
                self.header_chunks
                    .iter()
                    .map(|(id, size)| {
                        let mut c = Json::obj();
                        c.set("id", Json::str(format!("0x{id:08X}")));
                        c.set("size", Json::UInt(*size as u64));
                        c
                    })
                    .collect(),
            ),
        );
        o.set("num_nodes", Json::UInt(self.num_nodes as u64));
        o.set("external_nodes", Json::UInt(self.external_nodes as u64));
        o.set("body_size", Json::UInt(self.body_size as u64));
        o.set(
            "body_uncompressed_size",
            Json::UInt(self.body_uncompressed_size as u64),
        );
        o.set("body_complete", Json::Bool(self.body_complete));
        o
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReplayInfo {
    pub version: Option<u32>,
    pub map_uid: Option<String>,
    pub map_environment: Option<String>,
    pub map_author: Option<String>,
    pub time_ms: Option<u32>,
    pub nickname: Option<String>,
    pub driver_login: Option<String>,
    pub title_uid: Option<String>,
    pub community_xml: Option<String>,
    pub xml_attributes: Vec<(String, String)>,
    pub embedded_map_size: Option<usize>,
    pub ghost_count: Option<u32>,
    pub author_title: Option<String>,
    pub author_login: Option<String>,
    pub author_nickname: Option<String>,
    pub author_zone: Option<String>,
    pub author_extra_info: Option<String>,
    /// Combined record data written at replay level by merge/tooling instead
    /// of into a ghost (`CPlugEntRecordData` chunk `0x0911F000`).
    pub record_data: Option<RecordData>,
}

impl ReplayInfo {
    pub fn to_json(&self, include_timelines: bool) -> Json {
        let mut o = Json::obj();
        o.set("version", Json::opt_int(self.version));
        o.set("map_uid", Json::opt_str(self.map_uid.clone()));
        o.set(
            "map_environment",
            Json::opt_str(self.map_environment.clone()),
        );
        o.set("map_author", Json::opt_str(self.map_author.clone()));
        o.set("time_ms", Json::opt_int(self.time_ms));
        o.set("nickname", Json::opt_str(self.nickname.clone()));
        o.set("driver_login", Json::opt_str(self.driver_login.clone()));
        o.set("title_uid", Json::opt_str(self.title_uid.clone()));
        o.set(
            "embedded_map_size",
            Json::opt_int(self.embedded_map_size.map(|v| v as i64)),
        );
        o.set("ghost_count", Json::opt_int(self.ghost_count));
        o.set(
            "record_data",
            match &self.record_data {
                Some(record) => record_data_to_json(record, include_timelines),
                None => Json::Null,
            },
        );
        o.set("author_title", Json::opt_str(self.author_title.clone()));
        o.set("author_login", Json::opt_str(self.author_login.clone()));
        o.set(
            "author_nickname",
            Json::opt_str(self.author_nickname.clone()),
        );
        o.set("author_zone", Json::opt_str(self.author_zone.clone()));
        o.set(
            "author_extra_info",
            Json::opt_str(self.author_extra_info.clone()),
        );
        if !self.xml_attributes.is_empty() {
            let mut x = Json::obj();
            for (k, v) in &self.xml_attributes {
                x.set(k, Json::str(v.clone()));
            }
            o.set("community_xml", x);
        }
        if let Some(xml) = &self.community_xml {
            o.set("community_xml_raw", Json::str(xml.clone()));
        }
        o
    }
}

#[derive(Debug, Clone, Default)]
pub struct ValidationInfo {
    pub events_duration_ms: Option<i32>,
    pub exe_version: Option<String>,
    pub exe_checksum: Option<u32>,
    pub os_kind: Option<i32>,
    pub cpu_kind: Option<i32>,
    pub race_settings_xml: Option<String>,
    pub validation_seed: Option<i32>,
    pub title_id: Option<String>,
    pub title_checksum: Option<String>,
    pub steering_wheel_sensitivity: Option<bool>,
}

impl ValidationInfo {
    pub fn is_empty(&self) -> bool {
        self.events_duration_ms.is_none()
            && self.exe_version.is_none()
            && self.exe_checksum.is_none()
            && self.race_settings_xml.is_none()
            && self.title_id.is_none()
            && self.validation_seed.is_none()
            && self.steering_wheel_sensitivity.is_none()
    }

    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        o.set("events_duration_ms", Json::opt_int(self.events_duration_ms));
        o.set("exe_version", Json::opt_str(self.exe_version.clone()));
        o.set("exe_checksum", Json::opt_int(self.exe_checksum));
        o.set("os_kind", Json::opt_int(self.os_kind));
        o.set("cpu_kind", Json::opt_int(self.cpu_kind));
        o.set(
            "race_settings_xml",
            Json::opt_str(self.race_settings_xml.clone()),
        );
        o.set("validation_seed", Json::opt_int(self.validation_seed));
        o.set("title_id", Json::opt_str(self.title_id.clone()));
        o.set("title_checksum", Json::opt_str(self.title_checksum.clone()));
        o.set(
            "steering_wheel_sensitivity",
            Json::opt_int(self.steering_wheel_sensitivity.map(|b| b as i64)),
        );
        o
    }
}

#[derive(Debug, Clone)]
pub enum GhostInputs {
    Trackmania2020 {
        version: u32,
        start_offset_ms: Option<i32>,
        ticks: u32,
        truncated: bool,
        events: Vec<InputEvent>,
    },
    Legacy {
        control_names: Vec<String>,
        entries: Vec<ControlEntry>,
    },
}

impl GhostInputs {
    pub fn event_count(&self) -> usize {
        match self {
            GhostInputs::Trackmania2020 { events, .. } => events.len(),
            GhostInputs::Legacy { entries, .. } => entries.len(),
        }
    }

    pub fn to_json(&self, include_events: bool) -> Json {
        let mut o = Json::obj();
        match self {
            GhostInputs::Trackmania2020 {
                version,
                start_offset_ms,
                ticks,
                truncated,
                events,
            } => {
                o.set("format", Json::str("tm2020-bitpacked"));
                o.set("version", Json::UInt(*version as u64));
                o.set("start_offset_ms", Json::opt_int(*start_offset_ms));
                o.set("ticks", Json::UInt(*ticks as u64));
                o.set("decoded_truncated", Json::Bool(*truncated));
                o.set("event_count", Json::UInt(events.len() as u64));
                if include_events {
                    o.set(
                        "events",
                        Json::Arr(events.iter().map(event_to_json).collect()),
                    );
                }
            }
            GhostInputs::Legacy {
                control_names,
                entries,
            } => {
                o.set("format", Json::str("legacy-control-entries"));
                o.set(
                    "control_names",
                    Json::Arr(control_names.iter().map(|s| Json::str(s.clone())).collect()),
                );
                o.set("event_count", Json::UInt(entries.len() as u64));
                if include_events {
                    o.set(
                        "events",
                        Json::Arr(
                            entries
                                .iter()
                                .map(|e| {
                                    let mut o = Json::obj();
                                    o.set("time_ms", Json::Int(e.time_ms as i64));
                                    o.set("control", Json::str(e.control.clone()));
                                    o.set("onoff", Json::UInt(e.onoff as u64));
                                    o.set("value", Json::UInt(e.value as u64));
                                    o
                                })
                                .collect(),
                        ),
                    );
                }
            }
        }
        o
    }
}

fn event_to_json(e: &InputEvent) -> Json {
    use crate::inputs::Event;
    let mut o = Json::obj();
    o.set("time_ms", Json::Int(e.time_ms as i64));
    o.set("tick", Json::UInt(e.tick as u64));
    match &e.event {
        Event::Steer(v) => {
            o.set("type", Json::str("steer"));
            o.set("value", Json::Int(*v as i64));
            o.set("value_normalized", Json::Float(*v as f32 as f64 / 127.0));
        }
        Event::Accelerate(v) => {
            o.set("type", Json::str("accelerate"));
            o.set("value", Json::Bool(*v));
        }
        Event::Brake(v) => {
            o.set("type", Json::str("brake"));
            o.set("value", Json::Bool(*v));
        }
        Event::Horn(v) => {
            o.set("type", Json::str("horn"));
            o.set("value", Json::Bool(*v));
        }
        Event::Respawn => {
            o.set("type", Json::str("respawn"));
        }
        Event::SecondaryRespawn => {
            o.set("type", Json::str("secondary_respawn"));
        }
        Event::ActionSlot(slot, v) => {
            o.set("type", Json::str("action_slot"));
            o.set("slot", Json::UInt(*slot as u64));
            o.set("value", Json::Bool(*v));
        }
        Event::Jump(v) => {
            o.set("type", Json::str("jump"));
            o.set("value", Json::Bool(*v));
        }
        Event::FreeLook(v) => {
            o.set("type", Json::str("free_look"));
            o.set("value", Json::Bool(*v));
        }
        Event::Camera2(v) => {
            o.set("type", Json::str("camera2"));
            o.set("value", Json::Bool(*v));
        }
        Event::GunTrigger(v) => {
            o.set("type", Json::str("gun_trigger"));
            o.set("value", Json::Bool(*v));
        }
        Event::Action(v) => {
            o.set("type", Json::str("action"));
            o.set("value", Json::Bool(*v));
        }
        Event::Strafe(v) => {
            o.set("type", Json::str("strafe"));
            o.set("value", Json::UInt(*v as u64));
        }
        Event::Walk(v) => {
            o.set("type", Json::str("walk"));
            o.set("value", Json::UInt(*v as u64));
        }
        Event::Vertical(v) => {
            o.set("type", Json::str("vertical"));
            o.set("value", Json::UInt(*v as u64));
        }
        Event::Horizontal(v) => {
            o.set("type", Json::str("horizontal"));
            o.set("value", Json::UInt(*v as u64));
        }
        Event::Mouse(x, y) => {
            o.set("type", Json::str("mouse"));
            o.set("x", Json::UInt(*x as u64));
            o.set("y", Json::UInt(*y as u64));
        }
    }
    o
}

#[derive(Debug, Clone, Default)]
pub struct GhostInfo {
    pub node_index: usize,
    pub chunk_version: Option<u32>,
    pub appearance_version: Option<u32>,
    pub player_model: Option<Ident>,
    pub light_trail_color: Option<[f32; 3]>,
    pub skin_pack_descs: Vec<Fileref>,
    pub has_badges: Option<bool>,
    pub nickname: Option<String>,
    pub avatar_name: Option<String>,
    pub recording_context: Option<String>,
    pub trigram: Option<String>,
    pub zone: Option<String>,
    pub club_tag: Option<String>,
    pub race_time_ms: Option<i32>,
    pub respawns: Option<i32>,
    pub stunts_score: Option<i32>,
    pub checkpoints: Vec<(i32, i32)>,
    pub ghost_uid: Option<String>,
    pub login: Option<String>,
    pub validate_challenge_uid: Option<String>,
    pub ghost_version: Option<i32>,
    pub walltime_start: Option<i64>,
    pub walltime_end: Option<i64>,
    pub validation: ValidationInfo,
    pub inputs: Option<GhostInputs>,
    pub record_data: Option<RecordData>,
    pub legacy_record_blob: Option<Vec<u8>>,
}

impl GhostInfo {
    /// Number of decoded car states in this ghost's record data.
    pub fn sample_count(&self) -> usize {
        self.record_data
            .as_ref()
            .map(|r| r.vehicle_sample_count())
            .unwrap_or(0)
    }

    /// Number of samples of every other entity type in the record data.
    pub fn other_sample_count(&self) -> usize {
        self.record_data
            .as_ref()
            .map(|r| r.other_sample_count())
            .unwrap_or(0)
    }

    pub fn player_model_text(&self) -> String {
        self.player_model
            .as_ref()
            .map(|i| i.id.as_text())
            .unwrap_or_default()
    }

    pub fn to_json(&self, include_timelines: bool) -> Json {
        let mut o = Json::obj();
        o.set("node_index", Json::UInt(self.node_index as u64));
        o.set("chunk_version", Json::opt_int(self.chunk_version));
        o.set("appearance_version", Json::opt_int(self.appearance_version));
        if let Some(pm) = &self.player_model {
            let mut p = Json::obj();
            p.set("id", Json::str(pm.id.as_text()));
            p.set("collection", Json::str(pm.collection.as_text()));
            p.set("author", Json::str(pm.author.clone()));
            o.set("player_model", p);
        } else {
            o.set("player_model", Json::Null);
        }
        o.set(
            "light_trail_color",
            match &self.light_trail_color {
                Some(c) => Json::floats(c),
                None => Json::Null,
            },
        );
        o.set(
            "skin_pack_descs",
            Json::Arr(
                self.skin_pack_descs
                    .iter()
                    .map(|f| {
                        let mut p = Json::obj();
                        p.set("path", Json::str(f.path.clone()));
                        p.set("locator_url", Json::str(f.locator.clone()));
                        p
                    })
                    .collect(),
            ),
        );
        o.set(
            "has_badges",
            Json::opt_int(self.has_badges.map(|b| b as i64)),
        );
        o.set("nickname", Json::opt_str(self.nickname.clone()));
        o.set("avatar_name", Json::opt_str(self.avatar_name.clone()));
        o.set(
            "recording_context",
            Json::opt_str(self.recording_context.clone()),
        );
        o.set("trigram", Json::opt_str(self.trigram.clone()));
        o.set("zone", Json::opt_str(self.zone.clone()));
        o.set("club_tag", Json::opt_str(self.club_tag.clone()));
        o.set("race_time_ms", Json::opt_int(self.race_time_ms));
        o.set("respawns", Json::opt_int(self.respawns));
        o.set("stunts_score", Json::opt_int(self.stunts_score));
        o.set(
            "checkpoints",
            Json::Arr(
                self.checkpoints
                    .iter()
                    .map(|(time, stunts)| {
                        let mut c = Json::obj();
                        c.set("time_ms", Json::Int(*time as i64));
                        c.set("stunts_score", Json::Int(*stunts as i64));
                        c
                    })
                    .collect(),
            ),
        );
        o.set("ghost_uid", Json::opt_str(self.ghost_uid.clone()));
        o.set("login", Json::opt_str(self.login.clone()));
        o.set(
            "validate_challenge_uid",
            Json::opt_str(self.validate_challenge_uid.clone()),
        );
        o.set("ghost_version", Json::opt_int(self.ghost_version));
        o.set("walltime_start", Json::opt_int(self.walltime_start));
        o.set("walltime_end", Json::opt_int(self.walltime_end));
        o.set("validation", self.validation.to_json());
        o.set(
            "inputs",
            match &self.inputs {
                Some(i) => i.to_json(include_timelines),
                None => Json::Null,
            },
        );

        if let Some(record) = &self.record_data {
            o.set(
                "record_data",
                record_data_to_json(record, include_timelines),
            );
        } else if let Some(blob) = &self.legacy_record_blob {
            let mut r = Json::obj();
            r.set("format", Json::str("legacy-cgameghost"));
            r.set("blob_size", Json::UInt(blob.len() as u64));
            o.set("record_data", r);
        } else {
            o.set("record_data", Json::Null);
        }

        o
    }
}

/// JSON for a decoded record data node; used for ghost records and for the
/// combined record that merge tooling writes at replay level.
fn record_data_to_json(record: &RecordData, include_timelines: bool) -> Json {
    let mut r = Json::obj();
    r.set(
        "vehicle_sample_count",
        Json::UInt(record.vehicle_sample_count() as u64),
    );
    r.set("version", Json::UInt(record.version as u64));
    r.set("start_ms", Json::opt_int(record.start_ms));
    r.set("end_ms", Json::opt_int(record.end_ms));
    r.set(
        "descriptors",
        Json::Arr(
            record
                .descriptors
                .iter()
                .map(|d| {
                    let mut j = Json::obj();
                    j.set("class_id", Json::str(format!("0x{:08X}", d.class_id)));
                    j.set("u01", Json::Int(d.u01 as i64));
                    j.set("u02", Json::Int(d.u02 as i64));
                    j.set("u03", Json::Int(d.u03 as i64));
                    j.set("data_len", Json::UInt(d.data_len as u64));
                    j
                })
                .collect(),
        ),
    );
    r.set(
        "lists",
        Json::Arr(
            record
                .lists
                .iter()
                .map(|l| list_to_json(l, include_timelines))
                .collect(),
        ),
    );
    r
}

fn list_to_json(l: &EntListElem, include_samples: bool) -> Json {
    let mut o = Json::obj();
    o.set("type", Json::Int(l.type_index as i64));
    o.set(
        "class_id",
        Json::opt_str(l.class_id.map(|c| format!("0x{c:08X}"))),
    );
    o.set("u01", Json::Int(l.u01 as i64));
    o.set("u02", Json::Int(l.u02 as i64));
    o.set("u03", Json::Int(l.u03 as i64));
    o.set("u04", Json::Int(l.u04 as i64));
    o.set("sample_count", Json::UInt(l.samples.len() as u64));
    if include_samples {
        o.set(
            "samples",
            Json::Arr(l.samples.iter().map(sample_to_json).collect()),
        );
    }
    o
}

fn sample_to_json(s: &crate::record::Sample) -> Json {
    let mut o = Json::obj();
    o.set("time_ms", Json::Int(s.time_ms as i64));
    o.set("raw_len", Json::UInt(s.raw.len() as u64));
    if let Some(v) = &s.vehicle {
        o.set("vehicle", vehicle_to_json(v));
    }
    o
}

pub fn vehicle_to_json(v: &VehicleSample) -> Json {
    let mut o = Json::obj();
    o.set("position", Json::floats(&v.position));
    o.set("speed_mps", Json::Float(v.speed as f64));
    o.set("speed_kmh", Json::Float(v.speed as f64 * 3.6));
    o.set("velocity", Json::floats(&v.velocity));
    o.set("steer", Json::Float(v.steer as f64));
    o.set("gas", Json::Float(v.gas as f64));
    o.set("brake", Json::Float(v.brake as f64));
    o.set("rpm", Json::UInt(v.rpm as u64));
    o.set("gear", Json::Float(v.gear as f64));
    o.set("turbo_time", Json::UInt(v.turbo_time as u64));
    o.set("is_turbo", Json::UInt(v.is_turbo as u64));
    o
}

#[derive(Debug, Clone, Default)]
pub struct Extraction {
    pub file: String,
    pub file_size: u64,
    pub container: ContainerInfo,
    pub replay: ReplayInfo,
    pub ghosts: Vec<GhostInfo>,
    pub chunks: Vec<ChunkRecord>,
    pub warnings: Vec<String>,
    pub embedded_map: Option<Vec<u8>>,
    /// The decompressed body, kept only when raw dumping was requested.
    pub body: Vec<u8>,
    /// Payloads of the header (user-data) chunks, kept only when raw dumping
    /// was requested. They do not live in the body, so the chunk dump needs
    /// them from here.
    pub header_chunk_data: Vec<(u32, Vec<u8>)>,
}

impl Extraction {
    pub fn to_json(&self, include_timelines: bool) -> Json {
        let mut o = Json::obj();
        o.set("file", Json::str(self.file.clone()));
        o.set("file_size", Json::UInt(self.file_size));
        o.set("container", self.container.to_json());
        o.set("replay", self.replay.to_json(include_timelines));
        o.set(
            "ghosts",
            Json::Arr(
                self.ghosts
                    .iter()
                    .map(|g| g.to_json(include_timelines))
                    .collect(),
            ),
        );
        if !self.warnings.is_empty() {
            o.set(
                "warnings",
                Json::Arr(self.warnings.iter().map(|w| Json::str(w.clone())).collect()),
            );
        }
        if !self.chunks.is_empty() {
            o.set(
                "chunks",
                Json::Arr(
                    self.chunks
                        .iter()
                        .map(|c| {
                            let mut j = Json::obj();
                            j.set("node", Json::str(c.node.clone()));
                            j.set("id", Json::str(format!("0x{:08X}", c.id)));
                            j.set("offset", Json::UInt(c.offset as u64));
                            j.set("size", Json::opt_int(c.size.map(|v| v as i64)));
                            j.set("disposition", Json::str(c.disposition.as_str()));
                            j
                        })
                        .collect(),
                ),
            );
        }
        o
    }
}
