use std::fs;
use std::path::{self, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::plugins::NATIVE_DIR;
use crate::runtime::CONFIG_FILES;
use crate::vfs::{self, DirVfs};

pub const MANIFEST_FILE: &str = "build.toml";
pub const PACKAGE_EXTENSION: &str = "luvit";
pub const CONTAINER_FILE: &str = "container.toml";
pub const CONTAINER_EXTENSION: &str = "cont";
pub const ASSETS_DIR: &str = "assets";

const MANIFEST_TEMPLATE: &str = include_str!("../templates/build.toml");
const MAIN_TEMPLATE: &str = include_str!("../templates/main.luau");
pub const TYPES_TEMPLATE: &str = include_str!("../templates/types.d.luau");
const SETTINGS_TEMPLATE: &str = include_str!("../templates/settings.json");
const GITIGNORE_TEMPLATE: &str = include_str!("../templates/gitignore");
const HEADER_TEMPLATE: &str = include_str!("../templates/luv.h");
const BINDINGS_TEMPLATE: &str = include_str!("../templates/luv.rs");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildManifest {
    pub game: GameInfo,
    #[serde(default)]
    pub build: BuildSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameInfo {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "default_main")]
    pub main: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContainerManifest {
    pub container: ContainerInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub main: String,
    #[serde(default)]
    pub natives: Vec<String>,
}

impl ContainerInfo {
    pub fn from_manifest(manifest: &str) -> Result<Self> {
        toml::from_str(manifest).context("container manifest is invalid")
    }

    pub fn to_manifest(&self) -> Result<String> {
        Ok(toml::to_string(self)?)
    }

    pub fn id(&self) -> String {
        file_stem(&self.name)
    }
}

#[derive(Clone, Debug)]
pub struct ContainerProject {
    pub root: PathBuf,
    pub folder: String,
    pub info: ContainerInfo,
}

impl ContainerProject {
    pub fn load(root: &Path, folder: String) -> Result<Self> {
        let path = root.join(CONTAINER_FILE);
        let text = fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
        let manifest: ContainerManifest =
            toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Self {
            root: root.to_path_buf(),
            folder,
            info: manifest.container,
        })
    }

    pub fn id(&self) -> String {
        self.info.id()
    }

    pub fn entry(&self) -> Result<String> {
        let main = &self.info.main;
        let entry = vfs::normalize(main)
            .filter(|entry| !entry.is_empty())
            .with_context(|| format!("the main script `{main}` of container {} must be a path inside it", self.info.name))?;
        if !is_script(&entry) {
            bail!("the main script `{main}` of container {} must be a .luau or .lua file", self.info.name);
        }
        Ok(entry)
    }

    pub fn package_path(&self, output: &Path) -> PathBuf {
        output.join(format!("{}.{CONTAINER_EXTENSION}", self.id()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildSettings {
    #[serde(default = "default_output")]
    pub output: String,
    #[serde(default = "default_aliases")]
    pub aliases: bool,
}

impl GameInfo {
    pub fn from_manifest(manifest: &str) -> Result<Self> {
        toml::from_str(manifest).context("package manifest is invalid")
    }

    pub fn to_manifest(&self) -> Result<String> {
        Ok(toml::to_string(self)?)
    }
}

impl Default for BuildSettings {
    fn default() -> Self {
        Self {
            output: default_output(),
            aliases: default_aliases(),
        }
    }
}

fn default_version() -> String {
    "0.1.0".to_owned()
}

fn default_main() -> String {
    "src/main.luau".to_owned()
}

fn default_output() -> String {
    "build".to_owned()
}

fn default_aliases() -> bool {
    true
}

#[derive(Clone)]
pub struct Project {
    pub root: PathBuf,
    pub manifest: BuildManifest,
}

impl Project {
    pub fn discover(start: &Path) -> Result<Self> {
        let start = path::absolute(start)?;
        let root = start
            .ancestors()
            .find(|dir| dir.join(MANIFEST_FILE).is_file())
            .with_context(|| {
                format!(
                    "could not find {MANIFEST_FILE} in {} or any parent directory",
                    start.display()
                )
            })?;
        Self::load(root)
    }

    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(MANIFEST_FILE);
        let text = fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
        let manifest = toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
        })
    }

    pub fn entry(&self) -> Result<String> {
        let main = &self.manifest.game.main;
        let entry = vfs::normalize(main)
            .filter(|entry| !entry.is_empty())
            .with_context(|| format!("main script `{main}` must be a path inside the workspace"))?;
        if !is_script(&entry) {
            bail!("main script `{main}` must be a .luau or .lua file");
        }
        Ok(entry)
    }

    pub fn output_dir(&self) -> PathBuf {
        self.root.join(&self.manifest.build.output)
    }

    pub fn output_in_workspace(&self) -> Option<String> {
        let relative = self.output_dir().strip_prefix(&self.root).ok()?.to_string_lossy().into_owned();
        vfs::normalize(&relative).filter(|relative| !relative.is_empty())
    }

    pub fn source_vfs(&self) -> DirVfs {
        let output = self.output_in_workspace();
        let containers = self.container_folders();
        DirVfs::filtered(&self.root, move |path| {
            is_packaged(path, output.as_deref()) && !inside_any(path, &containers)
        })
    }

    pub fn container_folders(&self) -> Vec<String> {
        let output = self.output_in_workspace();
        let mut found = Vec::new();
        let mut pending = vec![(String::new(), self.root.clone())];
        while let Some((prefix, dir)) = pending.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|name| name.to_str()).map(str::to_owned) else {
                    continue;
                };
                let relative = vfs::join(&prefix, &name);
                if name.starts_with('.') || output.as_deref() == Some(relative.as_str()) || relative == NATIVE_DIR {
                    continue;
                }
                if path.join(CONTAINER_FILE).is_file() {
                    found.push(relative);
                } else {
                    pending.push((relative, path));
                }
            }
        }
        found.sort();
        found
    }

    pub fn containers(&self) -> Result<Vec<ContainerProject>> {
        let mut containers: Vec<ContainerProject> = Vec::new();
        for folder in self.container_folders() {
            let container = ContainerProject::load(&self.root.join(&folder), folder)?;
            if let Some(other) = containers.iter().find(|other| other.id().eq_ignore_ascii_case(&container.id())) {
                bail!(
                    "the containers in {} and {} are both named {}, container names must be unique",
                    other.folder,
                    container.folder,
                    container.info.name
                );
            }
            containers.push(container);
        }
        Ok(containers)
    }

    pub fn package_path(&self) -> PathBuf {
        self.output_dir()
            .join(format!("{}.{PACKAGE_EXTENSION}", file_stem(&self.manifest.game.name)))
    }
}

