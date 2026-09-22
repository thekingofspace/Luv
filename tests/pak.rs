use std::fs;
use std::io::Cursor;

use luv::vfs::{Codec, EntryKind, PackedFile, Pak, PakWriter, Vfs};
use mlua::chunk::ChunkMode;

fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

fn sample_package() -> Vec<u8> {
    let mut writer = PakWriter::new(Cursor::new(Vec::new()));
    let files = [
        ("src/main.luau", EntryKind::Bytecode, b"bytecode".repeat(64)),
        ("assets/text/readme.txt", EntryKind::Asset, b"hello ".repeat(500)),
        ("assets/noise.bin", EntryKind::Asset, noise(4096)),
    ];
    for (path, kind, data) in files {
        writer.push(PackedFile::pack(path, kind, data, 3).unwrap()).unwrap();
    }
    writer.finish("name = \"Sample\"\n").unwrap().into_inner()
}

#[test]
fn round_trips_entries_and_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.luvpak");
    fs::write(&path, sample_package()).unwrap();

    let pak = Pak::open(&path).unwrap();
    assert_eq!(pak.manifest(), "name = \"Sample\"\n");
    assert_eq!(pak.entries().count(), 3);
    assert_eq!(pak.read("src/main.luau").unwrap(), b"bytecode".repeat(64));
    assert_eq!(pak.read("./assets/text/../text/readme.txt").unwrap(), b"hello ".repeat(500));
    assert_eq!(pak.read("assets\\noise.bin").unwrap(), noise(4096));
    assert!(pak.read("missing.txt").is_err());
}

#[test]
fn keeps_entries_compressed_until_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.luvpak");
    fs::write(&path, sample_package()).unwrap();

    let pak = Pak::open(&path).unwrap();
    let text = pak.entry("assets/text/readme.txt").unwrap();
    assert_eq!(text.codec, Codec::Zstd);
    assert!(text.stored_len < text.raw_len);

    let noise = pak.entry("assets/noise.bin").unwrap();
    assert_eq!(noise.codec, Codec::Stored);
    assert_eq!(noise.stored_len, noise.raw_len);
}

#[test]
fn exposes_directories_and_chunk_modes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.luvpak");
    fs::write(&path, sample_package()).unwrap();

    let pak = Pak::open(&path).unwrap();
    assert!(pak.is_dir(""));
    assert!(pak.is_dir("assets"));
    assert!(pak.is_dir("assets/text"));
    assert!(!pak.is_dir("assets/noise.bin"));
    assert!(pak.is_file("assets/noise.bin"));
    assert_eq!(pak.chunk_mode("src/main.luau"), ChunkMode::Binary);
    assert_eq!(pak.chunk_mode("assets/text/readme.txt"), ChunkMode::Text);
}

#[test]
fn opens_packages_appended_to_other_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.exe");
    let mut bytes = noise(1000);
    bytes.extend(sample_package());
    fs::write(&path, bytes).unwrap();

    let pak = Pak::open(&path).unwrap();
    assert_eq!(pak.read("assets/noise.bin").unwrap(), noise(4096));
}

#[test]
fn rejects_invalid_packages() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.luvpak");

    fs::write(&path, b"definitely not a package, just some text that is long enough").unwrap();
    assert!(Pak::open(&path).is_err());

    let mut bytes = sample_package();
    let footer = bytes.len() - 36;
    bytes[footer] ^= 0xff;
    fs::write(&path, bytes).unwrap();
    assert!(Pak::open(&path).is_err());
}

#[test]
fn rejects_duplicate_paths() {
    let mut writer = PakWriter::new(Cursor::new(Vec::new()));
    writer
        .push(PackedFile::pack("a.txt", EntryKind::Asset, b"one".to_vec(), 3).unwrap())
        .unwrap();
    let duplicate = PackedFile::pack("./a.txt", EntryKind::Asset, b"two".to_vec(), 3).unwrap();
    assert!(writer.push(duplicate).is_err());
}
