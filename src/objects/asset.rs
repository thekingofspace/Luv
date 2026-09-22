use std::io::Cursor;
use std::sync::Arc;

use mlua::{Result, UserData, UserDataFields, UserDataMethods};

use super::file::{File, Handle};
use super::{BaseGameObject, GameObject};

pub struct Asset {
    base: BaseGameObject,
    path: String,
    data: Option<Arc<[u8]>>,
}

impl Asset {
    pub const CLASS_NAME: &'static str = "Asset";

    pub fn new(path: impl Into<String>, data: impl Into<Arc<[u8]>>) -> Self {
        let path = path.into();
        let mut base = BaseGameObject::new(Self::CLASS_NAME);
        base.set_name(path.rsplit('/').next().unwrap_or(&path));
        Self {
            base,
            path,
            data: Some(data.into()),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn extension(&self) -> Option<&str> {
        let name = self.path.rsplit('/').next().unwrap_or(&self.path);
        name.rsplit_once('.').map(|(_, extension)| extension)
    }

    pub fn data(&self) -> Result<Arc<[u8]>> {
        self.base.ensure_alive()?;
        self.data
            .clone()
            .ok_or_else(|| mlua::Error::runtime(format!("Asset '{}' has been destroyed", self.path)))
    }
}

impl GameObject for Asset {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.data = None;
    }
}

impl UserData for Asset {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("Path", |_, this| Ok(this.path.clone()));
        fields.add_field_method_get("Size", |_, this| Ok(this.data.as_ref().map_or(0, |data| data.len())));
        fields.add_field_method_get("Extension", |_, this| Ok(this.extension().map(str::to_owned)));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_method("ReadString", |lua, this, ()| lua.create_string(&*this.data()?));
        methods.add_method("ReadBuffer", |lua, this, ()| lua.create_buffer(&*this.data()?));
        methods.add_method("Open", |lua, this, ()| {
            let file = File::new(this.path.clone(), Handle::Memory(Cursor::new(this.data()?)), true);
            lua.create_userdata(file)
        });
    }
}
