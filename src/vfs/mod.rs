mod dir;
mod layered;
mod pak;

pub use dir::DirVfs;
pub use layered::LayeredVfs;
pub use pak::{Codec, EntryKind, PackedFile, Pak, PakEntry, PakWriter};

use std::io;

use mlua::chunk::ChunkMode;

pub trait Vfs: Send + Sync {
    fn is_file(&self, path: &str) -> bool;

    fn is_dir(&self, path: &str) -> bool;

    fn read(&self, path: &str) -> io::Result<Vec<u8>>;

    fn read_dir(&self, path: &str) -> io::Result<Vec<String>>;

    fn file_size(&self, path: &str) -> Option<u64>;

    fn chunk_mode(&self, path: &str) -> ChunkMode;
}

pub fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

pub fn join(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_owned()
    } else {
        format!("{parent}/{child}")
    }
}

pub(crate) fn not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("`{path}` does not exist"))
}
