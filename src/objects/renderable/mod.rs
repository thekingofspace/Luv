mod data;
mod object;
mod scene;

pub use object::Kind;
pub use scene::Scene;

use std::collections::{BTreeSet, HashSet};
use std::rc::{Rc, Weak};

use mlua::{AnyUserData, Lua, ObjectLike, Result, Table, UserData, UserDataFields, UserDataMethods, Value};

use self::data::{Bytes, Reader, Writer};
use self::object::{HookSlot, ImageSource, Loaded, Object, Slot, TextState};
use self::scene::Resources;
use super::{Asset, BaseGameObject, GameObject, Shader};
use crate::datatypes::enums::{BLEND_MODE, RESAMPLE_MODE, SHAPE_TYPE, TEXT_X_ALIGNMENT, TEXT_Y_ALIGNMENT};
use crate::datatypes::{Color, EnumItem, UDim};
use crate::graphics::geometry::ShapeKind;
use crate::graphics::picture;
use crate::graphics::protocol::{Blend, ObjectId};
use crate::graphics::reflect::BindingKind;
use crate::graphics::text::Align;
use crate::native::Pointer;

const ALL: &[Kind] = &[Kind::Renderable, Kind::Shape, Kind::Image, Kind::Text];
const SHADED: &[Kind] = &[Kind::Renderable, Kind::Shape, Kind::Image, Kind::Text, Kind::Post];
const POST: &[Kind] = &[Kind::Post];
const BASE: &[Kind] = &[Kind::Renderable];
const PLACED: &[Kind] = &[Kind::Shape, Kind::Image, Kind::Text];
const SHAPE: &[Kind] = &[Kind::Shape];
const STROKED: &[Kind] = &[Kind::Shape, Kind::Text];
const IMAGE: &[Kind] = &[Kind::Image];
const TEXT: &[Kind] = &[Kind::Text];
const PRIORITY: [&str; 3] = ["Image", "Font", "Shape"];
const DEFAULT_TEXT_SIZE: f64 = 16.0;

pub struct Renderable {
    base: BaseGameObject,
    id: ObjectId,
    kind: Kind,
    scene: Weak<Scene>,
}

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

fn finite(property: &str, value: f64) -> Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(runtime(format!("{property} must be a finite number")))
    }
}

fn at_least_zero(property: &str, value: f64) -> Result<f64> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(runtime(format!("{property} must be a number of at least 0")))
    }
}

fn count(property: &str, value: f64) -> Result<u32> {
    if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= f64::from(u32::MAX) {
        Ok(value as u32)
    } else {
        Err(runtime(format!("{property} must be a whole number of at least 0")))
    }
}

fn finite_udim(property: &str, value: UDim) -> Result<UDim> {
    if value.x.is_finite() && value.y.is_finite() && value.z.is_finite() {
        Ok(value)
    } else {
        Err(runtime(format!("{property} must only hold finite numbers")))
    }
}

fn align(item: EnumItem, enum_type: &str) -> Result<Align> {
    let item = item.of(enum_type)?;
    Ok(match item.name {
        "Left" | "Top" => Align::Start,
        "Right" | "Bottom" => Align::End,
        _ => Align::Center,
    })
}

fn align_name(align: Align, enum_type: &str) -> &'static str {
    match (align, enum_type == TEXT_X_ALIGNMENT) {
        (Align::Start, true) => "Left",
        (Align::End, true) => "Right",
        (Align::Start, false) => "Top",
        (Align::End, false) => "Bottom",
        (Align::Center, _) => "Center",
    }
}

fn enum_value(lua: &Lua, enum_type: &str, name: &str) -> Result<Value> {
    EnumItem::find(enum_type, name).map_or(Ok(Value::Nil), |item| item.canonical(lua))
}

fn asset_data(value: &AnyUserData, property: &str) -> Result<(std::sync::Arc<[u8]>, String)> {
    let asset = value
        .borrow::<Asset>()
        .map_err(|_| runtime(format!("{property} must be an Asset")))?;
    Ok((asset.data()?, asset.path().to_owned()))
}

fn hook_slot(value: &Value, property: &str, code: bool) -> Result<Option<HookSlot>> {
    let Some(pointer) = Pointer::from_value(value).map_err(|_| {
        runtime(format!("{property} must be a Pointer, NativeFunction, Callback or nil, got {}", value.type_name()))
    })?
    else {
        return Ok(None);
    };
    if code && pointer.address == 0 {
        return Err(runtime(format!("{property} cannot be a null Pointer")));
    }
    let holds = pointer.owner.hold()?.into_iter().collect();
    Ok(Some(HookSlot { pointer, holds }))
}

