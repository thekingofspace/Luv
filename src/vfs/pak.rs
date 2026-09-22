use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use mlua::chunk::ChunkMode;

use super::{Vfs, normalize, not_found};

const MAGIC: [u8; 8] = *b"LUVIT\0\0\0";
const VERSION: u32 = 2;
const FOOTER_LEN: u64 = 36;
const INDEX_COMPRESSION_LEVEL: i32 = 19;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Asset,
    Bytecode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Stored,
    Zstd,
}

#[derive(Clone, Copy, Debug)]
pub struct PakEntry {
    pub kind: EntryKind,
    pub codec: Codec,
    pub offset: u64,
    pub stored_len: u64,
    pub raw_len: u64,
}

pub struct PackedFile {
    pub path: String,
    pub kind: EntryKind,
    pub codec: Codec,
    pub raw_len: u64,
    pub data: Vec<u8>,
}

impl PackedFile {
    pub fn pack(path: impl Into<String>, kind: EntryKind, raw: Vec<u8>, level: i32) -> io::Result<Self> {
        let raw_len = raw.len() as u64;
        let compressed = zstd::bulk::compress(&raw, level)?;
        let (codec, data) = if compressed.len() < raw.len() {
            (Codec::Zstd, compressed)
        } else {
            (Codec::Stored, raw)
        };
        Ok(Self {
            path: path.into(),
            kind,
            codec,
            raw_len,
            data,
        })
    }
}

pub struct PakWriter<W: Write> {
    out: W,
    position: u64,
    entries: Vec<(String, PakEntry)>,
    paths: HashSet<String>,
}

impl<W: Write> PakWriter<W> {
    pub fn new(out: W) -> Self {
        Self {
            out,
            position: 0,
            entries: Vec::new(),
            paths: HashSet::new(),
        }
    }

    pub fn push(&mut self, file: PackedFile) -> io::Result<()> {
        let path = normalize(&file.path)
            .filter(|path| !path.is_empty() && path.len() <= u16::MAX as usize)
            .ok_or_else(|| invalid(format!("`{}` is not a valid package path", file.path)))?;
        if !self.paths.insert(path.clone()) {
            return Err(invalid(format!("`{path}` was added to the package twice")));
        }
        self.out.write_all(&file.data)?;
        let stored_len = file.data.len() as u64;
        self.entries.push((
            path,
            PakEntry {
                kind: file.kind,
                codec: file.codec,
                offset: self.position,
                stored_len,
                raw_len: file.raw_len,
            },
        ));
        self.position += stored_len;
        Ok(())
    }

    pub fn finish(mut self, manifest: &str) -> io::Result<W> {
        let mut index = Vec::new();
        index.extend((manifest.len() as u32).to_le_bytes());
        index.extend(manifest.as_bytes());
        index.extend((self.entries.len() as u32).to_le_bytes());
        for (path, entry) in &self.entries {
            index.extend((path.len() as u16).to_le_bytes());
            index.extend(path.as_bytes());
            index.push(entry.kind.into());
            index.push(entry.codec.into());
            index.extend(entry.offset.to_le_bytes());
            index.extend(entry.stored_len.to_le_bytes());
            index.extend(entry.raw_len.to_le_bytes());
        }
        let index = zstd::bulk::compress(&index, INDEX_COMPRESSION_LEVEL)?;
        let index_offset = self.position;
        let index_len = index.len() as u64;
        self.out.write_all(&index)?;

        let mut footer = Vec::with_capacity(FOOTER_LEN as usize);
        footer.extend(index_offset.to_le_bytes());
        footer.extend(index_len.to_le_bytes());
        footer.extend((index_offset + index_len + FOOTER_LEN).to_le_bytes());
        footer.extend(VERSION.to_le_bytes());
        footer.extend(MAGIC);
        self.out.write_all(&footer)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

pub struct Pak {
    file: Mutex<File>,
    base: u64,
    manifest: String,
    entries: HashMap<String, PakEntry>,
    children: HashMap<String, Vec<String>>,
}

impl Pak {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let file_len = file.seek(SeekFrom::End(0))?;
        if file_len < FOOTER_LEN {
            return Err(invalid("file is not a luv package"));
        }

        file.seek(SeekFrom::End(-(FOOTER_LEN as i64)))?;
        let mut footer = [0u8; FOOTER_LEN as usize];
        file.read_exact(&mut footer)?;
        let mut cursor = &footer[..];
        let index_offset = u64::from_le_bytes(take(&mut cursor)?);
        let index_len = u64::from_le_bytes(take(&mut cursor)?);
        let pak_len = u64::from_le_bytes(take(&mut cursor)?);
        let version = u32::from_le_bytes(take(&mut cursor)?);
        if take::<8>(&mut cursor)? != MAGIC {
            return Err(invalid("file is not a luv package"));
        }
        if version != VERSION {
            return Err(invalid(format!("unsupported package version {version}")));
        }
        if pak_len > file_len || index_offset.checked_add(index_len).and_then(|end| end.checked_add(FOOTER_LEN)) != Some(pak_len) {
            return Err(corrupt());
        }

        let base = file_len - pak_len;
        file.seek(SeekFrom::Start(base + index_offset))?;
        let mut index = vec![0u8; index_len as usize];
        file.read_exact(&mut index)?;
        let index = zstd::decode_all(&index[..])?;

        let mut cursor = &index[..];
        let manifest_len = u32::from_le_bytes(take(&mut cursor)?) as usize;
        let manifest = take_string(&mut cursor, manifest_len)?;
        let count = u32::from_le_bytes(take(&mut cursor)?) as usize;

        let mut entries = HashMap::with_capacity(count);
        let mut children: HashMap<String, BTreeSet<String>> = HashMap::from([(String::new(), BTreeSet::new())]);
        for _ in 0..count {
            let path_len = u16::from_le_bytes(take(&mut cursor)?) as usize;
            let path = take_string(&mut cursor, path_len)?;
            let [kind, codec] = take(&mut cursor)?;
            let entry = PakEntry {
                kind: kind.try_into()?,
                codec: codec.try_into()?,
                offset: u64::from_le_bytes(take(&mut cursor)?),
                stored_len: u64::from_le_bytes(take(&mut cursor)?),
                raw_len: u64::from_le_bytes(take(&mut cursor)?),
            };
            if entry.offset.checked_add(entry.stored_len).is_none_or(|end| end > index_offset) {
                return Err(corrupt());
            }
            let mut child = path.as_str();
            loop {
                let (dir, name) = child.rsplit_once('/').unwrap_or(("", child));
                children.entry(dir.to_owned()).or_default().insert(name.to_owned());
                if dir.is_empty() {
                    break;
                }
                child = dir;
            }
            entries.insert(path, entry);
        }
        let children = children
            .into_iter()
            .map(|(dir, names)| (dir, names.into_iter().collect()))
            .collect();

        Ok(Self {
            file: Mutex::new(file),
            base,
            manifest,
            entries,
            children,
        })
    }

    pub fn manifest(&self) -> &str {
        &self.manifest
    }

    pub fn start(&self) -> u64 {
        self.base
    }

    pub fn entry(&self, path: &str) -> Option<&PakEntry> {
        self.entries.get(&normalize(path)?)
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, &PakEntry)> {
        self.entries.iter().map(|(path, entry)| (path.as_str(), entry))
    }

    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let (entry, stored) = self.stored(path)?;
        Pak::decode(&entry, stored)
    }

