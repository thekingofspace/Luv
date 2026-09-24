use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use mlua::{AnyUserData, Function, Lua, Result, UserData, UserDataFields, UserDataMethods, UserDataRef, Value};

use super::{BaseGameObject, GameObject};
use crate::runtime::{Engine, compiler};
use crate::script;

const MODS: &str = "mods";

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

#[derive(Default)]
struct Loaded(RefCell<HashMap<String, Value>>);

impl Loaded {
    fn with<R>(lua: &Lua, action: impl FnOnce(&Loaded) -> R) -> R {
        if lua.app_data_ref::<Rc<Loaded>>().is_none() {
            lua.set_app_data(Rc::new(Loaded::default()));
        }
        let held = lua
            .app_data_ref::<Rc<Loaded>>()
            .unwrap_or_else(|| unreachable!("the module cache was just installed"));
        action(&held)
    }
}

fn base_dir(lua: &Lua) -> PathBuf {
    lua.app_data_ref::<Arc<Engine>>()
        .and_then(|engine| engine.game_dir().map(Path::to_path_buf))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

fn resolve(lua: &Lua, path: &str) -> Result<PathBuf> {
    let given = Path::new(path.trim());
    if path.trim().is_empty() {
        return Err(runtime("ECall needs the path of a Luau file"));
    }
    let full = if given.is_absolute() {
        given.to_path_buf()
    } else {
        base_dir(lua).join(given)
    };
    std::path::absolute(&full).map_err(|error| runtime(format!("cannot read {}: {error}", full.display())))
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "module".to_owned())
}

pub struct External {
    base: BaseGameObject,
    path: String,
    chunk: String,
    loader: RefCell<Option<Function>>,
}

impl External {
    pub const CLASS_NAME: &'static str = "ExternalModule";

    pub async fn call(lua: Lua, path: String) -> Result<AnyUserData> {
        let full = resolve(&lua, &path)?;
        let shown = full.to_string_lossy().replace('\\', "/");
        let reading = full.clone();
        let source = tokio::task::spawn_blocking(move || std::fs::read(&reading))
            .await
            .map_err(mlua::Error::external)?
            .map_err(|error| runtime(format!("cannot read {shown}: {error}")))?;
        let chunk = format!("@{MODS}/{}", stem(&full));
        let units = script::units(&source).map_err(|error| mlua::Error::SyntaxError {
            message: format!("{shown}:{error}"),
            incomplete_input: false,
        })?;
        let code = units
            .into_iter()
            .next()
            .ok_or_else(|| runtime(format!("{shown} has nothing to run")))?;
        let loader = lua
            .load(code)
            .set_name(&chunk)
            .set_compiler(compiler())
            .into_function()?;
        let mut base = BaseGameObject::new(Self::CLASS_NAME);
        base.set_name(stem(&full));
        lua.create_userdata(External {
            base,
            path: shown,
            chunk,
            loader: RefCell::new(Some(loader)),
        })
    }

    fn cached(&self, lua: &Lua) -> Option<Value> {
        Loaded::with(lua, |held| held.0.borrow().get(&self.path).cloned())
    }
}

impl GameObject for External {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        *self.loader.borrow_mut() = None;
    }
}

impl UserData for External {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("Path", |_, this| Ok(this.path.clone()));
        fields.add_field_method_get("Module", |_, this| Ok(this.chunk.clone()));
        fields.add_field_method_get("IsLoaded", |lua, this| Ok(this.cached(lua).is_some()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_async_method("Fetch", |lua, this: UserDataRef<Self>, ()| {
            let ready = this.base.ensure_alive().and_then(|()| {
                if let Some(value) = this.cached(&lua) {
                    return Ok(Err(value));
                }
                this.loader
                    .borrow()
                    .clone()
                    .map(Ok)
                    .ok_or_else(|| runtime(format!("{} was dropped and cannot run again", this.path)))
            });
            let path = this.path.clone();
            drop(this);
            async move {
                let value = match ready? {
                    Err(cached) => return Ok(cached),
                    Ok(loader) => loader.call_async::<Value>(()).await?,
                };
                Loaded::with(&lua, |held| held.0.borrow_mut().insert(path, value.clone()));
                Ok(value)
            }
        });
        methods.add_method("Drop", |lua, this, ()| {
            this.base.ensure_alive()?;
            Ok(Loaded::with(lua, |held| held.0.borrow_mut().remove(&this.path)).is_some())
        });
    }
}

pub fn install(lua: &Lua) -> Result<()> {
    let call = lua.create_async_function(|lua, path: String| External::call(lua, path))?;
    lua.globals().set("ECall", call)
}