fn sampler_mode(value: &Value) -> Result<bool> {
    match value {
        Value::String(mode) => match mode.to_str()?.to_ascii_lowercase().as_str() {
            "pixelated" | "nearest" => Ok(true),
            "smooth" | "linear" => Ok(false),
            other => Err(runtime(format!(
                "unknown sampler mode '{other}', expected \"Smooth\" or \"Pixelated\""
            ))),
        },
        Value::UserData(userdata) if userdata.is::<EnumItem>() => {
            Ok(userdata.borrow::<EnumItem>()?.of(RESAMPLE_MODE)?.name == "Pixelated")
        }
        other => Err(runtime(format!(
            "samplers take an enum.ResampleMode item, got {}",
            other.type_name()
        ))),
    }
}

impl Renderable {
    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn belongs_to(&self, scene: &Weak<Scene>) -> bool {
        Weak::ptr_eq(&self.scene, scene)
    }

    fn gone(&self) -> mlua::Error {
        runtime(format!("{} '{}' has been destroyed", self.kind.class_name(), self.base.name()))
    }

    fn scene(&self) -> Result<Rc<Scene>> {
        self.base.ensure_alive()?;
        self.scene.upgrade().ok_or_else(|| self.gone())
    }

    fn member(&self, property: &str, kinds: &[Kind]) -> Result<()> {
        if kinds.contains(&self.kind) {
            return Ok(());
        }
        Err(runtime(format!(
            "{property} is not a valid member of {} '{}'",
            self.kind.class_name(),
            self.base.name()
        )))
    }

    fn read<T>(&self, property: &str, kinds: &[Kind], action: impl FnOnce(&mut Object) -> Result<T>) -> Result<T> {
        self.member(property, kinds)?;
        let scene = self.scene()?;
        scene.read(self.id, action).unwrap_or_else(|| Err(self.gone()))
    }

    fn write(
        &self,
        property: &str,
        kinds: &[Kind],
        action: impl FnOnce(&mut Object, &mut Resources, &mut BTreeSet<(u32, u32)>) -> Result<()>,
    ) -> Result<()> {
        self.member(property, kinds)?;
        let scene = self.scene()?;
        scene.write(self.id, action).unwrap_or_else(|| Err(self.gone()))
    }

    fn decode(&self) {
        if let Some(scene) = self.scene.upgrade() {
            scene.spawn_decodes();
        }
    }

    fn update(&self, property: &str, kinds: &[Kind], action: impl FnOnce(&mut Object) -> Result<()>) -> Result<()> {
        self.write(property, kinds, |object, _, _| action(object))
    }

    fn set_image(&self, value: AnyUserData) -> Result<()> {
        let (data, name) = asset_data(&value, "Image")?;
        let (width, height) = picture::dimensions(&data, Some(&name))
            .map_err(|error| runtime(format!("'{name}' is not an image the engine can draw: {error}")))?;
        self.write("Image", IMAGE, |object, resources, _| {
            let texture = resources.acquire_texture(data, &name);
            if let Some(old) = object.image.take() {
                resources.release_texture(old.texture);
            }
            object.image = Some(ImageSource {
                asset: value,
                texture,
                width,
                height,
            });
            Ok(())
        })?;
        self.decode();
        Ok(())
    }

    fn set_font(&self, value: AnyUserData) -> Result<()> {
        let (data, name) = asset_data(&value, "Font")?;
        self.write("Font", TEXT, |object, resources, _| {
            let (font, face) = resources
                .acquire_font(data)
                .ok_or_else(|| runtime(format!("'{name}' is not a font file the engine can read")))?;
            match &mut object.text {
                Some(text) => {
                    resources.release_font(text.font);
                    text.asset = value;
                    text.font = font;
                    text.face = face;
                    text.cache = None;
                }
                None => {
                    object.text = Some(TextState {
                        asset: value,
                        font,
                        face,
                        text: String::new(),
                        size: DEFAULT_TEXT_SIZE,
                        bold: false,
                        italic: false,
                        underline: false,
                        strikethrough: false,
                        x_align: Align::Start,
                        y_align: Align::Start,
                        wrapped: false,
                        line_height: 1.0,
                        letter_spacing: 0.0,
                        background: Color::TRANSPARENT,
                        cache: None,
                    })
                }
            }
            Ok(())
        })
    }

    fn text<T>(&self, property: &str, action: impl FnOnce(&TextState) -> T) -> Result<T> {
        self.read(property, TEXT, |object| {
            object
                .text
                .as_ref()
                .map(action)
                .ok_or_else(|| runtime("the text has no font yet"))
        })
    }

