//! Ghost record data (`CPlugEntRecordData`, class 0x0911F000) and the vehicle
//! sample stream it carries.

use crate::error::{Error, Result};
use crate::reader::Reader;

pub const CLASS_CPLUG_ENT_RECORD_DATA: u32 = 0x0911_F000;
pub const CLASS_CSCENE_VEHICLE_VIS: u32 = 0x0A01_8000;

#[derive(Debug, Clone)]
pub struct EntRecordDesc {
    pub class_id: u32,
    pub u01: i32,
    pub u02: i32,
    pub u03: i32,
    pub data_len: usize,
    pub data_head: Vec<u8>,
    pub u_last: i32,
}

#[derive(Debug, Clone)]
pub struct Sample {
    /// Milliseconds from the start of the run.
    pub time_ms: i32,
    pub raw: Vec<u8>,
    pub vehicle: Option<VehicleSample>,
}

#[derive(Debug, Clone)]
pub struct EntListElem {
    pub type_index: i32,
    pub u01: i32,
    pub u02: i32,
    pub u03: i32,
    pub u04: i32,
    pub class_id: Option<u32>,
    pub samples: Vec<Sample>,
    pub extra_samples: usize,
}

#[derive(Debug, Clone, Default)]
pub struct RecordData {
    pub version: u32,
    pub start_ms: Option<i32>,
    pub end_ms: Option<i32>,
    pub descriptors: Vec<EntRecordDesc>,
    pub lists: Vec<EntListElem>,
    pub notice_records: Vec<NoticeRecord>,
    pub bulk_notices: Vec<Vec<u8>>,
    pub custom_module_lists: usize,
}

#[derive(Debug, Clone)]
pub struct NoticeRecord {
    pub u01: i32,
    pub u02: i32,
    pub class_id: Option<u32>,
}

/// Reads a `CPlugEntRecordData` node (chunks until the end-of-node marker).
pub fn parse_node(r: &mut Reader<'_>) -> Result<RecordData> {
    let mut record = RecordData::default();
    while r.remaining() >= 4 {
        let chunk_id = r.u32()?;
        if chunk_id == crate::gbx::FACADE {
            break;
        }
        match chunk_id {
            CLASS_CPLUG_ENT_RECORD_DATA => {
                let version = r.u32()?;
                record.version = version;
                if version < 5 {
                    parse_archive(r, version, &mut record)?;
                } else {
                    let uncompressed = r.u32()? as usize;
                    let compressed = r.u32()? as usize;
                    let data = r.take(compressed, "record data blob")?;
                    let inflated = inflate(data, uncompressed)?;
                    let mut sub = Reader::new(&inflated);
                    parse_archive(&mut sub, version, &mut record)?;
                }
            }
            _ => {
                return Err(Error::invalid(
                    "CPlugEntRecordData chunk",
                    r.pos() - 4,
                    format!("unexpected chunk 0x{chunk_id:08X}"),
                ));
            }
        }
    }
    Ok(record)
}

/// Inflates a zlib stream. The GameBox declares the uncompressed size, which is
/// used as an upper bound so a corrupt stream cannot blow up memory.
fn inflate(data: &[u8], expected: usize) -> Result<Vec<u8>> {
    let limit = if expected > 0 {
        expected + 4096
    } else {
        64 * 1024 * 1024
    };
    match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, limit) {
        Ok(out) => Ok(out),
        Err(zlib_err) => miniz_oxide::inflate::decompress_to_vec_with_limit(data, limit)
            .map_err(|raw_err| Error::Zlib(format!("{zlib_err:?} (raw deflate: {raw_err:?})"))),
    }
}

