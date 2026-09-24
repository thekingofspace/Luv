use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use mlua::{AnyUserData, Lua, Result, UserData, UserDataFields, UserDataMethods, UserDataRef, Value};

use super::{BaseGameObject, GameObject};
use crate::project::is_script;
use crate::runtime::{Engine, load_unit};
use crate::script;
use crate::vfs::{self, MemoryVfs, Vfs};

const MODS: &str = "mods";
const ENTRY: &str = "init.luau";
const MAX_FILES: usize = 4096;
const MAX_BYTES: u64 = 512 * 1024 * 1024;

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

#[derive(Default)]
struct Mounted {
    values: HashMap<String, Value>,
    folders: HashMap<String, String>,
    taken: Vec<String>,
}

fn with_mounted<R>(lua: &Lua, action: impl FnOnce(&mut Mounted) -> R) -> R {
    if lua.app_data_ref::<Rc<RefCell<Mounted>>>().is_none() {
        lua.set_app_data(Rc::new(RefCell::new(Mounted::default())));
    }
    let held = lua
        .app_data_ref::<Rc<RefCell<Mounted>>>()
        .map(|held| held.clone())
        .unwrap_or_else(|| unreachable!("the module table was just installed"));
    let mut borrowed = held.borrow_mut();
    action(&mut borrowed)
}

fn engine_of(lua: &Lua) -> Result<Arc<Engine>> {
    lua.app_data_ref::<Arc<Engine>>()
        .map(|engine| engine.clone())
        .ok_or_else(|| runtime("the luv engine is not running"))
}

fn base_dir(lua: &Lua) -> PathBuf {
    lua.app_data_ref::<Arc<Engine>>()
        .and_then(|engine| engine.game_dir().map(Path::to_path_buf))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

fn resolve(lua: &Lua, path: &str) -> Result<PathBuf> {
    if path.trim().is_empty() {
        return Err(runtime("ecall needs the path of a folder or a Luau file"));
    }
    let given = Path::new(path.trim());
    let full = if given.is_absolute() {
        given.to_path_buf()
    } else {
        base_dir(lua).join(given)
    };
    std::path::absolute(&full).map_err(|error| runtime(format!("cannot read {}: {error}", full.display())))
}

fn label(path: &Path) -> String {
    let name: String = path
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches('-').to_ascii_lowercase();
    if name.is_empty() { "mod".to_owned() } else { name }
}

fn gather(root: &Path, prefix: &str, into: &mut MemoryVfs, total: &mut u64) -> std::io::Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let inner = vfs::join(prefix, name);
        if path.is_dir() {
            gather(&path, &inner, into, total)?;
            continue;
        }
        if into.len() >= MAX_FILES {
            return Err(std::io::Error::other(format!("a mod can hold at most {MAX_FILES} files")));
        }
        let data = std::fs::read(&path)?;
        *total += data.len() as u64;
        if *total > MAX_BYTES {
            return Err(std::io::Error::other("that folder is too large to load"));
        }
        if is_script(&inner) {
            let code =
                script::compile(&data).map_err(|error| std::io::Error::other(format!("{}: {error}", path.display())))?;
            into.insert(inner, code, true);
        } else {
            into.insert(inner, data, false);
        }
    }
    Ok(())
}

struct Built {
    layer: MemoryVfs,
    entry: String,
    files: usize,
}

fn build(full: &Path, mount: &str) -> Result<Built> {
    if !full.exists() {
        return Err(runtime(format!("cannot read {}: there is no folder or file there", full.display())));
    }
    let mut layer = MemoryVfs::new();
    let mut total = 0;
    let entry = if full.is_dir() {
        gather(full, mount, &mut layer, &mut total).map_err(|error| runtime(error.to_string()))?;
        let entry = vfs::join(mount, ENTRY);
        if !layer.is_file(&entry) {
            return Err(runtime(format!("{} has no {ENTRY}", full.display())));
        }
        entry
    } else {
        let name = full
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| runtime("that file has no usable name"))?;
        let inner = vfs::join(mount, name);
        if !is_script(&inner) {
            return Err(runtime(format!("{} is not a Luau file", full.display())));
        }
        let data =
            std::fs::read(full).map_err(|error| runtime(format!("cannot read {}: {error}", full.display())))?;
        let code = script::compile(&data).map_err(|error| runtime(format!("{}: {error}", full.display())))?;
        layer.insert(inner.clone(), code, true);
        inner
    };
    let files = layer.len();
    Ok(Built { layer, entry, files })
}

fn list(vfs: &dyn Vfs, folder: &str, into: &mut Vec<String>) {
    let Ok(names) = vfs.read_dir(folder) else {
        return;
    };
    for name in names {
        let path = vfs::join(folder, &name);
        if vfs.is_file(&path) {
            into.push(path);
        } else {
            list(vfs, &path, into);
        }
    }
}