    fn set_text(&self, property: &str, action: impl FnOnce(&mut TextState) -> Result<()>) -> Result<()> {
        self.update(property, TEXT, |object| {
            let text = object.text.as_mut().ok_or_else(|| runtime("the text has no font yet"))?;
            action(text)?;
            text.cache = None;
            Ok(())
        })
    }

    fn shader_id(shader: &AnyUserData) -> Result<u64> {
        shader
            .borrow::<Shader>()
            .map(|shader| shader.id())
            .map_err(|_| runtime("expected a Shader"))
    }

    fn load_shader(&self, shader: AnyUserData) -> Result<()> {
        let (id, layout) = {
            let borrowed = shader.borrow::<Shader>().map_err(|_| runtime("LoadShader expects a Shader"))?;
            (borrowed.id(), borrowed.layout()?)
        };
        self.write("LoadShader", SHADED, |object, resources, _| {
            if object.shaders.iter().any(|loaded| loaded.id == id) {
                return Ok(());
            }
            resources.acquire_shader(id, layout.clone());
            object.shaders.push(Loaded {
                userdata: shader,
                id,
                layout,
            });
            Ok(())
        })
    }

    fn remove_shaders(&self, shader: Option<u64>) -> Result<bool> {
        let mut removed = false;
        self.write("RemoveShader", SHADED, |object, resources, dirty| {
            let before = object.shaders.len();
            let mut released = Vec::new();
            object.shaders.retain(|loaded| {
                let keep = shader.is_some_and(|id| id != loaded.id);
                if !keep {
                    released.push(loaded.id);
                }
                keep
            });
            removed = object.shaders.len() != before;
            for id in released {
                resources.release_shader(id);
            }
            let used: HashSet<(u32, u32)> = object
                .shaders
                .iter()
                .flat_map(|loaded| {
                    loaded
                        .layout
                        .data_bindings()
                        .map(|(_, binding)| (binding.group, binding.binding))
                        .collect::<Vec<_>>()
                })
                .collect();
            let stale: Vec<(u32, u32)> = object.slots.keys().filter(|key| !used.contains(key)).copied().collect();
            for key in stale {
                if let Some(Slot::Texture { texture, .. }) = object.slots.remove(&key) {
                    resources.release_texture(texture);
                }
                dirty.insert(key);
            }
            Ok(())
        })?;
        Ok(removed)
    }

    pub fn write_data(&self, shader: &AnyUserData, name: &str, value: Value) -> Result<()> {
        let shader_id = Self::shader_id(shader)?;
        let scene = self.scene()?;
        let me = scene.me().clone();
        let asset = match &value {
            Value::UserData(userdata) if userdata.is::<Asset>() => {
                let (data, path) = asset_data(userdata, "the texture")?;
                Some((userdata.clone(), data, path))
            }
            _ => None,
        };
        let resolve = |userdata: &AnyUserData| -> std::result::Result<Option<ObjectId>, String> {
            let Ok(target) = userdata.borrow::<Renderable>() else {
                return Ok(None);
            };
            if !target.belongs_to(&me) {
                return Err("shaders can only reference renderables from the same window".to_owned());
            }
            if target.base.is_destroyed() {
                return Err(format!("{} '{}' has been destroyed", target.kind.class_name(), target.base.name()));
            }
            Ok(Some(target.id))
        };
        let (class, object_name) = (self.kind.class_name(), self.base.name().to_owned());
        self.write("WriteShaderData", SHADED, |object, resources, dirty| {
            let loaded = object
                .shaders
                .iter()
                .find(|loaded| loaded.id == shader_id)
                .ok_or_else(|| runtime(format!("the shader is not loaded on {class} '{object_name}'")))?;
            let layout = loaded.layout.clone();
            let target = layout.resolve(name).map_err(runtime)?;
            let binding = layout.bindings[target.binding].clone();
            let key = (binding.group, binding.binding);
            match binding.kind {
                BindingKind::Uniform { size } | BindingKind::Storage { size, .. } => {
                    if !matches!(object.slots.get(&key), Some(Slot::Buffer(_))) {
                        let initial = binding.runtime.map_or(size as usize, |(offset, _)| offset as usize);
                        if let Some(Slot::Texture { texture, .. }) = object.slots.insert(key, Slot::Buffer(Bytes::zeroed(initial))) {
                            resources.release_texture(texture);
                        }
                    }
                    let Some(Slot::Buffer(bytes)) = object.slots.get_mut(&key) else {
                        return Err(runtime("the shader data could not be prepared"));
                    };
                    Writer {
                        layout: &layout,
                        resolve: &resolve,
                    }
                    .write(target.ty, &value, target.offset as usize, bytes)
                    .map_err(|error| runtime(format!("cannot write '{name}' in shader '{}': {error}", layout.name)))?;
                }
                BindingKind::Texture { .. } => {
                    let replacement = match (&value, asset) {
                        (Value::Nil, _) => None,
                        (_, Some((userdata, data, path))) => Some(Slot::Texture {
                            texture: resources.acquire_texture(data, &path),
                            asset: userdata,
                        }),
                        (other, None) => {
                            return Err(runtime(format!(
                                "'{name}' in shader '{}' is a texture, so it takes an image Asset, got {}",
                                layout.name,
                                other.type_name()
                            )));
                        }
                    };
                    let old = match replacement {
                        Some(slot) => object.slots.insert(key, slot),
                        None => object.slots.remove(&key),
                    };
                    if let Some(Slot::Texture { texture, .. }) = old {
                        resources.release_texture(texture);
                    }
                }
                BindingKind::Sampler { .. } => {
                    let pixelated = sampler_mode(&value)?;
                    object.slots.insert(key, Slot::Sampler { pixelated });
                }
            }
            dirty.insert(key);
            Ok(())
        })?;
        self.decode();
        Ok(())
    }

