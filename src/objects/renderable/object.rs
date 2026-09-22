use std::collections::BTreeMap;
use std::sync::Arc;

use mlua::AnyUserData;

use super::data::Bytes;
use crate::datatypes::{Color, UDim};
use crate::graphics::geometry::{Collider, ShapeKind};
use crate::graphics::protocol::{
    Blend, Body, FontId, NativeHook, ObjectId, ShaderId, SlotContent, Snapshot, TextContent, TextureId, Transform,
};
use crate::graphics::reflect::ShaderLayout;
use crate::graphics::text::{Align, Font, TextLayout, TextStyle};
use crate::native::{Hold, Pointer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Renderable,
    Shape,
    Image,
    Text,
    Post,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Renderable, Kind::Shape, Kind::Image, Kind::Text];

    pub fn class_name(self) -> &'static str {
        match self {
            Kind::Renderable => "Renderable",
            Kind::Shape => "RenderableShape",
            Kind::Image => "RenderableImage",
            Kind::Text => "RenderableText",
            Kind::Post => "PostProcess",
        }
    }

    pub fn from_class(name: &str) -> Option<Kind> {
        Self::ALL.into_iter().find(|kind| kind.class_name() == name)
    }
}

pub struct Loaded {
    pub userdata: AnyUserData,
    pub id: ShaderId,
    pub layout: Arc<ShaderLayout>,
}

pub enum Slot {
    Buffer(Bytes),
    Texture { asset: AnyUserData, texture: TextureId },
    Sampler { pixelated: bool },
}

impl Slot {
    pub fn content(&self) -> SlotContent {
        match self {
            Slot::Buffer(bytes) => SlotContent::Buffer {
                bytes: bytes.bytes.clone(),
                references: bytes.references.iter().map(|(offset, target)| (*offset, *target)).collect(),
            },
            Slot::Texture { texture, .. } => SlotContent::Texture(*texture),
            Slot::Sampler { pixelated } => SlotContent::Sampler { pixelated: *pixelated },
        }
    }
}

#[derive(Clone)]
pub struct HookSlot {
    pub pointer: Pointer,
    pub holds: Vec<Hold>,
}

pub struct ImageSource {
    pub asset: AnyUserData,
    pub texture: TextureId,
    pub width: u32,
    pub height: u32,
}

pub struct TextCache {
    pub bounds: [f64; 2],
    pub size: [f64; 2],
    pub content: Arc<TextContent>,
}

pub struct TextState {
    pub asset: AnyUserData,
    pub font: FontId,
    pub face: Font,
    pub text: String,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub x_align: Align,
    pub y_align: Align,
    pub wrapped: bool,
    pub line_height: f64,
    pub letter_spacing: f64,
    pub background: Color,
    pub cache: Option<TextCache>,
}

pub struct Object {
    pub kind: Kind,
    pub order: u64,
    pub z_index: f64,
    pub blend: Blend,
    pub sink: bool,
    pub shaders: Vec<Loaded>,
    pub slots: BTreeMap<(u32, u32), Slot>,
    pub vertex_count: u32,
    pub instance_count: u32,
    pub position: UDim,
    pub size: UDim,
    pub anchor: UDim,
    pub rotation: f64,
    pub color: Color,
    pub shape: ShapeKind,
    pub stroke_color: Color,
    pub stroke: f64,
    pub image: Option<ImageSource>,
    pub offset_position: UDim,
    pub offset_size: UDim,
    pub pixelated: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    pub text: Option<TextState>,
    pub hook: Option<HookSlot>,
    pub hook_data: Option<HookSlot>,
}

fn color(color: Color) -> [f32; 4] {
    [color.r as f32, color.g as f32, color.b as f32, color.a as f32]
}

impl Object {
    pub fn new(kind: Kind, order: u64) -> Object {
        Object {
            kind,
            order,
            z_index: 0.0,
            blend: Blend::Alpha,
            sink: true,
            shaders: Vec::new(),
            slots: BTreeMap::new(),
            vertex_count: 6,
            instance_count: 1,
            position: UDim::ZERO,
            size: match kind {
                Kind::Shape => UDim::new(100.0, 100.0, 0.0),
                _ => UDim::ZERO,
            },
            anchor: UDim::new(0.5, 0.5, 0.0),
            rotation: 0.0,
            color: Color::WHITE,
            shape: ShapeKind::Rectangle,
            stroke_color: Color::BLACK,
            stroke: 0.0,
            image: None,
            offset_position: UDim::ZERO,
            offset_size: UDim::ZERO,
            pixelated: false,
            flip_x: false,
            flip_y: false,
            text: None,
            hook: None,
            hook_data: None,
        }
    }

    pub fn invalidate_text(&mut self) {
        if let Some(text) = &mut self.text {
            text.cache = None;
        }
    }