pub struct External {
    base: BaseGameObject,
    source: String,
    folder: String,
    entry: String,
    files: usize,
}

impl External {
    pub const CLASS_NAME: &'static str = "ExternalModule";

    pub async fn call(lua: Lua, path: String) -> Result<AnyUserData> {
        let full = resolve(&lua, &path)?;
        let source = full.to_string_lossy().replace('\\', "/");
        let engine = engine_of(&lua)?;

        let known = with_mounted(&lua, |mounted| mounted.folders.get(&source).cloned());
        let (folder, entry, files) = match known {
            Some(folder) => {
                let named = vfs::join(&folder, ENTRY);
                let entry = if engine.vfs().is_file(&named) {
                    named
                } else {
                    vfs::join(&folder, full.file_name().and_then(|name| name.to_str()).unwrap_or(ENTRY))
                };
                let mut found = Vec::new();
                list(engine.vfs().as_ref(), &folder, &mut found);
                (folder, entry, found.len())
            }
            None => {
                let folder = with_mounted(&lua, |mounted| {
                    let wanted = vfs::join(MODS, &label(&full));
                    let mut chosen = wanted.clone();
                    let mut extra = 2;
                    while mounted.taken.contains(&chosen) {
                        chosen = format!("{wanted}-{extra}");
                        extra += 1;
                    }
                    mounted.taken.push(chosen.clone());
                    chosen
                });
                let reading = full.clone();
                let mount = folder.clone();
                let built = tokio::task::spawn_blocking(move || build(&reading, &mount))
                    .await
                    .map_err(mlua::Error::external)??;
                engine.mount(Arc::new(built.layer));
                with_mounted(&lua, |mounted| mounted.folders.insert(source.clone(), folder.clone()));
                (folder, built.entry, built.files)
            }
        };

        let mut base = BaseGameObject::new(Self::CLASS_NAME);
        base.set_name(folder.rsplit('/').next().unwrap_or(&folder));
        lua.create_userdata(External {
            base,
            source,
            folder,
            entry,
            files,
        })
    }

    fn cached(&self, lua: &Lua) -> Option<Value> {
        with_mounted(lua, |mounted| mounted.values.get(&self.entry).cloned())
    }
}

impl GameObject for External {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }
}

impl UserData for External {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("Source", |_, this| Ok(this.source.clone()));
        fields.add_field_method_get("Folder", |_, this| Ok(this.folder.clone()));
        fields.add_field_method_get("Entry", |_, this| Ok(this.entry.clone()));
        fields.add_field_method_get("Files", |_, this| Ok(this.files));
        fields.add_field_method_get("IsLoaded", |lua, this| Ok(this.cached(lua).is_some()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_method("GetFiles", |lua, this, ()| {
            this.base.ensure_alive()?;
            let engine = engine_of(lua)?;
            let mut found = Vec::new();
            list(engine.vfs().as_ref(), &this.folder, &mut found);
            lua.create_sequence_from(found)
        });
        methods.add_async_method("Fetch", |lua, this: UserDataRef<Self>, ()| {
            let ready = this.base.ensure_alive().map(|()| this.cached(&lua));
            let entry = this.entry.clone();
            let folder = this.folder.clone();
            drop(this);
            async move {
                if let Some(value) = ready? {
                    return Ok(value);
                }
                let engine = engine_of(&lua)?;
                let loader = load_unit(&lua, engine.vfs().as_ref(), &entry, 0)?;
                let value = loader.call_async::<Value>(folder).await?;
                with_mounted(&lua, |mounted| mounted.values.insert(entry, value.clone()));
                Ok(value)
            }
        });
        methods.add_method("Drop", |lua, this, ()| {
            this.base.ensure_alive()?;
            Ok(with_mounted(lua, |mounted| mounted.values.remove(&this.entry)).is_some())
        });
    }
}

const RESERVED: [&str; 8] = ["ecall", "import", "require", "enum", "udim", "color", "SetGlobal", "_G"];

fn set_global(lua: &Lua, name: &str, value: Value) -> Result<()> {
    let clean = name.trim();
    if clean.is_empty() {
        return Err(runtime("SetGlobal needs a name"));
    }
    let mut letters = clean.chars();
    let valid = letters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && letters.all(|letter| letter.is_ascii_alphanumeric() || letter == '_');
    if !valid {
        return Err(runtime(format!(
            "'{clean}' is not a valid global name, use letters, digits and underscores"
        )));
    }
    if RESERVED.contains(&clean) {
        return Err(runtime(format!("'{clean}' belongs to luv and cannot be replaced")));
    }
    lua.globals().set(clean, value)
}

pub fn install(lua: &Lua) -> Result<()> {
    let call = lua.create_async_function(|lua, path: String| External::call(lua, path))?;
    lua.globals().set("ecall", call)?;
    let setter = lua.create_function(|lua, (name, value): (String, Value)| set_global(lua, &name, value))?;
    lua.globals().set("SetGlobal", setter)
}
