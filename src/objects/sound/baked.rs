use std::sync::Arc;

use mlua::{Result, UserData, UserDataFields, UserDataMethods};

use crate::audio::Pcm;
use crate::objects::{BaseGameObject, GameObject};

pub struct BakedSound {
    base: BaseGameObject,
    pcm: Option<Arc<Pcm>>,
    source: Option<String>,
}

impl BakedSound {
    pub const CLASS_NAME: &'static str = "BakedSound";

    pub fn new(pcm: Arc<Pcm>, source: Option<String>) -> Self {
        let mut base = BaseGameObject::new(Self::CLASS_NAME);
        if let Some(source) = &source {
            base.set_name(source.rsplit('/').next().unwrap_or(source));
        }
        Self {
            base,
            pcm: Some(pcm),
            source,
        }
    }

    pub fn pcm(&self) -> Result<Arc<Pcm>> {
        self.base.ensure_alive()?;
        self.pcm
            .clone()
            .ok_or_else(|| mlua::Error::runtime("this BakedSound has been destroyed"))
    }

    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    fn read<T>(&self, read: impl FnOnce(&Pcm) -> T) -> Result<T> {
        let pcm = self.pcm()?;
        Ok(read(&pcm))
    }
}

impl GameObject for BakedSound {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.pcm = None;
    }
}

impl UserData for BakedSound {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("Source", |_, this| Ok(this.source.clone()));
        fields.add_field_method_get("Duration", |_, this| this.read(Pcm::seconds));
        fields.add_field_method_get("SampleRate", |_, this| this.read(Pcm::rate));
        fields.add_field_method_get("Channels", |_, this| this.read(Pcm::channels));
        fields.add_field_method_get("Frames", |_, this| this.read(Pcm::frames));
        fields.add_field_method_get("Peak", |_, this| this.read(|pcm| f64::from(pcm.peak())));
        fields.add_field_method_get("Memory", |_, this| this.read(Pcm::memory));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_method("GetBytes", |lua, this, ()| lua.create_buffer(this.pcm()?.wav()));
    }
}
