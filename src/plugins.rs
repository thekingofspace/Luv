use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow, bail};

use crate::project::{HEADER_FILE, Project};

pub const NATIVE_DIR: &str = "native";
const TARGET_DIR: &str = "native-target";
const OBJECTS_DIR: &str = "native-objects";
const SOURCE_EXTENSIONS: [&str; 4] = ["c", "cc", "cpp", "cxx"];

#[cfg(windows)]
pub const PREFIX: &str = "";
#[cfg(not(windows))]
pub const PREFIX: &str = "lib";
#[cfg(windows)]
pub const EXTENSION: &str = "dll";
#[cfg(not(windows))]
pub const EXTENSION: &str = "so";

#[cfg(all(windows, target_arch = "aarch64"))]
const TARGET: &str = "aarch64-pc-windows-msvc";
#[cfg(all(windows, not(target_arch = "aarch64")))]
const TARGET: &str = "x86_64-pc-windows-msvc";
#[cfg(all(not(windows), target_arch = "aarch64"))]
const TARGET: &str = "aarch64-unknown-linux-gnu";
#[cfg(all(not(windows), not(target_arch = "aarch64")))]
const TARGET: &str = "x86_64-unknown-linux-gnu";

pub fn library_file(name: &str) -> String {
    format!("{PREFIX}{name}.{EXTENSION}")
}

pub fn is_library(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    if cfg!(windows) {
        name.ends_with(".dll")
    } else {
        name.ends_with(".so") || name.contains(".so.")
    }
}

fn is_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| SOURCE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

fn is_cpp(path: &Path) -> bool {
    is_source(path) && !path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("c"))
}

#[derive(Debug)]
pub enum Plugin {
    C { name: String, sources: Vec<PathBuf> },
    Rust { directory: PathBuf },
    Prebuilt { path: PathBuf },
}

#[derive(Debug)]
pub struct Built {
    pub file: String,
    pub path: PathBuf,
    pub rebuilt: bool,
    pub container: Option<String>,
}

fn walk(directory: &Path, found: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(directory)?.map(|entry| entry.map(|entry| entry.path())).collect::<io::Result<_>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, found)?;
        } else {
            found.push(path);
        }
    }
    Ok(())
}

pub fn discover(root: &Path) -> Result<Vec<Plugin>> {
    let native = root.join(NATIVE_DIR);
    if !native.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(&native)
        .with_context(|| format!("failed to read {}", native.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<_>>()?;
    entries.sort();
    let mut plugins = Vec::new();
    for path in entries {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_owned)
            .with_context(|| format!("{} does not have a UTF-8 name", path.display()))?;
        if path.is_dir() {
            if path.join("Cargo.toml").is_file() {
                plugins.push(Plugin::Rust { directory: path });
                continue;
            }
            let mut files = Vec::new();
            walk(&path, &mut files).with_context(|| format!("failed to read {}", path.display()))?;
            let sources: Vec<PathBuf> = files.iter().filter(|file| is_source(file)).cloned().collect();
            if !sources.is_empty() {
                let name = path.file_name().and_then(|name| name.to_str()).unwrap_or(&stem).to_owned();
                plugins.push(Plugin::C { name, sources });
            }
            plugins.extend(files.into_iter().filter(|file| is_library(file)).map(|path| Plugin::Prebuilt { path }));
        } else if is_source(&path) {
            plugins.push(Plugin::C {
                name: stem,
                sources: vec![path],
            });
        } else if is_library(&path) {
            plugins.push(Plugin::Prebuilt { path });
        }
    }
    Ok(plugins)
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|metadata| metadata.modified()).ok()
}

fn fresh(output: &Path, inputs: &[PathBuf]) -> bool {
    let Some(built) = modified(output) else {
        return false;
    };
    inputs.iter().all(|input| modified(input).is_none_or(|changed| changed <= built))
}

fn copy_if_changed(from: &Path, to: &Path) -> Result<bool> {
    let same = match (fs::metadata(from), fs::metadata(to)) {
        (Ok(source), Ok(target)) => {
            source.len() == target.len() && source.modified().ok() <= target.modified().ok()
        }
        _ => false,
    };
    if same {
        return Ok(false);
    }
    fs::copy(from, to).with_context(|| format!("failed to copy {} to {}", from.display(), to.display()))?;
    Ok(true)
}

fn run(mut command: Command, what: &str) -> Result<()> {
    let output = command
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("could not start the compiler for {what}"))?;
    if output.status.success() {
        return Ok(());
    }
    let mut log = String::from_utf8_lossy(&output.stdout).into_owned();
    log.push_str(&String::from_utf8_lossy(&output.stderr));
    let log: Vec<&str> = log.lines().filter(|line| !line.trim().is_empty()).collect();
    bail!("{what} did not compile:\n{}", log.join("\n"))
}

