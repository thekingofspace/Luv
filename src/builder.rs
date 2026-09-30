use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use tokio::task::{self, JoinSet};

use crate::plugins::{self, Built, EMBEDDED_DIR};
use crate::progress::Progress;
use crate::project::{CONTAINER_FILE, ContainerProject, Project, inside_any, is_native_library, is_packable, is_script};
use crate::runtime::BootScripts;
use crate::script;
use crate::vfs::{self, EntryKind, PackedFile, Pak, PakWriter};

const COMPRESSION_LEVEL: i32 = 10;
const COLLISIONS_SHOWN: usize = 8;

pub struct BuildReport {
    pub package: PathBuf,
    pub scripts: usize,
    pub assets: usize,
    pub raw_bytes: u64,
    pub package_bytes: u64,
    pub unpacked_libraries: Vec<String>,
}

pub struct ContainerReport {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub rebuilt: bool,
    pub build: Option<BuildReport>,
}

fn read_head(path: &Path) -> Vec<u8> {
    use std::io::Read;
    let mut head = Vec::new();
    if let Ok(file) = File::open(path) {
        let _ = file.take(16 * 1024).read_to_end(&mut head);
    }
    head
}

fn scan_boot(files: &[(String, PathBuf)], entry: &str) -> BootScripts {
    let mut found = BootScripts::default();
    let mut scripts: Vec<&(String, PathBuf)> = files
        .iter()
        .filter(|(path, _)| is_script(path) && path != entry)
        .collect();
    scripts.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, source) in scripts {
        let headers = script::headers(&read_head(source));
        if headers.start_async {
            found.start_async.push(path.clone());
        } else if headers.start {
            found.start.push(path.clone());
        }
        if headers.boot {
            found.boot.push(path.clone());
        }
        if headers.boot_ready {
            found.boot_ready.push(path.clone());
        }
    }
    found
}

pub fn boot_scripts(project: &Project) -> Result<BootScripts> {
    let entry = project.entry()?;
    let containers = project.container_folders();
    let (files, _) = collect_files(&project.root, project.output_in_workspace().as_deref(), &containers)?;
    let mut game = project.manifest.game.clone();
    game.set_boot_scripts(&scan_boot(&files, &entry));
    Ok(game.boot_scripts())
}

pub async fn build(project: &Project) -> Result<BuildReport> {
    build_with(project, &Progress::quiet()).await
}

pub async fn build_with(project: &Project, progress: &Progress) -> Result<BuildReport> {
    let entry = project.entry()?;
    let containers = project.container_folders();
    let (mut files, unpacked_libraries) =
        collect_files(&project.root, project.output_in_workspace().as_deref(), &containers)?;
    if !files.iter().any(|(path, _)| *path == entry) {
        bail!("main script `{entry}` does not exist in {}", project.root.display());
    }
    let mut game = project.manifest.game.clone();
    game.set_boot_scripts(&scan_boot(&files, &entry));
    let workspace = project.clone();
    let embedded = task::spawn_blocking(move || plugins::build_embedded(&workspace)).await??;
    for library in embedded {
        progress.line(format!("Embedded native library {}", library.file));
        files.push((format!("{EMBEDDED_DIR}/{}", library.file), library.path));
    }
    game.main = entry;
    let manifest = game.to_manifest()?;
    let mut report = pack(files, manifest, project.package_path(), progress, "").await?;
    report.unpacked_libraries = unpacked_libraries;
    Ok(report)
}

pub async fn build_containers(project: &Project, natives: &[Built]) -> Result<Vec<ContainerReport>> {
    build_containers_with(project, natives, &Progress::quiet()).await
}

