//! GameBox container: header, reference table and (LZO compressed) body.

use crate::error::{Error, Result};
use crate::lzo;
use crate::reader::Reader;

pub const FACADE: u32 = 0xFACADE01;
pub const SKIP_MARKER: u32 = 0x534B_4950; // "PIKS" in the file

#[derive(Debug, Clone)]
pub struct HeaderChunk {
    pub id: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ExternalNode {
    pub flags: u32,
    pub file_name: Option<String>,
    pub resource_index: Option<u32>,
    pub node_index: u32,
}

#[derive(Debug)]
pub struct Gbx {
    pub version: u16,
    pub format: char,
    pub reference_table_compression: char,
    pub body_compression: char,
    pub edit_state: char,
    pub class_id: u32,
    pub user_data: Vec<u8>,
    pub header_chunks: Vec<HeaderChunk>,
    pub num_nodes: u32,
    pub external_nodes: Vec<ExternalNode>,
    pub body: Vec<u8>,
    /// Size the body decompresses to, according to the header.
    pub body_uncompressed_size: usize,
    /// False when the compressed body was cut short (truncated file).
    pub body_complete: bool,
}

impl Gbx {
    pub fn parse(data: &[u8]) -> Result<Gbx> {
        if data.len() < 17 || &data[0..3] != b"GBX" {
            return Err(Error::NotAGbx(format!(
                "magic is {:?}",
                String::from_utf8_lossy(&data[0..3.min(data.len())])
            )));
        }

        let mut r = Reader::new(data);
        r.set_pos(3);
        let version = r.u16()?;
        let mut format = ' ';
        let mut ref_comp = ' ';
        let mut body_comp = ' ';
        let mut edit_state = ' ';
        if version >= 3 {
            format = r.u8()? as char;
            ref_comp = r.u8()? as char;
            body_comp = r.u8()? as char;
        }
        if version >= 4 {
            edit_state = r.u8()? as char;
        }

        let mut class_id = 0;
        let mut num_nodes = 0;
        let mut header_chunks = Vec::new();
        let mut user_data = Vec::new();

        if version >= 3 {
            class_id = r.u32()?;
            if version >= 6 {
                // The size field does not include itself.
                let size = r.u32()? as usize;
                let block_start = r.pos();
                if block_start + size > data.len() {
                    return Err(Error::eof("user data block", block_start));
                }
                user_data = data[block_start..block_start + size].to_vec();
                header_chunks = parse_header_chunks(&user_data)?;
                r.set_pos(block_start + size);
            }
            num_nodes = r.u32()?;
        }

        let mut external_nodes = Vec::new();
        if version >= 3 {
            let num_external = r.u32()?;
            if num_external > 0 {
                // u32 ancestor level + folder tree
                let _ancestor_level = r.u32()?;
                read_folders(&mut r)?;
                for _ in 0..num_external {
                    let flags = r.u32()?;
                    let mut file_name = None;
                    let mut resource_index = None;
                    if flags & 4 == 0 {
                        file_name = Some(r.string()?);
                    } else {
                        resource_index = Some(r.u32()?);
                    }
                    let node_index = r.u32()?;
                    if version >= 5 {
                        let _use_file = r.u32()?;
                    }
                    if flags & 4 == 0 {
                        let _folder_index = r.u32()?;
                    }
                    external_nodes.push(ExternalNode {
                        flags,
                        file_name,
                        resource_index,
                        node_index,
                    });
                }
            }
        }

        let (body, body_uncompressed_size, body_complete) = if body_comp == 'C' {
            let uncompressed = r.u32()? as usize;
            let compressed = r.u32()? as usize;
            let available = r.remaining().min(compressed);
            let complete_available = available == compressed;
            let decoded = lzo::decompress(r.take(available, "compressed body")?)?;
            let complete =
                decoded.complete && complete_available && decoded.data.len() == uncompressed;
            (decoded.data, uncompressed, complete)
        } else {
            let rest = r.rest().to_vec();
            let n = rest.len();
            (rest, n, true)
        };

        Ok(Gbx {
            version,
            format,
            reference_table_compression: ref_comp,
            body_compression: body_comp,
            edit_state,
            class_id,
            user_data,
            header_chunks,
            num_nodes,
            external_nodes,
            body,
            body_uncompressed_size,
            body_complete,
        })
    }
}

fn parse_header_chunks(block: &[u8]) -> Result<Vec<HeaderChunk>> {
    if block.len() < 4 {
        return Ok(Vec::new());
    }
    let mut r = Reader::new(block);
    let count = r.u32()?;
    let mut entries = Vec::with_capacity(count.min(4096) as usize);
    for _ in 0..count {
        let id = r.u32()?;
        // Bit 31 marks a "heavy" header chunk, which the game skips when
        // scanning files; it is not part of the size.
        let size = (r.u32()? & 0x7FFF_FFFF) as usize;
        entries.push((id, size));
    }
    let mut chunks = Vec::with_capacity(entries.len());
    for (id, size) in entries {
        if r.remaining() < size {
            break;
        }
        let data = r.take(size, "header chunk data")?.to_vec();
        chunks.push(HeaderChunk { id, data });
    }
    Ok(chunks)
}

/// Reads the folder tree. It is stored as a pre-order sequence: each folder
/// starts with the number of its children, each child with its name and its
/// own child count. An explicit worklist is used instead of recursion so that
/// a crafted file cannot overflow the stack.
fn read_folders(r: &mut Reader<'_>) -> Result<()> {
    let mut pending = vec![r.u32()? as usize];
    while let Some(count) = pending.last_mut() {
        if *count == 0 {
            pending.pop();
            continue;
        }
        *count -= 1;
        let _name = r.string()?;
        let children = r.u32()? as usize;
        pending.push(children);
    }
    Ok(())
}

/// Description of the chunk at the reader's current position.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: u32,
    /// `Some(size)` when the chunk carried a SKIP marker, meaning its payload
    /// can be skipped without knowing its layout.
    pub skippable_size: Option<usize>,
    pub data_start: usize,
}