fn parse_archive(r: &mut Reader<'_>, version: u32, record: &mut RecordData) -> Result<()> {
    if version >= 1 {
        record.start_ms = Some(r.i32()?);
        record.end_ms = Some(r.i32()?);
    }

    let desc_count = r.u32()?;
    for _ in 0..desc_count.min(4096) {
        let class_id = r.u32()?;
        let u01 = r.i32()?;
        let u02 = r.i32()?;
        let u03 = r.i32()?;
        let data = r.data()?;
        let head: Vec<u8> = data.iter().copied().take(32).collect();
        let u_last = r.i32()?;
        record.descriptors.push(EntRecordDesc {
            class_id,
            u01,
            u02,
            u03,
            data_len: data.len(),
            data_head: head,
            u_last,
        });
    }

    if version >= 2 {
        let notice_count = r.u32()?;
        for _ in 0..notice_count.min(65536) {
            let u01 = r.i32()?;
            let u02 = r.i32()?;
            let class_id = if version >= 4 {
                let v = r.u32()?;
                if v == u32::MAX { None } else { Some(v) }
            } else {
                None
            };
            record
                .notice_records
                .push(NoticeRecord { u01, u02, class_id });
        }
    }

    // entList. The "has element" flag is read once up front and once at the
    // end of every element (before its secondary sample list), not twice.
    let mut has_elem = r.u8()?;
    while has_elem == 1 {
        let type_index = r.i32()?;
        let u01 = r.i32()?;
        let u02 = r.i32()?;
        let u03 = r.i32()?;
        let u04 = if version >= 6 { r.i32()? } else { u01 };

        let class_id = record
            .descriptors
            .get(type_index.max(0) as usize)
            .map(|d| d.class_id);

        let mut samples = Vec::new();
        if version < 11 {
            loop {
                let has = r.u8()?;
                if has == 0 || has > 1 {
                    break;
                }
                let time = r.i32()?;
                let data = r.data()?;
                let vehicle = class_id.and_then(|_| decode_vehicle_sample(&data));
                samples.push(Sample {
                    time_ms: time,
                    raw: data,
                    vehicle,
                });
            }
        } else {
            let num_samples = r.i32()?;
            let sample_size = r.i32()?;
            if num_samples > 0 && sample_size >= 0 && num_samples < 10_000_000 && sample_size < 4096
            {
                let mut times = Vec::with_capacity(num_samples as usize);
                for _ in 0..num_samples {
                    times.push(r.i32()?);
                }
                // Column-major, delta-encoded per column.
                let mut buffers = vec![vec![0u8; sample_size as usize]; num_samples as usize];
                for slot in 0..sample_size as usize {
                    let mut accumulator: u8 = 0;
                    for b in 0..num_samples as usize {
                        let delta = r.u8()?;
                        accumulator = accumulator.wrapping_add(delta);
                        buffers[b][slot] = accumulator;
                    }
                }
                let mut time = 0i32;
                for b in 0..num_samples as usize {
                    time = time.wrapping_add(times[b]);
                    let data = buffers[b].clone();
                    let vehicle = class_id.and_then(|_| decode_vehicle_sample(&data));
                    samples.push(Sample {
                        time_ms: time,
                        raw: data,
                        vehicle,
                    });
                }
            }
        }

        // Flag for the next element, then this element's secondary samples.
        has_elem = r.u8()?;
        let mut extra_samples = 0usize;
        if version >= 2 {
            loop {
                let has = r.u8()?;
                if has == 0 || has > 1 {
                    break;
                }
                let _type = r.i32()?;
                let _time = r.i32()?;
                let _data = r.data()?;
                extra_samples += 1;
            }
        }

        record.lists.push(EntListElem {
            type_index,
            u01,
            u02,
            u03,
            u04,
            class_id,
            samples,
            extra_samples,
        });
    }

    if version >= 3 {
        // Auxiliary sections (bulk notices, custom modules). These carry no
        // vehicle data, so a parse failure here only means the rest of the
        // archive is left undecoded.
        let _ = parse_auxiliary(r, version, record);
    }

    Ok(())
}