pub async fn build_containers_with(
    project: &Project,
    natives: &[Built],
    progress: &Progress,
) -> Result<Vec<ContainerReport>> {
    let containers = project.containers()?;
    if containers.is_empty() {
        return Ok(Vec::new());
    }
    let folders: Vec<String> = containers.iter().map(|container| container.folder.clone()).collect();
    let (game_files, _) = collect_files(&project.root, project.output_in_workspace().as_deref(), &folders)?;
    let mut owners: HashMap<String, String> = game_files.into_iter().map(|(path, _)| (path, "the game".to_owned())).collect();
    let mut collisions = Vec::new();
    let mut planned = Vec::new();
    for container in containers {
        let (files, _) = collect_files(&container.root, None, &[])?;
        for (path, _) in &files {
            let owner = format!("container {}", container.info.name);
            if let Some(previous) = owners.insert(path.clone(), owner.clone()) {
                collisions.push(format!("{path} is in both {previous} and {owner}"));
            }
        }
        planned.push((container, files));
    }
    if !collisions.is_empty() {
        let hidden = collisions.len().saturating_sub(COLLISIONS_SHOWN);
        collisions.truncate(COLLISIONS_SHOWN);
        let mut message = format!(
            "containers share the game's root folder, so their files cannot use paths the game or another container already uses, give each container its own folders such as src/<container>/ and assets/<container>/:\n  {}",
            collisions.join("\n  ")
        );
        if hidden > 0 {
            message.push_str(&format!("\n  and {hidden} more"));
        }
        bail!(message);
    }
    let output = project.output_dir();
    let mut reports = Vec::new();
    for (container, files) in planned {
        reports.push(build_container(&container, files, natives, &output, progress).await?);
    }
    Ok(reports)
}

async fn build_container(
    container: &ContainerProject,
    files: Vec<(String, PathBuf)>,
    natives: &[Built],
    output: &Path,
    progress: &Progress,
) -> Result<ContainerReport> {
    let entry = container.entry()?;
    if !files.iter().any(|(path, _)| *path == entry) {
        bail!(
            "the main script `{entry}` of container {} does not exist in {}",
            container.info.name,
            container.root.display()
        );
    }
    let id = container.id();
    let mut info = container.info.clone();
    info.main = entry;
    info.natives = natives
        .iter()
        .filter(|library| library.container.as_deref() == Some(id.as_str()))
        .map(|library| library.file.clone())
        .collect();
    let manifest = info.to_manifest()?;
    let target = container.package_path(output);
    let mut inputs: Vec<PathBuf> = files.iter().map(|(_, path)| path.clone()).collect();
    inputs.push(container.root.join(CONTAINER_FILE));
    inputs.extend(std::env::current_exe().ok());
    let current = Pak::open(&target)
        .ok()
        .is_some_and(|pak| pak.manifest() == manifest && fresh(&target, &inputs));
    if current {
        return Ok(ContainerReport {
            name: info.name,
            version: info.version,
            path: target,
            rebuilt: false,
            build: None,
        });
    }
    let label = format!("Container {} ", info.name);
    let report = pack(files, manifest, target.clone(), progress, &label).await?;
    Ok(ContainerReport {
        name: info.name,
        version: info.version,
        path: target,
        rebuilt: true,
        build: Some(report),
    })
}

fn fresh(output: &Path, inputs: &[PathBuf]) -> bool {
    let modified = |path: &Path| fs::metadata(path).and_then(|metadata| metadata.modified()).ok();
    let Some(built) = modified(output) else {
        return false;
    };
    inputs
        .iter()
        .all(|input| modified(input).is_none_or(|changed: SystemTime| changed <= built))
}

async fn pack(
    files: Vec<(String, PathBuf)>,
    manifest: String,
    target: PathBuf,
    progress: &Progress,
    label: &str,
) -> Result<BuildReport> {
    let (scripts, assets): (Vec<_>, Vec<_>) = files.into_iter().partition(|(path, _)| is_script(path));
    let total = scripts.len() + assets.len();
    let mut packed = Vec::with_capacity(total);
    let mut errors = Vec::new();
    for (stage, group) in [("Scripts", scripts), ("Assets", assets)] {
        let count = group.len();
        let mut tasks = JoinSet::new();
        for (path, source) in group {
            tasks.spawn_blocking(move || pack_file(path, &source));
        }
        let name = format!("{label}{}", stage.to_ascii_lowercase());
        while let Some(result) = tasks.join_next().await {
            match result? {
                Ok((file, capture)) => {
                    if let Some(line) = capture {
                        progress.line(line);
                    }
                    packed.push(file);
                }
                Err(err) => errors.push(format!("{err:#}")),
            }
            progress.advance(&name, packed.len() + errors.len(), total);
        }
        if count > 0 && errors.is_empty() {
            progress.line(format!("{label}{stage} complete ({count})"));
        }
    }
    progress.done();
    if !errors.is_empty() {
        errors.sort();
        bail!("build failed:\n{}", errors.join("\n"));
    }
    packed.sort_by(|a, b| a.path.cmp(&b.path));
    let report = task::spawn_blocking(move || write_package(target, packed, &manifest)).await??;
    progress.line(format!("{label}Package written"));
    Ok(report)
}