    pub fn stored(&self, path: &str) -> io::Result<(PakEntry, Vec<u8>)> {
        let entry = *self.entry(path).ok_or_else(|| not_found(path))?;
        let mut stored = vec![0u8; entry.stored_len as usize];
        let mut file = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        file.seek(SeekFrom::Start(self.base + entry.offset))?;
        file.read_exact(&mut stored)?;
        Ok((entry, stored))
    }

    pub fn decode(entry: &PakEntry, stored: Vec<u8>) -> io::Result<Vec<u8>> {
        let data = match entry.codec {
            Codec::Stored => stored,
            Codec::Zstd => zstd::bulk::decompress(&stored, entry.raw_len as usize)?,
        };
        if data.len() as u64 != entry.raw_len {
            return Err(corrupt());
        }
        Ok(data)
    }
}

impl Vfs for Pak {
    fn is_file(&self, path: &str) -> bool {
        self.entry(path).is_some()
    }

    fn is_dir(&self, path: &str) -> bool {
        normalize(path).is_some_and(|path| self.children.contains_key(&path))
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        Pak::read(self, path)
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<String>> {
        normalize(path)
            .and_then(|path| self.children.get(&path))
            .cloned()
            .ok_or_else(|| not_found(path))
    }

    fn file_size(&self, path: &str) -> Option<u64> {
        self.entry(path).map(|entry| entry.raw_len)
    }

    fn chunk_mode(&self, path: &str) -> ChunkMode {
        match self.entry(path).map(|entry| entry.kind) {
            Some(EntryKind::Bytecode) => ChunkMode::Binary,
            _ => ChunkMode::Text,
        }
    }
}

impl From<EntryKind> for u8 {
    fn from(kind: EntryKind) -> Self {
        match kind {
            EntryKind::Asset => 0,
            EntryKind::Bytecode => 1,
        }
    }
}

impl TryFrom<u8> for EntryKind {
    type Error = io::Error;

    fn try_from(value: u8) -> io::Result<Self> {
        match value {
            0 => Ok(EntryKind::Asset),
            1 => Ok(EntryKind::Bytecode),
            _ => Err(corrupt()),
        }
    }
}

impl From<Codec> for u8 {
    fn from(codec: Codec) -> Self {
        match codec {
            Codec::Stored => 0,
            Codec::Zstd => 1,
        }
    }
}

impl TryFrom<u8> for Codec {
    type Error = io::Error;

    fn try_from(value: u8) -> io::Result<Self> {
        match value {
            0 => Ok(Codec::Stored),
            1 => Ok(Codec::Zstd),
            _ => Err(corrupt()),
        }
    }
}

fn take<const N: usize>(cursor: &mut &[u8]) -> io::Result<[u8; N]> {
    let mut buf = [0u8; N];
    cursor.read_exact(&mut buf).map_err(|_| corrupt())?;
    Ok(buf)
}

fn take_string(cursor: &mut &[u8], len: usize) -> io::Result<String> {
    let (bytes, rest) = cursor.split_at_checked(len).ok_or_else(corrupt)?;
    *cursor = rest;
    String::from_utf8(bytes.to_vec()).map_err(|_| corrupt())
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn corrupt() -> io::Error {
    invalid("package is corrupt")
}