    pub fn write_values(&self, shader: &AnyUserData, values: Value) -> Result<()> {
        let Value::Table(values) = values else {
            return Err(runtime(format!(
                "shader data must be a table of names and values, got {}",
                values.type_name()
            )));
        };
        for pair in values.pairs::<Value, Value>() {
            let (name, value) = pair?;
            let Value::String(name) = name else {
                return Err(runtime(format!("shader data names must be strings, got {}", name.type_name())));
            };
            self.write_data(shader, &name.to_str()?, value)?;
        }
        Ok(())
    }

    fn read_data(&self, lua: &Lua, shader: &AnyUserData, name: &str) -> Result<Value> {
        let shader_id = Self::shader_id(shader)?;
        let scene = self.scene()?;
        let (class, object_name) = (self.kind.class_name(), self.base.name().to_owned());
        enum Found {
            Bytes(std::sync::Arc<crate::graphics::reflect::ShaderLayout>, wgpu::naga::Handle<wgpu::naga::Type>, usize, Bytes),
            Value(Value),
            Mode(bool),
        }
        let found = self.read("ReadShaderData", SHADED, |object| {
            let loaded = object
                .shaders
                .iter()
                .find(|loaded| loaded.id == shader_id)
                .ok_or_else(|| runtime(format!("the shader is not loaded on {class} '{object_name}'")))?;
            let layout = loaded.layout.clone();
            let target = layout.resolve(name).map_err(runtime)?;
            let binding = &layout.bindings[target.binding];
            let key = (binding.group, binding.binding);
            Ok(match (binding.kind, object.slots.get(&key)) {
                (BindingKind::Texture { .. }, Some(Slot::Texture { asset, .. })) => Found::Value(Value::UserData(asset.clone())),
                (BindingKind::Texture { .. }, _) => Found::Value(Value::Nil),
                (BindingKind::Sampler { .. }, Some(Slot::Sampler { pixelated })) => Found::Mode(*pixelated),
                (BindingKind::Sampler { .. }, _) => Found::Mode(false),
                (BindingKind::Uniform { size } | BindingKind::Storage { size, .. }, slot) => {
                    let bytes = match slot {
                        Some(Slot::Buffer(bytes)) => bytes.clone(),
                        _ => Bytes::zeroed(binding.runtime.map_or(size as usize, |(offset, _)| offset as usize)),
                    };
                    Found::Bytes(layout.clone(), target.ty, target.offset as usize, bytes)
                }
            })
        })?;
        match found {
            Found::Value(value) => Ok(value),
            Found::Mode(pixelated) => enum_value(lua, RESAMPLE_MODE, if pixelated { "Pixelated" } else { "Smooth" }),
            Found::Bytes(layout, ty, offset, bytes) => {
                let resolve = |id: ObjectId| scene.userdata(id);
                Reader {
                    lua,
                    layout: &layout,
                    resolve: &resolve,
                }
                .read(ty, &bytes, offset)
            }
        }
    }