type Collected = (Vec<(String, PathBuf)>, Vec<String>);

fn collect_files(root: &Path, output: Option<&str>, excluded: &[String]) -> Result<Collected> {
    let mut files = Vec::new();
    let mut libraries = Vec::new();
    let mut pending = vec![(String::new(), root.to_path_buf())];
    while let Some((prefix, dir)) = pending.pop() {
        let entries = fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?;
        for entry in entries {
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .with_context(|| format!("{} does not have a UTF-8 name", path.display()))?;
            let relative = vfs::join(&prefix, name);
            if inside_any(&relative, excluded) {
                continue;
            }
            if is_native_library(name) && path.is_file() && output != Some(prefix.as_str()) {
                libraries.push(relative);
                continue;
            }
            if !is_packable(&relative, name) || output == Some(relative.as_str()) {
                continue;
            }
            if path.is_dir() {
                pending.push((relative, path));
            } else if path.is_file() {
                files.push((relative, path));
            }
        }
    }
    libraries.sort();
    Ok((files, libraries))
}

fn pack_file(path: String, source: &Path) -> Result<(PackedFile, Option<String>)> {
    let raw = fs::read(source).with_context(|| format!("failed to read {path}"))?;
    let mut capture = None;
    let (kind, data) = if is_script(&path) {
        let started = Instant::now();
        let bundle = script::compile(&raw).map_err(|error| anyhow!("{path}:{error}"))?;
        if let Some(message) = script::headers(&raw).capture {
            let units = bundle.first_chunk::<4>().map_or(1, |count| u32::from_le_bytes(*count) as usize);
            capture = Some(script::capture_line(
                message.as_deref(),
                &script::Compiled {
                    path: &path,
                    seconds: started.elapsed().as_secs_f64(),
                    chunks: units.saturating_sub(1),
                    lines: raw.iter().filter(|byte| **byte == b'\n').count() + 1,
                    bytes: raw.len(),
                },
            ));
        }
        (EntryKind::Bytecode, bundle)
    } else {
        (EntryKind::Asset, raw)
    };
    let file = PackedFile::pack(path.as_str(), kind, data, COMPRESSION_LEVEL)
        .with_context(|| format!("failed to compress {path}"))?;
    Ok((file, capture))
}

fn write_package(package: PathBuf, files: Vec<PackedFile>, manifest: &str) -> Result<BuildReport> {
    let dir = package.parent().context("package path has no parent directory")?;
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;

    let extension = package
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = package.with_extension(format!("{extension}.tmp"));
    let mut report = BuildReport {
        package,
        scripts: 0,
        assets: 0,
        raw_bytes: 0,
        package_bytes: 0,
        unpacked_libraries: Vec::new(),
    };

    let written = (|| -> Result<()> {
        let mut writer = PakWriter::new(BufWriter::new(File::create(&temp)?));
        for file in files {
            match file.kind {
                EntryKind::Bytecode => report.scripts += 1,
                EntryKind::Asset => report.assets += 1,
            }
            report.raw_bytes += file.raw_len;
            writer.push(file)?;
        }
        let file = writer.finish(manifest)?.into_inner().map_err(|err| err.into_error())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &report.package)?;
        Ok(())
    })();

    if let Err(err) = written {
        let _ = fs::remove_file(&temp);
        return Err(err.context(format!("failed to write {}", report.package.display())));
    }

    report.package_bytes = fs::metadata(&report.package)?.len();
    Ok(report)
}
