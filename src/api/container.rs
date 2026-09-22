use std::sync::Arc;

use mlua::{Lua, Result, Table, UserData, UserDataFields, UserDataMethods};

use crate::objects::{BaseGameObject, GameObject};
use crate::runtime::{Engine, LoadedContainer};

pub struct ContainerLibrary {
    base: BaseGameObject,
    container: Arc<LoadedContainer>,
}

impl ContainerLibrary {
    pub const CLASS_NAME: &'static str = "ContainerLibrary";

    fn new(container: Arc<LoadedContainer>) -> Self {
        let mut base = BaseGameObject::new(Self::CLASS_NAME);
        base.set_name(container.info.name.clone());
        Self { base, container }
    }
}

impl GameObject for ContainerLibrary {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }
}

impl UserData for ContainerLibrary {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("Id", |_, this| Ok(this.container.id.clone()));
        fields.add_field_method_get("Version", |_, this| Ok(this.container.info.version.clone()));
        fields.add_field_method_get("Description", |_, this| Ok(this.container.info.description.clone()));
        fields.add_field_method_get("Main", |_, this| Ok(this.container.info.main.clone()));
        fields.add_field_method_get("Path", |_, this| Ok(this.container.path.display().to_string()));
        fields.add_field_method_get("Natives", |lua, this| lua.create_sequence_from(this.container.info.natives.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_method("GetRequire", |_, this, ()| Ok(format!("@{}", this.container.id)));
    }
}

pub fn create(lua: &Lua, engine: &Arc<Engine>) -> Result<Table> {
    let container = lua.create_table()?;
    let containers = engine.containers().clone();

    container.set("Exists", {
        let containers = containers.clone();
        lua.create_function(move |_, name: String| Ok(containers.exists(&name)))?
    })?;
    container.set("IsLoaded", {
        let containers = containers.clone();
        lua.create_function(move |_, name: String| Ok(containers.loaded(&name).is_some()))?
    })?;
    container.set("GetContainers", {
        let containers = containers.clone();
        lua.create_function(move |lua, ()| lua.create_sequence_from(containers.names()))?
    })?;
    container.set("GetLibrary", {
        let containers = containers.clone();
        lua.create_function(move |_, name: String| Ok(containers.loaded(&name).map(ContainerLibrary::new)))?
    })?;
    container.set("Refresh", {
        let containers = containers.clone();
        lua.create_async_function(move |lua, ()| {
            let containers = containers.clone();
            async move {
                let names = tokio::task::spawn_blocking(move || containers.refresh())
                    .await
                    .map_err(mlua::Error::external)?;
                lua.create_sequence_from(names)
            }
        })?
    })?;
    container.set("LoadLibrary", {
        let containers = containers.clone();
        lua.create_async_function(move |_, name: String| {
            let containers = containers.clone();
            async move {
                let loaded = tokio::task::spawn_blocking(move || containers.load(&name))
                    .await
                    .map_err(mlua::Error::external)?
                    .map_err(mlua::Error::runtime)?;
                Ok(ContainerLibrary::new(loaded))
            }
        })?
    })?;

    container.set_readonly(true);
    Ok(container)
}