pub fn compile_c(name: &str, sources: &[PathBuf], includes: &[PathBuf], output: &Path, objects: &Path) -> Result<()> {
    let what = format!("the native library '{name}'");
    fs::create_dir_all(objects).with_context(|| format!("failed to create {}", objects.display()))?;
    let mut build = cc::Build::new();
    build
        .target(TARGET)
        .host(TARGET)
        .opt_level(2)
        .debug(false)
        .cargo_metadata(false)
        .cargo_warnings(false)
        .cpp(sources.iter().any(|source| is_cpp(source)));
    let compiler = build
        .try_get_compiler()
        .map_err(|error| anyhow!("no C compiler was found to build {what}: {error}"))?;
    let mut command = compiler.to_command();
    if compiler.is_like_msvc() {
        command.args(["/nologo", "/LD"]);
        for include in includes {
            command.arg(format!("/I{}", include.display()));
        }
        command.args(sources);
        command.arg(format!("/Fo{}\\", objects.display()));
        command.arg(format!("/Fe{}", output.display()));
        command.args(["/link", "/NOLOGO"]);
        command.arg(format!("/IMPLIB:{}", objects.join(format!("{name}.lib")).display()));
    } else {
        command.args(["-shared", "-fPIC", "-pthread"]);
        for include in includes {
            command.arg("-I").arg(include);
        }
        command.args(sources);
        command.arg("-o").arg(output);
        command.arg("-lm");
    }
    run(command, &what)
}

fn crate_library(directory: &Path) -> Result<String> {
    let path = directory.join("Cargo.toml");
    let text = fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let manifest: toml::Table = toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
    let lib = manifest.get("lib").and_then(|lib| lib.as_table());
    let dynamic = lib
        .and_then(|lib| lib.get("crate-type"))
        .and_then(|types| types.as_array())
        .is_some_and(|types| types.iter().any(|kind| kind.as_str() == Some("cdylib")));
    if !dynamic {
        bail!(
            "{} needs `crate-type = [\"cdylib\"]` in its [lib] section to build a native library",
            path.display()
        );
    }
    let name = lib
        .and_then(|lib| lib.get("name"))
        .and_then(|name| name.as_str())
        .or_else(|| manifest.get("package")?.get("name")?.as_str())
        .with_context(|| format!("{} has no package name", path.display()))?;
    Ok(name.replace('-', "_"))
}

fn compile_rust(directory: &Path, target: &Path) -> Result<PathBuf> {
    let name = crate_library(directory)?;
    let status = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .arg("build")
        .arg("--release")
        .arg("--manifest-path")
        .arg(directory.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(target)
        .stdin(Stdio::null())
        .status()
        .map_err(|error| {
            anyhow!(
                "cannot run cargo to build {} ({error}), install Rust from https://rustup.rs",
                directory.display()
            )
        })?;
    if !status.success() {
        bail!("cargo could not build {}", directory.display());
    }
    let artifact = target.join("release").join(library_file(&name));
    if !artifact.is_file() {
        bail!("cargo built {} but {} is missing", directory.display(), artifact.display());
    }
    Ok(artifact)
}

pub fn build(project: &Project) -> Result<Vec<Built>> {
    let game_natives = project.root.join(NATIVE_DIR);
    let mut sources = vec![(None, project.root.clone(), vec![game_natives.clone()])];
    for container in project.containers()? {
        let includes = vec![container.root.join(NATIVE_DIR), game_natives.clone()];
        sources.push((Some(container.id()), container.root.clone(), includes));
    }
    let mut plugins = Vec::new();
    for (owner, root, includes) in sources {
        for plugin in discover(&root)? {
            plugins.push((owner.clone(), includes.clone(), plugin));
        }
    }
    if plugins.is_empty() {
        return Ok(Vec::new());
    }
    let output = project.output_dir();
    fs::create_dir_all(&output).with_context(|| format!("failed to create {}", output.display()))?;
    let header = project.root.join(HEADER_FILE);
    let mut built = Vec::new();
    let mut owners: HashMap<String, PathBuf> = HashMap::new();
    for (container, includes, plugin) in plugins {
        let (file, origin) = match &plugin {
            Plugin::C { name, sources } => (library_file(name), sources[0].clone()),
            Plugin::Rust { directory } => (library_file(&crate_library(directory)?), directory.clone()),
            Plugin::Prebuilt { path } => (
                path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_owned(),
                path.clone(),
            ),
        };
        if let Some(previous) = owners.insert(file.to_ascii_lowercase(), origin.clone()) {
            bail!(
                "{} and {} both produce a native library named {file}",
                previous.display(),
                origin.display()
            );
        }
        let path = output.join(&file);
        let rebuilt = match plugin {
            Plugin::C { name, sources } => {
                let mut inputs = sources.clone();
                inputs.push(header.clone());
                inputs.extend(includes.iter().map(|include| include.join("luv.h")));
                if fresh(&path, &inputs) {
                    false
                } else {
                    compile_c(&name, &sources, &includes, &path, &output.join(OBJECTS_DIR).join(&name))?;
                    true
                }
            }
            Plugin::Rust { directory } => {
                let artifact = compile_rust(&directory, &output.join(TARGET_DIR))?;
                copy_if_changed(&artifact, &path)?
            }
            Plugin::Prebuilt { path: source } => copy_if_changed(&source, &path)?,
        };
        built.push(Built {
            file,
            path,
            rebuilt,
            container,
        });
    }
    Ok(built)
}
