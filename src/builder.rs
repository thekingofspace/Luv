use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow, bail};
use tokio::task::{self, JoinSet};

use crate::plugins::Built;
use crate::project::{CONTAINER_FILE, ContainerProject, Project, inside_any, is_native_library, is_packable, is_script};
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

pub async fn build(project: &Project) -> Result<BuildReport> {
    let entry = project.entry()?;
    let containers = project.container_folders();
    let (files, unpacked_libraries) = collect_files(&project.root, project.output_in_workspace().as_deref(), &containers)?;
    if !files.iter().any(|(path, _)| *path == entry) {
        bail!("main script `{entry}` does not exist in {}", project.root.display());
    }
    let mut game = project.manifest.game.clone();
    game.main = entry;
    let manifest = game.to_manifest()?;
    let mut report = pack(files, manifest, project.package_path()).await?;
    report.unpacked_libraries = unpacked_libraries;
    Ok(report)
}

pub async fn build_containers(project: &Project, natives: &[Built]) -> Result<Vec<ContainerReport>> {
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
        reports.push(build_container(&container, files, natives, &output).await?);
    }
    Ok(reports)
}

async fn build_container(
    container: &ContainerProject,
    files: Vec<(String, PathBuf)>,
    natives: &[Built],
    output: &Path,
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
    let report = pack(files, manifest, target.clone()).await?;
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

async fn pack(files: Vec<(String, PathBuf)>, manifest: String, target: PathBuf) -> Result<BuildReport> {
    let mut tasks = JoinSet::new();
    for (path, source) in files {
        tasks.spawn_blocking(move || pack_file(path, &source));
    }

    let mut packed = Vec::new();
    let mut errors = Vec::new();
    while let Some(result) = tasks.join_next().await {
        match result? {
            Ok(file) => packed.push(file),
            Err(err) => errors.push(format!("{err:#}")),
        }
    }
    if !errors.is_empty() {
        errors.sort();
        bail!("build failed:\n{}", errors.join("\n"));
    }
    packed.sort_by(|a, b| a.path.cmp(&b.path));
    task::spawn_blocking(move || write_package(target, packed, &manifest)).await?
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

fn pack_file(path: String, source: &Path) -> Result<PackedFile> {
    let raw = fs::read(source).with_context(|| format!("failed to read {path}"))?;
    let (kind, data) = if is_script(&path) {
        let bundle = script::compile(&raw).map_err(|error| anyhow!("{path}:{error}"))?;
        (EntryKind::Bytecode, bundle)
    } else {
        (EntryKind::Asset, raw)
    };
    PackedFile::pack(path.as_str(), kind, data, COMPRESSION_LEVEL)
        .with_context(|| format!("failed to compress {path}"))
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
