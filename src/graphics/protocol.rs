use std::any::Any;
use std::fmt;
use std::sync::Arc;

use tokio::sync::oneshot;

use super::geometry::{Collider, Hit, Outline, Query, ShapeKind};
use super::reflect::ShaderLayout;
use super::text::PlacedGlyph;

pub type ObjectId = u64;
pub type TextureId = u64;
pub type FontId = u64;
pub type ShaderId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Blend {
    Alpha,
    Additive,
    Multiply,
    Opaque,
}

impl Blend {
    pub const ALL: [Blend; 4] = [Blend::Alpha, Blend::Additive, Blend::Multiply, Blend::Opaque];

    pub fn name(self) -> &'static str {
        match self {
            Blend::Alpha => "Alpha",
            Blend::Additive => "Additive",
            Blend::Multiply => "Multiply",
            Blend::Opaque => "Opaque",
        }
    }

    pub fn from_name(name: &str) -> Option<Blend> {
        Self::ALL.into_iter().find(|blend| blend.name() == name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: [f64; 2],
    pub size: [f64; 2],
    pub anchor: [f64; 2],
    pub rotation: f64,
}

impl Transform {
    pub fn collider(&self, id: ObjectId, shape: ShapeKind, outline: Option<Outline>) -> Collider {
        Collider {
            outline,
            id,
            position: self.position,
            size: self.size,
            anchor: self.anchor,
            rotation: self.rotation,
            shape,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextContent {
    pub font: FontId,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub glyphs: Vec<PlacedGlyph>,
    pub decorations: Vec<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    Custom {
        vertex_count: u32,
        instance_count: u32,
    },
    Post {
        vertex_count: u32,
        instance_count: u32,
    },
    Shape {
        transform: Transform,
        color: [f32; 4],
        shape: ShapeKind,
        outline: Option<Outline>,
        stroke_color: [f32; 4],
        stroke: f32,
    },
    Image {
        transform: Transform,
        color: [f32; 4],
        texture: TextureId,
        uv: [f32; 4],
        pixelated: bool,
    },
    Text {
        transform: Transform,
        color: [f32; 4],
        background: [f32; 4],
        stroke_color: [f32; 4],
        stroke: f32,
        content: Arc<TextContent>,
    },
}

impl Body {
    pub fn collider(&self, id: ObjectId) -> Option<Collider> {
        match self {
            Body::Custom { .. } | Body::Post { .. } => None,
            Body::Shape {
                transform,
                shape,
                outline,
                ..
            } => Some(transform.collider(id, *shape, outline.clone())),
            Body::Image { transform, .. } | Body::Text { transform, .. } => {
                Some(transform.collider(id, ShapeKind::Rectangle, None))
            }
        }
    }
}

#[derive(Clone)]
pub struct NativeHook {
    pub function: usize,
    pub data: usize,
    pub keep: Vec<Arc<dyn Any + Send + Sync>>,
}

impl fmt::Debug for NativeHook {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeHook")
            .field("function", &self.function)
            .field("data", &self.data)
            .finish()
    }
}

impl PartialEq for NativeHook {
    fn eq(&self, other: &Self) -> bool {
        self.function == other.function && self.data == other.data
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub id: ObjectId,
    pub order: u64,
    pub z_index: f64,
    pub blend: Blend,
    pub shaders: Arc<[ShaderId]>,
    pub body: Body,
    pub hook: Option<NativeHook>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SlotContent {
    Buffer {
        bytes: Vec<u8>,
        references: Vec<(u32, ObjectId)>,
    },
    Texture(TextureId),
    Sampler {
        pixelated: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SlotWrite {
    pub object: ObjectId,
    pub group: u32,
    pub binding: u32,
    pub content: SlotContent,
}

pub enum Resource {
    Texture {
        id: TextureId,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    Font {
        id: FontId,
        data: Arc<[u8]>,
    },
    Shader {
        id: ShaderId,
        layout: Arc<ShaderLayout>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Release {
    Texture(TextureId),
    Font(FontId),
    Shader(ShaderId),
}

#[derive(Default)]
pub struct SceneDelta {
    pub resources: Vec<Resource>,
    pub removals: Vec<ObjectId>,
    pub upserts: Vec<Snapshot>,
    pub slots: Vec<SlotWrite>,
    pub releases: Vec<Release>,
}

impl SceneDelta {
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
            && self.removals.is_empty()
            && self.upserts.is_empty()
            && self.slots.is_empty()
            && self.releases.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameInfo {
    pub background: [f32; 4],
    pub time: f64,
    pub delta: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub type QueryReply = oneshot::Sender<Result<Vec<Hit>, String>>;
pub type CaptureReply = oneshot::Sender<Result<Capture, String>>;
pub type WarmReply = oneshot::Sender<()>;

pub enum RenderCommand {
    Delta(SceneDelta),
    Present(FrameInfo),
    Redraw,
    Query(Query, QueryReply),
    Capture(FrameInfo, CaptureReply),
    Warm(Vec<ObjectId>, WarmReply),
}
