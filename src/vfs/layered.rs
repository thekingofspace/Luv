use std::collections::BTreeSet;
use std::io;
use std::sync::{Arc, PoisonError, RwLock};

use mlua::chunk::ChunkMode;

use super::{Vfs, not_found};

pub struct LayeredVfs {
    root: Arc<dyn Vfs>,
    layers: RwLock<Vec<Arc<dyn Vfs>>>,
}

impl LayeredVfs {
    pub fn new(root: Arc<dyn Vfs>) -> Self {
        Self {
            root,
            layers: RwLock::new(Vec::new()),
        }
    }

    pub fn mount(&self, layer: Arc<dyn Vfs>) {
        self.layers.write().unwrap_or_else(PoisonError::into_inner).push(layer);
    }

    fn all(&self) -> Vec<Arc<dyn Vfs>> {
        let layers = self.layers.read().unwrap_or_else(PoisonError::into_inner);
        let mut all = Vec::with_capacity(layers.len() + 1);
        all.push(self.root.clone());
        all.extend(layers.iter().cloned());
        all
    }

    fn owner(&self, path: &str) -> Option<Arc<dyn Vfs>> {
        if self.root.is_file(path) {
            return Some(self.root.clone());
        }
        self.layers
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|layer| layer.is_file(path))
            .cloned()
    }
}

impl Vfs for LayeredVfs {
    fn is_file(&self, path: &str) -> bool {
        self.owner(path).is_some()
    }

    fn is_dir(&self, path: &str) -> bool {
        self.all().iter().any(|layer| layer.is_dir(path))
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        self.owner(path).ok_or_else(|| not_found(path))?.read(path)
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<String>> {
        let mut names = BTreeSet::new();
        let mut found = false;
        for layer in self.all() {
            if layer.is_dir(path) {
                found = true;
                names.extend(layer.read_dir(path)?);
            }
        }
        if !found {
            return Err(not_found(path));
        }
        Ok(names.into_iter().collect())
    }

    fn file_size(&self, path: &str) -> Option<u64> {
        self.owner(path)?.file_size(path)
    }

    fn chunk_mode(&self, path: &str) -> ChunkMode {
        self.owner(path).map_or(ChunkMode::Text, |layer| layer.chunk_mode(path))
    }
}
