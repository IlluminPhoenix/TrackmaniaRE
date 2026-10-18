//! Binary reader for GameBox streams, including Nadeo's lookback-string /
//! identifier ("Id") compression.

use crate::error::{Error, Result};
use std::collections::HashMap;

/// State of the lookback-string ("Id") table. The engine keeps one table per
/// node and resets it after every header chunk.
#[derive(Debug, Clone, Default)]
pub struct IdState {
    version: Option<u32>,
    dict: HashMap<u32, String>,
    count: usize,
}

impl IdState {
    /// Reads the one-time lookback-string version marker (value 3).
    fn ensure_version(&mut self, r: &mut Reader<'_>) -> Result<()> {
        if self.version.is_none() {
            self.version = Some(r.u32()?);
        }
        Ok(())
    }

    fn insert(&mut self, str: String) -> String {
        self.count += 1;
        let key = self.count as u32;
        self.dict.insert(key, str.clone());
        str
    }

    pub fn lookup(&self, index: u32) -> Option<&String> {
        self.dict.get(&index)
    }

    /// Reads an index/identifier. Returns either a numeric id (a position in
    /// the global name table, which we cannot resolve) or the string itself.
    pub fn read_id(&mut self, r: &mut Reader<'_>) -> Result<Id> {
        let index = self.read_index(r)?;
        if index == u32::MAX {
            return Ok(Id::None);
        }
        if index & 0xC000_0000 != 0x4000_0000 && index & 0xC000_0000 != 0x8000_0000 {
            // Plain numeric id: position in the global name table.
            return Ok(Id::Index(index));
        }
        Ok(Id::Text(self.read_id_string(r, index)?))
    }

    /// Same as `read_id` but always yields the string form (empty when the id
    /// cannot be resolved locally).
    pub fn read_id_string(&mut self, r: &mut Reader<'_>, index: u32) -> Result<String> {
        if index & 0x3FFF_FFFF != 0 {
            return Ok(self.dict.get(&index).cloned().unwrap_or_default());
        }
        let s = r.string()?;
        Ok(self.insert(s))
    }

    fn read_index(&mut self, r: &mut Reader<'_>) -> Result<u32> {
        self.ensure_version(r)?;
        r.u32()
    }

    pub fn read_ident(&mut self, r: &mut Reader<'_>) -> Result<Ident> {
        let id = self.read_id(r)?;
        let collection = self.read_id(r)?;
        let author_index = self.read_index(r)?;
        let author = self.read_id_string(r, author_index)?;
        Ok(Ident {
            id,
            collection,
            author,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Id {
    None,
    Index(u32),
    Text(String),
}

impl Id {
    pub fn as_text(&self) -> String {
        match self {
            Id::None => String::new(),
            Id::Index(i) => i.to_string(),
            Id::Text(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Ident {
    pub id: Id,
    pub collection: Id,
    pub author: String,
}

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    ids: IdState,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader {
            data,
            pos: 0,
            ids: IdState::default(),
        }
    }

    pub fn with_ids(data: &'a [u8], ids: IdState) -> Self {
        Reader { data, pos: 0, ids }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// The four bytes at an absolute offset, if they are inside the data.
    pub fn data_at(&self, pos: usize) -> [u8; 4] {
        let mut out = [0u8; 4];
        if pos + 4 <= self.data.len() {
            out.copy_from_slice(&self.data[pos..pos + 4]);
        }
        out
    }

    pub fn rest(&self) -> &'a [u8] {
        &self.data[self.pos.min(self.data.len())..]
    }

    /// A reader sharing the same data and identifier state, for probing.
    pub fn clone_for_probe(&self) -> Reader<'_> {
        Reader {
            data: self.data,
            pos: self.pos,
            ids: self.ids.clone(),
        }
    }

    pub fn ids(&self) -> &IdState {
        &self.ids
    }

    /// Adopts the identifier state of a sub-reader so lookback strings read
    /// inside a chunk stay visible to the rest of the node.
    pub fn set_ids(&mut self, ids: IdState) {
        self.ids = ids;
    }

    pub fn take(&mut self, n: usize, what: &str) -> Result<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return Err(Error::eof(what, self.pos));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        if self.pos + n > self.data.len() {
            return Err(Error::eof("skipped data", self.pos));
        }
        self.pos += n;
        Ok(())
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1, "u8")?[0])
    }

    pub fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.take(2, "u16")?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32> {
        let b = self.take(4, "u32")?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn u64(&mut self) -> Result<u64> {
        let b = self.take(8, "u64")?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// GameBox booleans are 32-bit little-endian integers.
    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u32()? != 0)
    }

    /// A boolean stored as a single byte (used by the record data structures).
    pub fn bool_byte(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }

    /// A length-prefixed UTF-8 string.
    pub fn string(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        let bytes = self.take(n, "string")?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    /// A length-prefixed blob (`data` in the wiki).
    pub fn data(&mut self) -> Result<Vec<u8>> {
        let n = self.u32()? as usize;
        Ok(self.take(n, "data blob")?.to_vec())
    }

    pub fn vec3(&mut self) -> Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    /// A 32-bit unix timestamp (seconds) where -1 means "null".
    pub fn nullable_unix_time32(&mut self) -> Result<Option<i64>> {
        let v = self.i32()?;
        Ok(if v == -1 { None } else { Some(v as i64) })
    }

    /// A 64-bit little-endian value where -1 means "null".
    pub fn nullable_unix_time(&mut self) -> Result<Option<i64>> {
        let lo = self.u32()? as u64;
        let hi = self.u32()? as u64;
        let value = (hi << 32) | lo;
        if value == u64::MAX {
            Ok(None)
        } else {
            Ok(Some(value as i64))
        }
    }

    pub fn ident(&mut self) -> Result<Ident> {
        let mut ids = std::mem::take(&mut self.ids);
        let result = ids.read_ident(self);
        self.ids = ids;
        result
    }

    pub fn id(&mut self) -> Result<Id> {
        let mut ids = std::mem::take(&mut self.ids);
        let result = ids.read_id(self);
        self.ids = ids;
        result
    }

    /// Reads an identifier and returns just its text form.
    pub fn id_string(&mut self) -> Result<String> {
        let mut ids = std::mem::take(&mut self.ids);
        let result = ids.read_id(self).map(|id| id.as_text());
        self.ids = ids;
        result
    }

    pub fn fileref(&mut self) -> Result<Fileref> {
        let version = self.u8()?;
        let checksum = self.take(32, "fileref checksum")?.to_vec();
        let path = self.string()?;
        let mut locator = String::new();
        if (!path.is_empty() && version >= 1) || version >= 3 {
            locator = self.string()?;
        }
        Ok(Fileref {
            version,
            checksum,
            path,
            locator,
        })
    }

    /// Creates a reader over the next `n` bytes (used for skippable chunk
    /// payloads), sharing the identifier state of the current node.
    pub fn sub(&mut self, n: usize, what: &str) -> Result<Reader<'a>> {
        let bytes = self.take(n, what)?;
        Ok(Reader::with_ids(bytes, self.ids.clone()))
    }
}

#[derive(Debug, Clone)]
pub struct Fileref {
    pub version: u8,
    pub checksum: Vec<u8>,
    pub path: String,
    pub locator: String,
}
