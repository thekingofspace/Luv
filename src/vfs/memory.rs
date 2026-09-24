use std::collections::{BTreeMap, BTreeSet};
use std::io;

use mlua::chunk::ChunkMode;

use super::{Vfs, not_found};

pub struct MemoryVfs {
    files: BTreeMap<String, (Vec<u8>, bool)>,
    dirs: BTreeSet<String>,
}

impl MemoryVfs {
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            dirs: BTreeSet::new(),
        }
    }

    pub fn insert(&mut self, path: impl Into<String>, data: Vec<u8>, compiled: bool) {
        let path = path.into();
        let mut parent = path.as_str();
        while let Some((head, _)) = parent.rsplit_once('/') {
            if !self.dirs.insert(head.to_owned()) {
                break;
            }
            parent = head;
        }
        self.dirs.insert(String::new());
        self.files.insert(path, (data, compiled));
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

impl Default for MemoryVfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Vfs for MemoryVfs {
    fn is_file(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    fn is_dir(&self, path: &str) -> bool {
        self.dirs.contains(path)
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        self.files
            .get(path)
            .map(|(data, _)| data.clone())
            .ok_or_else(|| not_found(path))
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<String>> {
        if !self.is_dir(path) {
            return Err(not_found(path));
        }
        let mut names = BTreeSet::new();
        let prefix = if path.is_empty() { String::new() } else { format!("{path}/") };
        for known in self.files.keys().chain(self.dirs.iter()) {
            let Some(rest) = known.strip_prefix(&prefix) else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            names.insert(rest.split('/').next().unwrap_or(rest).to_owned());
        }
        Ok(names.into_iter().collect())
    }

    fn file_size(&self, path: &str) -> Option<u64> {
        self.files.get(path).map(|(data, _)| data.len() as u64)
    }

    fn chunk_mode(&self, path: &str) -> ChunkMode {
        match self.files.get(path) {
            Some((_, true)) => ChunkMode::Binary,
            _ => ChunkMode::Text,
        }
    }
}