    pub fn text_cache(&mut self) -> Option<&TextCache> {
        let size = [self.size.x, self.size.y];
        let text = self.text.as_mut()?;
        if text.cache.is_none() {
            let style = TextStyle {
                size: text.size as f32,
                bold: text.bold,
                letter_spacing: text.letter_spacing as f32,
                line_height: text.line_height as f32,
                wrap: (text.wrapped && size[0] > 0.0).then_some(size[0] as f32),
                underline: text.underline,
                strikethrough: text.strikethrough,
            };
            let layout = TextLayout::new(&text.face, &text.text, &style);
            let bounds = layout.bounds().map(|value| (f64::from(value) * 10_000.0).round() / 10_000.0);
            let effective = [
                if size[0] != 0.0 { size[0] } else { bounds[0] },
                if size[1] != 0.0 { size[1] } else { bounds[1] },
            ];
            let placed = layout.place([effective[0] as f32, effective[1] as f32], text.x_align, text.y_align);
            text.cache = Some(TextCache {
                bounds,
                size: effective,
                content: Arc::new(TextContent {
                    font: text.font,
                    size: text.size as f32,
                    bold: text.bold,
                    italic: text.italic,
                    glyphs: placed.glyphs,
                    decorations: placed.decorations,
                }),
            });
        }
        text.cache.as_ref()
    }

    pub fn box_size(&mut self) -> [f64; 2] {
        if self.kind == Kind::Text
            && let Some(cache) = self.text_cache()
        {
            return cache.size;
        }
        [self.size.x, self.size.y]
    }

    pub fn transform(&mut self) -> Transform {
        Transform {
            position: [self.position.x, self.position.y],
            size: self.box_size(),
            anchor: [self.anchor.x, self.anchor.y],
            rotation: self.rotation.to_radians(),
        }
    }

    pub fn uv(&self) -> [f32; 4] {
        let Some(image) = &self.image else {
            return [0.0, 0.0, 1.0, 1.0];
        };
        let width = f64::from(image.width.max(1));
        let height = f64::from(image.height.max(1));
        let (left, top) = (self.offset_position.x, self.offset_position.y);
        let span_x = if self.offset_size.x != 0.0 { self.offset_size.x } else { width - left };
        let span_y = if self.offset_size.y != 0.0 { self.offset_size.y } else { height - top };
        let mut uv = [left / width, top / height, (left + span_x) / width, (top + span_y) / height];
        if self.flip_x {
            uv.swap(0, 2);
        }
        if self.flip_y {
            uv.swap(1, 3);
        }
        uv.map(|value| value as f32)
    }

    pub fn collider(&mut self, id: ObjectId) -> Option<Collider> {
        let shape = match self.kind {
            Kind::Renderable | Kind::Post => return None,
            Kind::Shape => self.shape,
            Kind::Image | Kind::Text => ShapeKind::Rectangle,
        };
        Some(self.transform().collider(id, shape))
    }

    pub fn snapshot(&mut self, id: ObjectId) -> Snapshot {
        let body = match self.kind {
            Kind::Renderable => Body::Custom {
                vertex_count: self.vertex_count,
                instance_count: self.instance_count,
            },
            Kind::Post => Body::Post {
                vertex_count: self.vertex_count,
                instance_count: self.instance_count,
            },
            Kind::Shape => Body::Shape {
                transform: self.transform(),
                color: color(self.color),
                shape: self.shape,
                stroke_color: color(self.stroke_color),
                stroke: self.stroke as f32,
            },
            Kind::Image => Body::Image {
                transform: self.transform(),
                color: color(self.color),
                texture: self.image.as_ref().map_or(0, |image| image.texture),
                uv: self.uv(),
                pixelated: self.pixelated,
            },
            Kind::Text => {
                let transform = self.transform();
                let content = self
                    .text_cache()
                    .map(|cache| cache.content.clone())
                    .unwrap_or_else(|| {
                        Arc::new(TextContent {
                            font: 0,
                            size: 0.0,
                            bold: false,
                            italic: false,
                            glyphs: Vec::new(),
                            decorations: Vec::new(),
                        })
                    });
                let background = self.text.as_ref().map_or(Color::TRANSPARENT, |text| text.background);
                Body::Text {
                    transform,
                    color: color(self.color),
                    background: color(background),
                    stroke_color: color(self.stroke_color),
                    stroke: self.stroke as f32,
                    content,
                }
            }
        };
        let hook = self.hook.as_ref().map(|hook| NativeHook {
            function: hook.pointer.address,
            data: self.hook_data.as_ref().map_or(0, |data| data.pointer.address),
            keep: hook
                .holds
                .iter()
                .chain(self.hook_data.iter().flat_map(|data| data.holds.iter()))
                .cloned()
                .map(Hold::erase)
                .collect(),
        });
        Snapshot {
            id,
            order: self.order,
            z_index: self.z_index,
            blend: self.blend,
            shaders: self.shaders.iter().map(|loaded| loaded.id).collect(),
            body,
            hook,
        }
    }
}
