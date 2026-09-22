use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use mlua::chunk::ChunkMode;

use crate::project::{CONTAINER_EXTENSION, ContainerInfo, file_stem};
use crate::vfs::{self, EntryKind, LayeredVfs, Pak, PakEntry, Vfs};

struct Preloaded {
    pak: Pak,
    scripts: HashMap<String, (PakEntry, Vec<u8>)>,
}

impl Preloaded {
    fn new(pak: Pak) -> io::Result<Self> {
        let scripts = pak
            .entries()
            .filter(|(_, entry)| entry.kind == EntryKind::Bytecode)
            .map(|(path, _)| Ok((path.to_owned(), pak.stored(path)?)))
            .collect::<io::Result<HashMap<String, (PakEntry, Vec<u8>)>>>()?;
        Ok(Self { pak, scripts })
    }
}

impl Vfs for Preloaded {
    fn is_file(&self, path: &str) -> bool {
        self.pak.is_file(path)
    }

    fn is_dir(&self, path: &str) -> bool {
        self.pak.is_dir(path)
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        match vfs::normalize(path).and_then(|path| self.scripts.get(&path)) {
            Some((entry, stored)) => Pak::decode(entry, stored.clone()),
            None => self.pak.read(path),
        }
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<String>> {
        self.pak.read_dir(path)
    }

    fn file_size(&self, path: &str) -> Option<u64> {
        self.pak.file_size(path)
    }

    fn chunk_mode(&self, path: &str) -> ChunkMode {
        self.pak.chunk_mode(path)
    }
}

pub struct LoadedContainer {
    pub id: String,
    pub info: ContainerInfo,
    pub path: PathBuf,
}

pub struct Containers {
    vfs: Arc<LayeredVfs>,
    directories: Vec<PathBuf>,
    found: Mutex<Vec<(String, PathBuf)>>,
    loaded: Mutex<Vec<Arc<LoadedContainer>>>,
    loading: Mutex<()>,
}

fn scan(directories: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        let mut here: Vec<(String, PathBuf)> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let extension = path.extension()?.to_str()?;
                if !extension.eq_ignore_ascii_case(CONTAINER_EXTENSION) || !path.is_file() {
                    return None;
                }
                Some((path.file_stem()?.to_str()?.to_owned(), path))
            })
            .collect();
        here.sort();
        for (id, path) in here {
            if !found.iter().any(|(known, _)| known.eq_ignore_ascii_case(&id)) {
                found.push((id, path));
            }
        }
    }
    found
}

fn matches(id: &str, name: &str) -> bool {
    id.eq_ignore_ascii_case(name) || id.eq_ignore_ascii_case(&file_stem(name))
}

impl Containers {
    pub fn new(vfs: Arc<LayeredVfs>, directories: Vec<PathBuf>) -> Self {
        Self {
            vfs,
            directories,
            found: Mutex::new(Vec::new()),
            loaded: Mutex::new(Vec::new()),
            loading: Mutex::new(()),
        }
    }

    pub fn names(&self) -> Vec<String> {
        self.found
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn refresh(&self) -> Vec<String> {
        let found = scan(&self.directories);
        *self.found.lock().unwrap_or_else(PoisonError::into_inner) = found;
        self.names()
    }

    pub fn exists(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    fn find(&self, name: &str) -> Option<(String, PathBuf)> {
        self.found
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(id, _)| matches(id, name))
            .cloned()
    }

    pub fn loaded(&self, name: &str) -> Option<Arc<LoadedContainer>> {
        self.loaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|container| matches(&container.id, name))
            .cloned()
    }

    pub fn load(&self, name: &str) -> Result<Arc<LoadedContainer>, String> {
        let _loading = self.loading.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(existing) = self.loaded(name) {
            return Ok(existing);
        }
        let (id, path) = self
            .find(name)
            .ok_or_else(|| format!("no container named {name} was found next to the game"))?;
        let pak = Pak::open(&path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
        let info = ContainerInfo::from_manifest(pak.manifest())
            .map_err(|error| format!("{} is not a container: {error:#}", path.display()))?;
        if !pak.is_file(&info.main) {
            return Err(format!("{} is missing its main script {}", path.display(), info.main));
        }
        let layer = Preloaded::new(pak).map_err(|error| format!("cannot read the scripts in {}: {error}", path.display()))?;
        self.vfs.mount(Arc::new(layer));
        let loaded = Arc::new(LoadedContainer { id, info, path });
        self.loaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(loaded.clone());
        Ok(loaded)
    }

    pub fn entry(&self, alias: &str) -> Option<String> {
        self.loaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|container| container.id.eq_ignore_ascii_case(alias))
            .map(|container| container.info.main.clone())
    }
}