    pub fn create_post(scene: &Rc<Scene>, lua: &Lua, shader: Option<AnyUserData>) -> Result<AnyUserData> {
        let (id, order) = scene
            .allocate()
            .ok_or_else(|| runtime("post processes cannot be added because the window is closed"))?;
        let userdata = match lua.create_userdata(Renderable {
            base: BaseGameObject::new(Kind::Post.class_name()),
            id,
            kind: Kind::Post,
            scene: Rc::downgrade(scene),
        }) {
            Ok(userdata) => userdata,
            Err(error) => {
                scene.cancel(id);
                return Err(error);
            }
        };
        let mut object = Object::new(Kind::Post, order);
        object.z_index = order as f64;
        scene.insert(id, userdata.clone(), object);
        if let Some(shader) = shader {
            let loaded = userdata.borrow::<Renderable>().and_then(|post| post.load_shader(shader));
            if let Err(error) = loaded {
                if let Ok(mut post) = userdata.borrow_mut::<Renderable>() {
                    post.destroy();
                }
                return Err(error);
            }
        }
        Ok(userdata)
    }

    pub fn create(lua: &Lua, scene: &Rc<Scene>, class: &str, config: Option<Table>) -> Result<AnyUserData> {
        let kind = Kind::from_class(class).ok_or_else(|| {
            runtime(format!(
                "'{class}' is not a renderable class, expected Renderable, RenderableShape, RenderableImage or RenderableText"
            ))
        })?;
        let config = match config {
            Some(config) => config,
            None => lua.create_table()?,
        };
        match kind {
            Kind::Image if config.get::<Value>("Image")?.is_nil() => {
                return Err(runtime("a RenderableImage needs an Image asset in its config"));
            }
            Kind::Text if config.get::<Value>("Font")?.is_nil() => {
                return Err(runtime("a RenderableText needs a Font asset in its config"));
            }
            _ => {}
        }
        let (id, order) = scene
            .allocate()
            .ok_or_else(|| runtime("renderables cannot be created because the window is closed"))?;
        let userdata = match lua.create_userdata(Renderable {
            base: BaseGameObject::new(kind.class_name()),
            id,
            kind,
            scene: Rc::downgrade(scene),
        }) {
            Ok(userdata) => userdata,
            Err(error) => {
                scene.cancel(id);
                return Err(error);
            }
        };
        scene.insert(id, userdata.clone(), Object::new(kind, order));

        let applied = (|| -> Result<()> {
            for key in PRIORITY {
                let value: Value = config.get(key)?;
                if !value.is_nil() {
                    userdata.set(key, value)?;
                }
            }
            let mut sized = false;
            for pair in config.pairs::<Value, Value>() {
                let (key, value) = pair?;
                let Value::String(key) = key else {
                    return Err(runtime(format!("config keys must be strings, got {}", key.type_name())));
                };
                let key = key.to_str()?.to_string();
                if PRIORITY.contains(&key.as_str()) {
                    continue;
                }
                if key == "Shaders" {
                    let Value::Table(shaders) = value else {
                        return Err(runtime("Shaders must be an array of Shader objects"));
                    };
                    for shader in shaders.sequence_values::<AnyUserData>() {
                        userdata.call_method::<()>("LoadShader", shader?)?;
                    }
                    continue;
                }
                sized |= key == "Size";
                userdata.set(key.as_str(), value)?;
            }
            if kind == Kind::Image && !sized {
                let natural: UDim = userdata.get("ImageSize")?;
                userdata.set("Size", natural)?;
            }
            Ok(())
        })();
        if let Err(error) = applied {
            if let Ok(mut renderable) = userdata.borrow_mut::<Renderable>() {
                renderable.destroy();
            }
            return Err(error);
        }
        Ok(userdata)
    }
}

impl GameObject for Renderable {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        if let Some(scene) = self.scene.upgrade() {
            scene.remove(self.id);
        }
        self.scene = Weak::new();
    }
}

