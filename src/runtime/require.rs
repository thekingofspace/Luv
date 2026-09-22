use std::io;
use std::sync::Arc;

use mlua::luau::{NavigateError, Require};
use mlua::{Function, Lua, Result};

use super::containers::Containers;
use super::{aliases, load_unit};
use crate::vfs::{self, Vfs};

const EXTENSIONS: [&str; 2] = ["luau", "lua"];
pub(crate) const CONFIG_FILES: [&str; 2] = [".luaurc", ".config.luau"];

pub struct VfsRequirer {
    vfs: Arc<dyn Vfs>,
    containers: Option<Arc<Containers>>,
    module: Vec<String>,
    resolved: Option<String>,
}

impl VfsRequirer {
    pub fn new(vfs: Arc<dyn Vfs>) -> Self {
        Self {
            vfs,
            containers: None,
            module: Vec::new(),
            resolved: None,
        }
    }

    pub fn with_containers(mut self, containers: Arc<Containers>) -> Self {
        self.containers = Some(containers);
        self
    }

    fn split(path: &str) -> Vec<String> {
        path.split('/').filter(|part| !part.is_empty()).map(str::to_owned).collect()
    }

    fn resolve(&self, module: &[String]) -> std::result::Result<Option<String>, NavigateError> {
        let path = module.join("/");
        let mut found = None;

        let explicit = module.last().is_some_and(|name| {
            EXTENSIONS
                .iter()
                .any(|ext| name.strip_suffix(ext).is_some_and(|stem| stem.len() > 1 && stem.ends_with('.')))
        });
        if explicit && self.vfs.is_file(&path) {
            return Ok(Some(path));
        }

        if module.last().is_some_and(|name| name != "init") {
            for ext in EXTENSIONS {
                let candidate = format!("{path}.{ext}");
                if self.vfs.is_file(&candidate) && found.replace(candidate).is_some() {
                    return Err(NavigateError::Ambiguous);
                }
            }
        }

        if self.vfs.is_dir(&path) {
            for ext in EXTENSIONS {
                let candidate = vfs::join(&path, &format!("init.{ext}"));
                if self.vfs.is_file(&candidate) && found.replace(candidate).is_some() {
                    return Err(NavigateError::Ambiguous);
                }
            }
            if found.is_none() {
                return Ok(None);
            }
        }

        found.map(Some).ok_or(NavigateError::NotFound)
    }

    fn navigate(&mut self, module: Vec<String>) -> std::result::Result<(), NavigateError> {
        self.resolved = self.resolve(&module)?;
        self.module = module;
        Ok(())
    }

    fn script_module(&self, path: &str) -> Option<Vec<String>> {
        if !self.vfs.is_file(path) {
            return None;
        }
        module_of(path).map(|module| Self::split(&module))
    }
}

pub fn module_of(script: &str) -> Option<String> {
    let (stem, ext) = script.rsplit_once('.')?;
    if !EXTENSIONS.contains(&ext) {
        return None;
    }
    Some(match stem.rsplit_once('/') {
        Some((directory, "init")) => directory.to_owned(),
        None if stem == "init" => String::new(),
        _ => stem.to_owned(),
    })
}

impl Require for VfsRequirer {
    fn is_require_allowed(&self, chunk_name: &str) -> bool {
        chunk_name.starts_with('@')
    }

    fn reset(&mut self, chunk_name: &str) -> std::result::Result<(), NavigateError> {
        let name = chunk_name.strip_prefix('@').ok_or(NavigateError::NotFound)?;
        let name = match name.rsplit_once(':') {
            Some((path, line)) if line.parse::<u32>().is_ok() => path,
            _ => name,
        };
        let path = vfs::normalize(name).ok_or(NavigateError::NotFound)?;

        if let Some(module) = self.script_module(&path) {
            self.module = module;
            self.resolved = Some(path);
            return Ok(());
        }

        self.module = Self::split(&path);
        self.resolved = self.resolve(&self.module).ok().flatten();
        Ok(())
    }

    fn to_alias_override(&mut self, alias: &str) -> std::result::Result<(), NavigateError> {
        let entry = self
            .containers
            .as_ref()
            .and_then(|containers| containers.entry(alias))
            .ok_or(NavigateError::NotFound)?;
        let module = module_of(&entry).ok_or(NavigateError::NotFound)?;
        self.navigate(Self::split(&module))
    }

    fn jump_to_alias(&mut self, path: &str) -> std::result::Result<(), NavigateError> {
        let path = vfs::normalize(path).ok_or(NavigateError::NotFound)?;
        self.navigate(Self::split(&path))
    }

    fn to_parent(&mut self) -> std::result::Result<(), NavigateError> {
        let mut module = self.module.clone();
        module.pop().ok_or(NavigateError::NotFound)?;
        self.navigate(module)
    }

    fn to_child(&mut self, name: &str) -> std::result::Result<(), NavigateError> {
        let mut module = self.module.clone();
        module.push(name.to_owned());
        self.navigate(module)
    }

    fn has_module(&self) -> bool {
        self.resolved.as_deref().is_some_and(|path| self.vfs.is_file(path))
    }

    fn cache_key(&self) -> String {
        self.resolved.clone().unwrap_or_default()
    }

    fn has_config(&self) -> bool {
        let dir = self.module.join("/");
        self.vfs.is_dir(&dir) && CONFIG_FILES.iter().any(|file| self.vfs.is_file(&vfs::join(&dir, file)))
    }

    fn config(&self) -> io::Result<Vec<u8>> {
        let dir = self.module.join("/");
        let path = CONFIG_FILES
            .iter()
            .map(|file| vfs::join(&dir, file))
            .find(|path| self.vfs.is_file(path))
            .ok_or_else(|| vfs::not_found(&dir))?;
        let source = self.vfs.read(&path)?;
        if !path.ends_with(aliases::LUAURC) {
            return Ok(source);
        }
        aliases::canonical_luaurc(&String::from_utf8_lossy(&source))
            .map(String::into_bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, format!("{path}: {error}")))
    }

    fn loader(&self, lua: &Lua) -> Result<Function> {
        let path = self.resolved.as_deref().ok_or_else(|| mlua::Error::runtime("no module is selected"))?;
        load_unit(lua, self.vfs.as_ref(), path, 0)
    }
}