/// Best-effort parse of the trailing sections of a record archive.
fn parse_auxiliary(r: &mut Reader<'_>, version: u32, record: &mut RecordData) -> Result<()> {
    // Bulk notice records
    loop {
        let has = r.u8()?;
        if has == 0 || has > 1 {
            break;
        }
        let _u01 = r.i32()?;
        let _u02 = r.i32()?;
        record.bulk_notices.push(r.data()?);
    }

    // Custom module delta lists
    let list_count = if version >= 8 { r.i32()? } else { 1 };
    for _ in 0..list_count.clamp(0, 4096) {
        loop {
            let has = r.u8()?;
            if has == 0 || has > 1 {
                break;
            }
            let _u01 = r.i32()?;
            let _data = r.data()?;
            if version >= 9 {
                let _u02 = r.data()?;
            }
        }
        if version >= 10 {
            let _period = r.i32()?;
        }
        record.custom_module_lists += 1;
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct VehicleSample {
    pub position: [f32; 3],
    pub rotation_angle: f32,
    pub rotation_axis_heading: f32,
    pub rotation_axis_pitch: f32,
    /// Metres per second along the forward axis.
    pub speed: f32,
    pub velocity: [f32; 3],
    pub side_speed: f32,
    pub rpm: u8,
    pub wheel_rotation: [u8; 8],
    /// -1 .. 1
    pub steer: f32,
    /// 0 .. 1
    pub gas: f32,
    /// 0 .. 1
    pub brake: f32,
    pub turbo_time: u8,
    pub suspension: [u8; 4],
    pub ground_material: [u8; 4],
    pub is_turbo: u8,
    pub vehicle_state: u8,
    pub ground_mode: u8,
    pub booster_air_control: u8,
    /// -0.25 (reverse) .. 1 (last gear)
    pub gear: f32,
    pub wheel_dirt: [u8; 4],
    pub wetness: u8,
}

/// Decodes a `CSceneVehicleVis` record delta.
pub fn decode_vehicle_sample(data: &[u8]) -> Option<VehicleSample> {
    if data.len() < 103 {
        return None;
    }
    let u8_at = |i: usize| data[i];
    let u16_at = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]);
    let i16_at = |i: usize| i16::from_le_bytes([data[i], data[i + 1]]);
    let f32_at = |i: usize| {
        f32::from_bits(u32::from_le_bytes([
            data[i],
            data[i + 1],
            data[i + 2],
            data[i + 3],
        ]))
    };

    let position = [f32_at(47), f32_at(51), f32_at(55)];
    let rotation_angle = u16_at(59) as f32 / u16::MAX as f32 * std::f32::consts::PI;
    let rotation_axis_heading = i16_at(61) as f32 / i16::MAX as f32 * std::f32::consts::PI;
    let rotation_axis_pitch = i16_at(63) as f32 / i16::MAX as f32 * (std::f32::consts::PI / 2.0);
    let speed = (i16_at(65) as f32 / 1000.0).exp();
    let velocity_heading = data[67] as i8 as f32 / i8::MAX as f32 * std::f32::consts::PI;
    let velocity_pitch = data[68] as i8 as f32 / i8::MAX as f32 * (std::f32::consts::PI / 2.0);
    let velocity = [
        speed * velocity_pitch.cos() * velocity_heading.cos(),
        speed * velocity_pitch.cos() * velocity_heading.sin(),
        speed * velocity_pitch.sin(),
    ];

    let steer_raw = u8_at(14);
    let gas_raw = u8_at(15);
    let brake_raw = u8_at(18);

    Some(VehicleSample {
        position,
        rotation_angle,
        rotation_axis_heading,
        rotation_axis_pitch,
        speed,
        velocity,
        side_speed: u16_at(2) as f32,
        rpm: u8_at(5),
        wheel_rotation: [
            u8_at(6),
            u8_at(7),
            u8_at(8),
            u8_at(9),
            u8_at(10),
            u8_at(11),
            u8_at(12),
            u8_at(13),
        ],
        steer: (steer_raw as f32 / 255.0 - 0.5) * 2.0,
        gas: gas_raw as f32 / 255.0,
        brake: brake_raw as f32 / 255.0,
        turbo_time: u8_at(21),
        suspension: [u8_at(23), u8_at(25), u8_at(27), u8_at(29)],
        ground_material: [u8_at(24), u8_at(26), u8_at(28), u8_at(30)],
        is_turbo: u8_at(31),
        vehicle_state: u8_at(76),
        ground_mode: u8_at(89),
        booster_air_control: u8_at(90),
        gear: (u8_at(91) as f32 - 1.0) / 4.0,
        wheel_dirt: [u8_at(93), u8_at(95), u8_at(97), u8_at(99)],
        wetness: u8_at(101),
    })
}

/// Legacy `CGameGhost` record data (chunks `0x0303F005` / `0x0303F006`), used
/// by TM2 and older: a zlib blob of `compressed` bytes expanding to
/// `uncompressed` bytes.
pub fn parse_ghost_record_blob(compressed: &[u8], uncompressed: usize) -> Result<Vec<u8>> {
    inflate(compressed, uncompressed)
}

/// Splits a decompressed legacy ghost blob into per-sample blobs.
pub fn split_legacy_samples(data: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut r = Reader::new(data);
    let _class_id = r.u32()?;
    let _skip_list2 = r.u32()?;
    let _sample_period = r.i32()?;
    let _u01 = r.u32()?;
    let sample_data_len = r.u32()? as usize;
    let sample_data_start = r.pos();
    r.skip(sample_data_len)?;
    let num_samples = r.i32()?;
    let mut first_offset = 0usize;
    let size_per_sample: i32;
    let mut sample_sizes: Vec<usize> = Vec::new();
    if num_samples > 0 {
        first_offset = r.u32()? as usize;
        if num_samples > 1 {
            size_per_sample = r.i32()?;
            if size_per_sample == -1 {
                for _ in 0..(num_samples - 1).min(1_000_000) {
                    sample_sizes.push(r.u32()? as usize);
                }
            } else {
                sample_sizes.push(size_per_sample as usize);
            }
        }
    }

    // Offsets describe every sample but the last, which takes the remainder.
    let mut out = Vec::new();
    let mut offset = sample_data_start + first_offset;
    for i in 0..num_samples.max(0) as usize {
        let len = if i < sample_sizes.len() {
            sample_sizes[i]
        } else {
            sample_sizes.last().copied().unwrap_or(0)
        };
        if offset + len > data.len() {
            break;
        }
        out.push(data[offset..offset + len].to_vec());
        offset += len;
    }
    Ok(out)
}