pub fn is_packable(relative: &str, name: &str) -> bool {
    if name.starts_with('.') {
        return CONFIG_FILES.contains(&name);
    }
    relative != MANIFEST_FILE
        && relative != CONTAINER_FILE
        && relative != NATIVE_DIR
        && !name.ends_with(".d.luau")
        && !is_native_library(name)
}

pub fn inside_any(path: &str, folders: &[String]) -> bool {
    folders.iter().any(|folder| {
        path.strip_prefix(folder.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

pub fn is_packaged(path: &str, output: Option<&str>) -> bool {
    let mut prefix = String::new();
    for name in path.split('/') {
        prefix = vfs::join(&prefix, name);
        if !is_packable(&prefix, name) || output == Some(prefix.as_str()) {
            return false;
        }
    }
    true
}

pub fn is_native_library(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.ends_with(".dll") || name.ends_with(".so") || name.ends_with(".dylib") || name.contains(".so.")
}

pub fn is_script(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    !name.starts_with('.') && !name.ends_with(".d.luau") && (name.ends_with(".luau") || name.ends_with(".lua"))
}

pub fn file_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let stem = stem.trim_matches('-');
    if stem.is_empty() { "game".to_owned() } else { stem.to_owned() }
}

pub const TYPES_FILE: &str = "types.d.luau";
pub const HEADER_FILE: &str = "native/luv.h";
pub const BINDINGS_FILE: &str = "native/luv.rs";
pub const SETTINGS_FILE: &str = ".vscode/settings.json";

pub struct InitReport {
    pub root: PathBuf,
    pub existing: bool,
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Policy {
    Create,
    Sync,
}

pub fn init(dir: &Path, name: Option<String>) -> Result<InitReport> {
    let root = path::absolute(dir)?;
    let existing = root.join(MANIFEST_FILE).is_file();

    let name = name
        .or_else(|| root.file_name().map(|name| name.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "Game".to_owned());
    let manifest = MANIFEST_TEMPLATE.replace("\"{{name}}\"", &toml::Value::String(name).to_string());
    let parsed: BuildManifest = toml::from_str(&manifest).context("the build.toml template is invalid")?;

    let mut roots = vec![root.clone()];
    if existing && let Ok(project) = Project::load(&root) {
        roots = crate::typegen::roots(&project);
    }
    let (types, _) = crate::typegen::build(&root, &roots)?;

    let mut files = vec![
        (TYPES_FILE, types.as_str(), Policy::Sync),
        (HEADER_FILE, HEADER_TEMPLATE, Policy::Sync),
        (BINDINGS_FILE, BINDINGS_TEMPLATE, Policy::Sync),
        (SETTINGS_FILE, SETTINGS_TEMPLATE, Policy::Create),
    ];
    if !existing {
        files.extend([
            (MANIFEST_FILE, manifest.as_str(), Policy::Create),
            (parsed.game.main.as_str(), MAIN_TEMPLATE, Policy::Create),
            (".gitignore", GITIGNORE_TEMPLATE, Policy::Create),
        ]);
        fs::create_dir_all(root.join(ASSETS_DIR)).with_context(|| format!("failed to create {}", root.display()))?;
    }

    let mut report = InitReport {
        root,
        existing,
        created: Vec::new(),
        updated: Vec::new(),
        skipped: Vec::new(),
    };
    for (path, contents, policy) in files {
        let target = report.root.join(path);
        if target.exists() {
            let current = fs::read(&target).with_context(|| format!("failed to read {}", target.display()))?;
            if policy == Policy::Create || current == contents.as_bytes() {
                report.skipped.push(path.to_owned());
                continue;
            }
            fs::write(&target, contents).with_context(|| format!("failed to write {}", target.display()))?;
            report.updated.push(path.to_owned());
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, contents).with_context(|| format!("failed to write {}", target.display()))?;
        report.created.push(path.to_owned());
    }
    Ok(report)
}