impl UserData for Renderable {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);

        fields.add_field_method_get("SinkUpdates", |_, this| this.read("SinkUpdates", ALL, |object| Ok(object.sink)));
        fields.add_field_method_set("SinkUpdates", |_, this, value: bool| {
            this.update("SinkUpdates", ALL, |object| {
                object.sink = value;
                Ok(())
            })
        });
        fields.add_field_method_get("ZIndex", |_, this| this.read("ZIndex", ALL, |object| Ok(object.z_index)));
        fields.add_field_method_set("ZIndex", |_, this, value: f64| {
            let value = finite("ZIndex", value)?;
            this.update("ZIndex", ALL, |object| {
                object.z_index = value;
                Ok(())
            })
        });
        fields.add_field_method_get("BlendMode", |lua, this| {
            let blend = this.read("BlendMode", ALL, |object| Ok(object.blend))?;
            enum_value(lua, BLEND_MODE, blend.name())
        });
        fields.add_field_method_set("BlendMode", |_, this, item: EnumItem| {
            let blend = Blend::from_name(item.of(BLEND_MODE)?.name).unwrap_or(Blend::Alpha);
            this.update("BlendMode", ALL, |object| {
                object.blend = blend;
                Ok(())
            })
        });

        fields.add_field_method_get("RenderHook", |_, this| {
            this.read("RenderHook", SHADED, |object| Ok(object.hook.as_ref().map(|hook| hook.pointer.clone())))
        });
        fields.add_field_method_set("RenderHook", |_, this, value: Value| {
            let slot = hook_slot(&value, "RenderHook", true)?;
            this.update("RenderHook", SHADED, |object| {
                object.hook = slot;
                Ok(())
            })
        });
        fields.add_field_method_get("RenderHookData", |_, this| {
            this.read("RenderHookData", SHADED, |object| {
                Ok(object.hook_data.as_ref().map(|data| data.pointer.clone()))
            })
        });
        fields.add_field_method_set("RenderHookData", |_, this, value: Value| {
            let slot = hook_slot(&value, "RenderHookData", false)?;
            this.update("RenderHookData", SHADED, |object| {
                object.hook_data = slot;
                Ok(())
            })
        });

        fields.add_field_method_get("Enabled", |_, this| this.read("Enabled", POST, |object| Ok(object.sink)));
        fields.add_field_method_set("Enabled", |_, this, value: bool| {
            this.update("Enabled", POST, |object| {
                object.sink = value;
                Ok(())
            })
        });
        fields.add_field_method_get("Order", |_, this| this.read("Order", POST, |object| Ok(object.z_index)));
        fields.add_field_method_set("Order", |_, this, value: f64| {
            let value = finite("Order", value)?;
            this.update("Order", POST, |object| {
                object.z_index = value;
                Ok(())
            })
        });

        fields.add_field_method_get("VertexCount", |_, this| {
            this.read("VertexCount", BASE, |object| Ok(object.vertex_count))
        });
        fields.add_field_method_set("VertexCount", |_, this, value: f64| {
            let value = count("VertexCount", value)?;
            this.update("VertexCount", BASE, |object| {
                object.vertex_count = value;
                Ok(())
            })
        });
        fields.add_field_method_get("InstanceCount", |_, this| {
            this.read("InstanceCount", BASE, |object| Ok(object.instance_count))
        });
        fields.add_field_method_set("InstanceCount", |_, this, value: f64| {
            let value = count("InstanceCount", value)?;
            this.update("InstanceCount", BASE, |object| {
                object.instance_count = value;
                Ok(())
            })
        });

        fields.add_field_method_get("Position", |_, this| this.read("Position", PLACED, |object| Ok(object.position)));
        fields.add_field_method_set("Position", |_, this, value: UDim| {
            let value = finite_udim("Position", value)?;
            this.update("Position", PLACED, |object| {
                object.position = value;
                Ok(())
            })
        });
        fields.add_field_method_get("Size", |_, this| this.read("Size", PLACED, |object| Ok(object.size)));
        fields.add_field_method_set("Size", |_, this, value: UDim| {
            let value = finite_udim("Size", value)?;
            this.update("Size", PLACED, |object| {
                object.size = value;
                object.invalidate_text();
                Ok(())
            })
        });
        fields.add_field_method_get("AnchorPoint", |_, this| {
            this.read("AnchorPoint", PLACED, |object| Ok(object.anchor))
        });
        fields.add_field_method_set("AnchorPoint", |_, this, value: UDim| {
            let value = finite_udim("AnchorPoint", value)?;
            this.update("AnchorPoint", PLACED, |object| {
                object.anchor = value;
                Ok(())
            })
        });
        fields.add_field_method_get("Rotation", |_, this| this.read("Rotation", PLACED, |object| Ok(object.rotation)));
        fields.add_field_method_set("Rotation", |_, this, value: f64| {
            let value = finite("Rotation", value)?;
            this.update("Rotation", PLACED, |object| {
                object.rotation = value;
                Ok(())
            })
        });
        fields.add_field_method_get("Color", |_, this| this.read("Color", PLACED, |object| Ok(object.color)));
        fields.add_field_method_set("Color", |_, this, value: Color| {
            this.update("Color", PLACED, |object| {
                object.color = value;
                Ok(())
            })
        });

        fields.add_field_method_get("Shape", |lua, this| {
            let shape = this.read("Shape", SHAPE, |object| Ok(object.shape))?;
            enum_value(lua, SHAPE_TYPE, shape.name())
        });
        fields.add_field_method_set("Shape", |_, this, item: EnumItem| {
            let shape = ShapeKind::from_name(item.of(SHAPE_TYPE)?.name).unwrap_or(ShapeKind::Rectangle);
            this.update("Shape", SHAPE, |object| {
                object.shape = shape;
                Ok(())
            })
        });
        fields.add_field_method_get("StrokeColor", |_, this| {
            this.read("StrokeColor", STROKED, |object| Ok(object.stroke_color))
        });
        fields.add_field_method_set("StrokeColor", |_, this, value: Color| {
            this.update("StrokeColor", STROKED, |object| {
                object.stroke_color = value;
                Ok(())
            })
        });
        fields.add_field_method_get("StrokeThickness", |_, this| {
            this.read("StrokeThickness", STROKED, |object| Ok(object.stroke))
        });
        fields.add_field_method_set("StrokeThickness", |_, this, value: f64| {
            let value = at_least_zero("StrokeThickness", value)?;
            this.update("StrokeThickness", STROKED, |object| {
                object.stroke = value;
                Ok(())
            })
        });

        fields.add_field_method_get("Image", |_, this| {
            this.read("Image", IMAGE, |object| Ok(object.image.as_ref().map(|image| image.asset.clone())))
        });
        fields.add_field_method_set("Image", |_, this, value: AnyUserData| this.set_image(value));
        fields.add_field_method_get("ImageSize", |_, this| {
            this.read("ImageSize", IMAGE, |object| {
                Ok(object.image.as_ref().map_or(UDim::ZERO, |image| {
                    UDim::new(f64::from(image.width), f64::from(image.height), 0.0)
                }))
            })
        });
        fields.add_field_method_get("OffsetPosition", |_, this| {
            this.read("OffsetPosition", IMAGE, |object| Ok(object.offset_position))
        });
        fields.add_field_method_set("OffsetPosition", |_, this, value: UDim| {
            let value = finite_udim("OffsetPosition", value)?;
            this.update("OffsetPosition", IMAGE, |object| {
                object.offset_position = value;
                Ok(())
            })
        });
        fields.add_field_method_get("OffsetSize", |_, this| {
            this.read("OffsetSize", IMAGE, |object| Ok(object.offset_size))
        });
        fields.add_field_method_set("OffsetSize", |_, this, value: UDim| {
            let value = finite_udim("OffsetSize", value)?;
            this.update("OffsetSize", IMAGE, |object| {
                object.offset_size = value;
                Ok(())
            })
        });
        fields.add_field_method_get("ResampleMode", |lua, this| {
            let pixelated = this.read("ResampleMode", IMAGE, |object| Ok(object.pixelated))?;
            enum_value(lua, RESAMPLE_MODE, if pixelated { "Pixelated" } else { "Smooth" })
        });
        fields.add_field_method_set("ResampleMode", |_, this, item: EnumItem| {
            let pixelated = item.of(RESAMPLE_MODE)?.name == "Pixelated";
            this.update("ResampleMode", IMAGE, |object| {
                object.pixelated = pixelated;
                Ok(())
            })
        });
        fields.add_field_method_get("FlipX", |_, this| this.read("FlipX", IMAGE, |object| Ok(object.flip_x)));
        fields.add_field_method_set("FlipX", |_, this, value: bool| {
            this.update("FlipX", IMAGE, |object| {
                object.flip_x = value;
                Ok(())
            })
        });
        fields.add_field_method_get("FlipY", |_, this| this.read("FlipY", IMAGE, |object| Ok(object.flip_y)));
        fields.add_field_method_set("FlipY", |_, this, value: bool| {
            this.update("FlipY", IMAGE, |object| {
                object.flip_y = value;
                Ok(())
            })
        });

        fields.add_field_method_get("Font", |_, this| this.text("Font", |text| text.asset.clone()));
        fields.add_field_method_set("Font", |_, this, value: AnyUserData| this.set_font(value));
        fields.add_field_method_get("Text", |_, this| this.text("Text", |text| text.text.clone()));
        fields.add_field_method_set("Text", |_, this, value: String| {
            this.set_text("Text", |text| {
                text.text = value;
                Ok(())
            })
        });
        fields.add_field_method_get("TextSize", |_, this| this.text("TextSize", |text| text.size));
        fields.add_field_method_set("TextSize", |_, this, value: f64| {
            if !(value.is_finite() && value > 0.0) {
                return Err(runtime("TextSize must be a number greater than 0"));
            }
            this.set_text("TextSize", |text| {
                text.size = value;
                Ok(())
            })
        });
        for (property, get, set) in [
            ("Bold", (|text: &TextState| text.bold) as fn(&TextState) -> bool, (|text: &mut TextState, value: bool| text.bold = value) as fn(&mut TextState, bool)),
            ("Italic", |text| text.italic, |text, value| text.italic = value),
            ("Underline", |text| text.underline, |text, value| text.underline = value),
            ("Strikethrough", |text| text.strikethrough, |text, value| text.strikethrough = value),
            ("TextWrapped", |text| text.wrapped, |text, value| text.wrapped = value),
        ] {
            fields.add_field_method_get(property, move |_, this| this.text(property, get));
            fields.add_field_method_set(property, move |_, this, value: bool| {
                this.set_text(property, |text| {
                    set(text, value);
                    Ok(())
                })
            });
        }
        for (property, enum_type) in [("TextXAlignment", TEXT_X_ALIGNMENT), ("TextYAlignment", TEXT_Y_ALIGNMENT)] {
            fields.add_field_method_get(property, move |lua, this| {
                let current = this.text(property, |text| {
                    if enum_type == TEXT_X_ALIGNMENT { text.x_align } else { text.y_align }
                })?;
                enum_value(lua, enum_type, align_name(current, enum_type))
            });
            fields.add_field_method_set(property, move |_, this, item: EnumItem| {
                let value = align(item, enum_type)?;
                this.set_text(property, |text| {
                    if enum_type == TEXT_X_ALIGNMENT {
                        text.x_align = value;
                    } else {
                        text.y_align = value;
                    }
                    Ok(())
                })
            });
        }
        fields.add_field_method_get("LineHeight", |_, this| this.text("LineHeight", |text| text.line_height));
        fields.add_field_method_set("LineHeight", |_, this, value: f64| {
            let value = at_least_zero("LineHeight", value)?;
            this.set_text("LineHeight", |text| {
                text.line_height = value;
                Ok(())
            })
        });
        fields.add_field_method_get("LetterSpacing", |_, this| this.text("LetterSpacing", |text| text.letter_spacing));
        fields.add_field_method_set("LetterSpacing", |_, this, value: f64| {
            let value = finite("LetterSpacing", value)?;
            this.set_text("LetterSpacing", |text| {
                text.letter_spacing = value;
                Ok(())
            })
        });
        fields.add_field_method_get("BackgroundColor", |_, this| {
            this.text("BackgroundColor", |text| text.background)
        });
        fields.add_field_method_set("BackgroundColor", |_, this, value: Color| {
            this.update("BackgroundColor", TEXT, |object| {
                if let Some(text) = &mut object.text {
                    text.background = value;
                }
                Ok(())
            })
        });
        fields.add_field_method_get("TextBounds", |_, this| {
            this.read("TextBounds", TEXT, |object| {
                Ok(object
                    .text_cache()
                    .map_or(UDim::ZERO, |cache| UDim::new(cache.bounds[0], cache.bounds[1], 0.0)))
            })
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_method("LoadShader", |_, this, shader: AnyUserData| this.load_shader(shader));
        methods.add_method("RemoveShader", |_, this, shader: AnyUserData| {
            let id = Self::shader_id(&shader)?;
            this.remove_shaders(Some(id))
        });
        methods.add_method("ClearShaders", |_, this, ()| this.remove_shaders(None).map(|_| ()));
        methods.add_method("HasShader", |_, this, shader: AnyUserData| {
            let id = Self::shader_id(&shader)?;
            this.read("HasShader", SHADED, |object| Ok(object.shaders.iter().any(|loaded| loaded.id == id)))
        });
        methods.add_method("GetShaders", |lua, this, ()| {
            let shaders = this.read("GetShaders", SHADED, |object| {
                Ok(object.shaders.iter().map(|loaded| loaded.userdata.clone()).collect::<Vec<_>>())
            })?;
            lua.create_sequence_from(shaders)
        });
        methods.add_method("WriteShaderData", |_, this, (shader, name, value): (AnyUserData, Value, Value)| {
            match name {
                Value::String(name) => this.write_data(&shader, &name.to_str()?, value),
                table @ Value::Table(_) => this.write_values(&shader, table),
                other => Err(runtime(format!(
                    "WriteShaderData expects a data name or a table of names and values, got {}",
                    other.type_name()
                ))),
            }
        });
        methods.add_method("ReadShaderData", |lua, this, (shader, name): (AnyUserData, String)| {
            this.read_data(lua, &shader, &name)
        });
    }
}
