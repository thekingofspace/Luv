use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use anyhow::{Context, Result, anyhow, bail};
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, RgbaImage};

use crate::graphics::picture;
use crate::plugins::Built;
use crate::project::{Project, file_stem};
use crate::vfs::Pak;

pub const PACKAGE_DIR: &str = "package";
const PE_POINTER: usize = 0x3c;
const SUBSYSTEM_OFFSET: usize = 4 + 20 + 68;
const SUBSYSTEM_GUI: u16 = 2;
const SUBSYSTEM_CONSOLE: u16 = 3;
const REMEMBERED: usize = 5;
const ICON_SIDE: u32 = 256;
const ICO_MAGIC: [u8; 4] = [0, 0, 1, 0];

static REPORTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub struct PackageReport {
    pub executable: PathBuf,
    pub icon: IconReport,
    pub libraries: Vec<String>,
    pub containers: Vec<String>,
    pub bytes: u64,
}

pub enum IconReport {
    Unset,
    Missing(String),
    Embedded(String),
    Beside { source: String, file: String },
}

enum IconFile {
    Unset,
    Missing(String),
    Found(String, Vec<u8>),
}

fn icon_file(project: &Project) -> Result<IconFile> {
    let Some(icon) = project.manifest.game.icon.as_deref().map(str::trim).filter(|icon| !icon.is_empty()) else {
        return Ok(IconFile::Unset);
    };
    let path = project.root.join(icon);
    match fs::read(&path) {
        Ok(bytes) => Ok(IconFile::Found(icon.to_owned(), bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(IconFile::Missing(icon.to_owned())),
        Err(error) => Err(error).with_context(|| format!("failed to read the game icon {}", path.display())),
    }
}

fn icon_pixels(name: &str, bytes: &[u8]) -> Result<RgbaImage> {
    let pixels = picture::decode_fitted(bytes, Some(name), ICON_SIDE)
        .map_err(|error| anyhow!("the game icon {name} cannot be used: {error}"))?;
    let (side, _, rgba) = picture::square(pixels);
    let image = RgbaImage::from_raw(side, side, rgba).with_context(|| format!("the game icon {name} has no pixels"))?;
    if side > ICON_SIDE {
        return Ok(image::imageops::resize(&image, ICON_SIDE, ICON_SIDE, FilterType::Lanczos3));
    }
    Ok(image)
}

fn embed_icon(engine: &[u8], name: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    let mut program = editpe::Image::parse(engine)
        .map_err(|error| anyhow!("the luv executable cannot be read as a Windows program: {error}"))?;
    let mut resources = program.resource_directory().cloned().unwrap_or_default();
    let applied = if bytes.starts_with(&ICO_MAGIC) {
        picture::decode(bytes, Some(name)).map_err(|error| anyhow!("the game icon {name} cannot be used: {error}"))?;
        resources.set_main_icon(bytes)
    } else {
        resources.set_main_icon(DynamicImage::ImageRgba8(icon_pixels(name, bytes)?))
    };
    applied.map_err(|error| anyhow!("the game icon {name} cannot be used: {error}"))?;
    program
        .set_resource_directory(resources)
        .map_err(|error| anyhow!("the icon could not be added to the game program: {error}"))?;
    let mut output = Vec::with_capacity(engine.len());
    program
        .write_writer(&mut output)
        .map_err(|error| anyhow!("the icon could not be added to the game program: {error}"))?;
    Ok(output)
}

fn write_icon(path: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    icon_pixels(name, bytes)?
        .save_with_format(path, ImageFormat::Png)
        .with_context(|| format!("failed to write the game icon to {}", path.display()))
}

pub fn embedded() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    Pak::open(&executable).ok()?;
    Some(executable)
}

fn engine_bytes() -> Result<Vec<u8>> {
    let engine = std::env::current_exe().context("cannot find the luv executable")?;
    let mut bytes = fs::read(&engine).with_context(|| format!("failed to read {}", engine.display()))?;
    if let Ok(existing) = Pak::open(&engine) {
        bytes.truncate(existing.start() as usize);
    }
    Ok(bytes)
}

fn set_subsystem(bytes: &mut [u8], subsystem: u16) -> Result<()> {
    let header = bytes
        .get(PE_POINTER..PE_POINTER + 4)
        .map(|pointer| u32::from_le_bytes([pointer[0], pointer[1], pointer[2], pointer[3]]) as usize)
        .context("the luv executable is not a Windows program")?;
    if bytes.get(header..header + 4) != Some(b"PE\0\0") {
        bail!("the luv executable is not a Windows program");
    }
    let field = header + SUBSYSTEM_OFFSET;
    let current = bytes
        .get(field..field + 2)
        .map(|value| u16::from_le_bytes([value[0], value[1]]))
        .context("the luv executable is not a Windows program")?;
    if current != SUBSYSTEM_GUI && current != SUBSYSTEM_CONSOLE {
        bail!("the luv executable has an unexpected Windows subsystem ({current})");
    }
    bytes[field..field + 2].copy_from_slice(&subsystem.to_le_bytes());
    Ok(())
}

pub fn package(
    project: &Project,
    game: &Path,
    libraries: &[Built],
    containers: &[PathBuf],
    console: bool,
) -> Result<PackageReport> {
    let icon = icon_file(project)?;
    let mut engine = engine_bytes()?;
    if cfg!(windows) {
        if let IconFile::Found(name, bytes) = &icon {
            engine = embed_icon(&engine, name, bytes)?;
        }
        set_subsystem(&mut engine, if console { SUBSYSTEM_CONSOLE } else { SUBSYSTEM_GUI })?;
    }
    let directory = project.output_dir().join(PACKAGE_DIR);
    if directory.exists() {
        fs::remove_dir_all(&directory).with_context(|| format!("failed to clear {}", directory.display()))?;
    }
    fs::create_dir_all(&directory).with_context(|| format!("failed to create {}", directory.display()))?;

    let stem = file_stem(&project.manifest.game.name);
    let icon = match icon {
        IconFile::Unset => IconReport::Unset,
        IconFile::Missing(name) => IconReport::Missing(name),
        IconFile::Found(name, _) if cfg!(windows) => IconReport::Embedded(name),
        IconFile::Found(name, bytes) => {
            let file = format!("{stem}.png");
            write_icon(&directory.join(&file), &name, &bytes)?;
            IconReport::Beside { source: name, file }
        }
    };
    let executable = directory.join(if cfg!(windows) { format!("{stem}.exe") } else { stem });
    let payload = fs::read(game).with_context(|| format!("failed to read {}", game.display()))?;
    {
        let mut file = File::create(&executable).with_context(|| format!("failed to create {}", executable.display()))?;
        file.write_all(&engine)?;
        file.write_all(&payload)?;
        file.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
    }
    Pak::open(&executable).with_context(|| format!("{} was written but cannot be read back", executable.display()))?;

    let mut shipped = Vec::new();
    for library in libraries {
        let target = directory.join(&library.file);
        fs::copy(&library.path, &target)
            .with_context(|| format!("failed to copy {} next to the game", library.path.display()))?;
        shipped.push(library.file.clone());
    }
    let mut copied = Vec::new();
    for container in containers {
        let name = container
            .file_name()
            .with_context(|| format!("{} is not a container file", container.display()))?;
        fs::copy(container, directory.join(name))
            .with_context(|| format!("failed to copy {} next to the game", container.display()))?;
        copied.push(name.to_string_lossy().into_owned());
    }
    Ok(PackageReport {
        bytes: fs::metadata(&executable)?.len(),
        executable,
        icon,
        libraries: shipped,
        containers: copied,
    })
}

pub fn remember(message: &str) {
    let mut reports = REPORTS.lock().unwrap_or_else(PoisonError::into_inner);
    if reports.len() < REMEMBERED {
        reports.push(message.to_owned());
    }
}

pub fn remembered() -> Vec<String> {
    REPORTS.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

#[cfg(windows)]
pub fn alert(title: &str, message: &str) {
    use std::os::raw::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleWindow() -> *mut c_void;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(window: *mut c_void, text: *const u16, caption: *const u16, style: u32) -> i32;
    }
    const ICON_ERROR: u32 = 0x10;

    if !unsafe { GetConsoleWindow() }.is_null() {
        return;
    }
    let text: Vec<u16> = message.encode_utf16().chain([0]).collect();
    let caption: Vec<u16> = title.encode_utf16().chain([0]).collect();
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), ICON_ERROR) };
}

#[cfg(not(windows))]
pub fn alert(_: &str, _: &str) {}
