use std::cell::OnceCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{Lua, Result, Table, UserData, UserDataFields, UserDataMethods};
use wgpu::naga::Module;
use wgpu::naga::valid::ModuleInfo;

use super::{BaseGameObject, GameObject};
use crate::datatypes::UDim;
use crate::graphics::combine::Combined;
use crate::graphics::reflect::ShaderLayout;
use crate::graphics::shader::{CompiledShader, Language, stage_name};

static NEXT_SHADER: AtomicU64 = AtomicU64::new(1);

pub struct Shader {
    base: BaseGameObject,
    id: u64,
    language: Language,
    compiled: Option<CompiledShader>,
    error: Option<String>,
    layout: OnceCell<std::result::Result<Arc<ShaderLayout>, String>>,
}

impl Shader {
    pub const CLASS_NAME: &'static str = "Shader";

    pub fn new(name: impl Into<String>, language: Language, result: std::result::Result<CompiledShader, String>) -> Self {
        let mut base = BaseGameObject::new(Self::CLASS_NAME);
        base.set_name(name);
        let (compiled, error) = match result {
            Ok(compiled) => (Some(compiled), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            base,
            id: NEXT_SHADER.fetch_add(1, Ordering::Relaxed),
            language,
            compiled,
            error,
            layout: OnceCell::new(),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn layout(&self) -> Result<Arc<ShaderLayout>> {
        let compiled = self.compiled.as_ref().ok_or_else(|| {
            mlua::Error::runtime(match &self.error {
                Some(error) => format!("shader '{}' did not compile: {error}", self.base.name()),
                None => format!("shader '{}' has been destroyed", self.base.name()),
            })
        })?;
        self.layout
            .get_or_init(|| ShaderLayout::new(self.base.name(), compiled.module.clone(), &compiled.info).map(Arc::new))
            .clone()
            .map_err(mlua::Error::runtime)
    }

    pub fn language(&self) -> Language {
        self.language
    }

    pub fn is_compiled(&self) -> bool {
        self.compiled.is_some()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn module(&self) -> Option<Arc<Module>> {
        self.compiled.as_ref().map(|compiled| compiled.module.clone())
    }

    pub fn info(&self) -> Option<Arc<ModuleInfo>> {
        self.compiled.as_ref().map(|compiled| compiled.info.clone())
    }

    pub fn spirv(&self) -> Option<Arc<[u32]>> {
        self.compiled.as_ref().map(|compiled| compiled.spirv.clone())
    }

    fn entry_points(&self, lua: &Lua) -> Result<Table> {
        let entries = lua.create_table()?;
        for entry in self.compiled.iter().flat_map(|compiled| &compiled.entry_points) {
            let table = lua.create_table()?;
            table.set("Name", entry.name.as_str())?;
            table.set("Stage", stage_name(entry.stage))?;
            let [x, y, z] = entry.workgroup_size.map(f64::from);
            table.set("WorkgroupSize", UDim::new(x, y, z))?;
            entries.push(table)?;
        }
        Ok(entries)
    }

    fn spirv_bytes(&self) -> Result<Vec<u8>> {
        let spirv = self.spirv().ok_or_else(|| {
            mlua::Error::runtime(match &self.error {
                Some(error) => format!("shader '{}' did not compile: {error}", self.base.name()),
                None => format!("shader '{}' has been destroyed", self.base.name()),
            })
        })?;
        Ok(spirv.iter().flat_map(|word| word.to_le_bytes()).collect())
    }
}

pub struct ShaderCombo {
    base: BaseGameObject,
    combined: Option<Arc<Combined>>,
}

impl ShaderCombo {
    pub const CLASS_NAME: &'static str = "ShaderCombo";

    pub fn new(combined: Combined) -> Self {
        let mut base = BaseGameObject::new(Self::CLASS_NAME);
        base.set_name(combined.name.clone());
        Self {
            base,
            combined: Some(Arc::new(combined)),
        }
    }

    pub fn combined(&self) -> Result<Arc<Combined>> {
        self.base.ensure_alive()?;
        self.combined
            .clone()
            .ok_or_else(|| mlua::Error::runtime(format!("ShaderCombo '{}' has been destroyed", self.base.name())))
    }
}

impl GameObject for ShaderCombo {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.combined = None;
    }
}

impl UserData for ShaderCombo {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("Language", |_, this| Ok(this.combined()?.language.name()));
        fields.add_field_method_get("Parts", |lua, this| {
            lua.create_sequence_from(this.combined()?.parts.iter().map(|part| part.name.clone()))
        });
        fields.add_field_method_get("Source", |_, this| Ok(this.combined()?.code.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
    }
}

impl GameObject for Shader {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.compiled = None;
    }
}

impl UserData for Shader {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("Language", |_, this| Ok(this.language.name()));
        fields.add_field_method_get("Compiled", |_, this| Ok(this.is_compiled()));
        fields.add_field_method_get("Error", |_, this| Ok(this.error.clone()));
        fields.add_field_method_get("Size", |_, this| Ok(this.spirv().map_or(0, |spirv| spirv.len() * 4)));
        fields.add_field_method_get("EntryPoints", |lua, this| this.entry_points(lua));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_method("GetSpirv", |lua, this, ()| lua.create_buffer(this.spirv_bytes()?));
    }
}
