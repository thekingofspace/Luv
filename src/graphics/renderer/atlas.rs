use std::collections::HashMap;

use swash::scale::image::Image;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Angle, Format, Stroke, Transform};

use crate::graphics::protocol::FontId;
use crate::graphics::text::{Font, bold_strength};

pub const ITALIC_ANGLE: f32 = 12.0;
const START_SIZE: u32 = 1024;
const MAX_SIZE: u32 = 8192;
const PADDING: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub font: FontId,
    pub glyph: u16,
    pub size: u32,
    pub bold: bool,
    pub italic: bool,
    pub stroke: u32,
}

impl GlyphKey {
    pub fn quantize(value: f64) -> u32 {
        (value * 4.0).round().clamp(0.0, u32::MAX as f64) as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphEntry {
    pub uv: [f32; 4],
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug)]
pub struct AtlasFull;

struct Shelf {
    y: u32,
    height: u32,
    x: u32,
}

pub struct Atlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    size: u32,
    shelves: Vec<Shelf>,
    next_y: u32,
    glyphs: HashMap<GlyphKey, Option<GlyphEntry>>,
    context: ScaleContext,
}

fn create(device: &wgpu::Device, size: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("glyph atlas"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn rasterize(context: &mut ScaleContext, font: &Font, key: GlyphKey) -> Option<Image> {
    let size = key.size as f32 / 4.0;
    let mut scaler = context.builder(font.reference()).size(size).hint(false).build();
    let mut render = Render::new(&[Source::Outline]);
    render.format(Format::Alpha);
    if key.bold {
        render.embolden(bold_strength(size));
    }
    if key.italic {
        render.transform(Some(Transform::skew(Angle::from_degrees(ITALIC_ANGLE), Angle::ZERO)));
    }
    if key.stroke > 0 {
        render.style(Stroke::new(key.stroke as f32 / 2.0));
    }
    render.render(&mut scaler, key.glyph)
}

impl Atlas {
    pub fn new(device: &wgpu::Device) -> Atlas {
        let (texture, view) = create(device, START_SIZE);
        Atlas {
            texture,
            view,
            size: START_SIZE,
            shelves: Vec::new(),
            next_y: 0,
            glyphs: HashMap::new(),
            context: ScaleContext::new(),
        }
    }

    pub fn grow(&mut self, device: &wgpu::Device) -> bool {
        self.clear();
        if self.size >= MAX_SIZE {
            return false;
        }
        self.size *= 2;
        let (texture, view) = create(device, self.size);
        self.texture = texture;
        self.view = view;
        true
    }

    pub fn clear(&mut self) {
        self.shelves.clear();
        self.next_y = 0;
        self.glyphs.clear();
    }

    pub fn forget_font(&mut self, font: FontId) {
        self.glyphs.retain(|key, _| key.font != font);
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let (width, height) = (width + PADDING, height + PADDING);
        if width > self.size || height > self.size {
            return None;
        }
        for shelf in &mut self.shelves {
            if height <= shelf.height && shelf.x + width <= self.size {
                let x = shelf.x;
                shelf.x += width;
                return Some((x, shelf.y));
            }
        }
        if self.next_y + height > self.size {
            return None;
        }
        let y = self.next_y;
        self.shelves.push(Shelf { y, height, x: width });
        self.next_y += height;
        Some((0, y))
    }

    pub fn glyph(&mut self, queue: &wgpu::Queue, font: &Font, key: GlyphKey) -> Result<Option<GlyphEntry>, AtlasFull> {
        if let Some(entry) = self.glyphs.get(&key) {
            return Ok(*entry);
        }
        let entry = match rasterize(&mut self.context, font, key) {
            Some(image) if image.placement.width > 0 && image.placement.height > 0 => {
                let placement = image.placement;
                let (x, y) = self.allocate(placement.width, placement.height).ok_or(AtlasFull)?;
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &self.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x, y, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &image.data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(placement.width),
                        rows_per_image: Some(placement.height),
                    },
                    wgpu::Extent3d {
                        width: placement.width,
                        height: placement.height,
                        depth_or_array_layers: 1,
                    },
                );
                let size = self.size as f32;
                Some(GlyphEntry {
                    uv: [
                        x as f32 / size,
                        y as f32 / size,
                        (x + placement.width) as f32 / size,
                        (y + placement.height) as f32 / size,
                    ],
                    left: placement.left as f32,
                    top: placement.top as f32,
                    width: placement.width as f32,
                    height: placement.height as f32,
                })
            }
            _ => None,
        };
        self.glyphs.insert(key, entry);
        Ok(entry)
    }
}