/// Reads the next chunk header. Returns `None` at the end-of-node marker.
pub fn read_chunk(r: &mut Reader<'_>) -> Result<Option<Chunk>> {
    if r.remaining() < 4 {
        return Ok(None);
    }
    let id = r.u32()?;
    if id == FACADE {
        return Ok(None);
    }
    let data_start = r.pos();
    let skippable_size = if r.remaining() >= 4 {
        let save = r.pos();
        let marker = r.u32()?;
        if marker == SKIP_MARKER {
            let size = r.u32()? as usize;
            Some(size)
        } else {
            r.set_pos(save);
            None
        }
    } else {
        None
    };
    Ok(Some(Chunk {
        id,
        skippable_size,
        data_start,
    }))
}

/// Skips the payload of a skippable chunk.
pub fn skip_payload(r: &mut Reader<'_>, size: usize) -> Result<()> {
    r.skip(size)
}

/// True when the four bytes at the reader position look like a chunk id
/// (engine in 0x00..=0x0A) that is followed by a SKIP marker, or the
/// end-of-node marker.
pub fn looks_like_chunk_start(r: &Reader<'_>) -> bool {
    if r.remaining() < 4 {
        return false;
    }
    let mut probe = r.clone_for_probe();
    let id = match probe.u32() {
        Ok(v) => v,
        Err(_) => return false,
    };
    if id == FACADE {
        return true;
    }
    if id >> 24 > 0x0A {
        return false;
    }
    if probe.remaining() < 4 {
        return false;
    }
    matches!(probe.u32(), Ok(marker) if marker == SKIP_MARKER)
}

/// Scans forward for the next plausible chunk boundary so that an unknown
/// non-skippable chunk does not abort the whole node. Returns the number of
/// bytes that were skipped over.
pub fn resync_to_next_chunk(r: &mut Reader<'_>, max_scan: usize) -> Result<Option<usize>> {
    let start = r.pos();
    for offset in 0..max_scan {
        r.set_pos(start + offset);
        if r.remaining() < 8 {
            break;
        }
        if looks_like_chunk_start(r) {
            let skipped = offset;
            r.set_pos(start + offset);
            return Ok(Some(skipped));
        }
    }
    r.set_pos(start);
    Ok(None)
}
