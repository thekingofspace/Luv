use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use mlua::chunk::ChunkMode;

use super::{Vfs, join, normalize, not_found};

type Filter = Arc<dyn Fn(&str) -> bool + Send + Sync>;

pub struct DirVfs {
    root: PathBuf,
    filter: Option<Filter>,
}

impl DirVfs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            filter: None,
        }
    }

    pub fn filtered(root: impl Into<PathBuf>, filter: impl Fn(&str) -> bool + Send + Sync + 'static) -> Self {
        Self {
            root: root.into(),
            filter: Some(Arc::new(filter)),
        }
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    fn visible(&self, path: &str) -> bool {
        path.is_empty() || self.filter.as_ref().is_none_or(|filter| filter(path))
    }

    fn resolve(&self, path: &str) -> Option<(String, PathBuf)> {
        let path = normalize(path)?;
        if !self.visible(&path) {
            return None;
        }
        let mut resolved = self.root.clone();
        resolved.extend(path.split('/').filter(|part| !part.is_empty()));
        Some((path, resolved))
    }
}

impl Vfs for DirVfs {
    fn is_file(&self, path: &str) -> bool {
        self.resolve(path).is_some_and(|(_, path)| path.is_file())
    }

    fn is_dir(&self, path: &str) -> bool {
        self.resolve(path).is_some_and(|(_, path)| path.is_dir())
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        fs::read(self.resolve(path).ok_or_else(|| not_found(path))?.1)
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<String>> {
        let (relative, resolved) = self.resolve(path).ok_or_else(|| not_found(path))?;
        let mut names = Vec::new();
        for entry in fs::read_dir(resolved)? {
            let Ok(name) = entry?.file_name().into_string() else { continue };
            if self.visible(&join(&relative, &name)) {
                names.push(name);
            }
        }
        names.sort();
        Ok(names)
    }

    fn file_size(&self, path: &str) -> Option<u64> {
        let (_, resolved) = self.resolve(path)?;
        fs::metadata(resolved)
            .ok()
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len())
    }

    fn chunk_mode(&self, _path: &str) -> ChunkMode {
        ChunkMode::Text
    }
}
