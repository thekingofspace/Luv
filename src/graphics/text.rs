use std::cell::RefCell;
use std::sync::Arc;

use swash::shape::ShapeContext;
use swash::{CacheKey, FontRef};

thread_local! {
    static SHAPER: RefCell<ShapeContext> = RefCell::new(ShapeContext::new());
}

#[derive(Clone)]
pub struct Font {
    data: Arc<[u8]>,
    offset: u32,
    key: CacheKey,
}

impl Font {
    pub fn parse(data: Arc<[u8]>) -> Option<Font> {
        let font = FontRef::from_index(&data, 0)?;
        let (offset, key) = (font.offset, font.key);
        Some(Font { data, offset, key })
    }

    pub fn reference(&self) -> FontRef<'_> {
        FontRef {
            data: &self.data,
            offset: self.offset,
            key: self.key,
        }
    }

    pub fn data(&self) -> &Arc<[u8]> {
        &self.data
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Align {
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub bold: bool,
    pub letter_spacing: f32,
    pub line_height: f32,
    pub wrap: Option<f32>,
    pub underline: bool,
    pub strikethrough: bool,
}

pub fn bold_strength(size: f32) -> f32 {
    size / 24.0
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub id: u16,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Line {
    glyphs: Vec<PlacedGlyph>,
    width: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlacedText {
    pub glyphs: Vec<PlacedGlyph>,
    pub decorations: Vec<[f32; 4]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextLayout {
    lines: Vec<Line>,
    line_height: f32,
    baseline: f32,
    underline: Option<(f32, f32)>,
    strikethrough: Option<(f32, f32)>,
    pub width: f32,
    pub height: f32,
}

struct Cluster {
    glyphs: Vec<PlacedGlyph>,
    advance: f32,
    space: bool,
}

fn shape(font: FontRef<'_>, text: &str, style: &TextStyle) -> Vec<Cluster> {
    let extra = if style.bold { bold_strength(style.size) } else { 0.0 };
    let mut clusters = Vec::new();
    SHAPER.with(|context| {
        let mut context = context.borrow_mut();
        let mut shaper = context.builder(font).size(style.size).build();
        shaper.add_str(text);
        shaper.shape_with(|cluster| {
            let space = cluster.info.is_whitespace();
            let mut pen = 0.0;
            let mut glyphs = Vec::with_capacity(cluster.glyphs.len());
            for glyph in cluster.glyphs {
                glyphs.push(PlacedGlyph {
                    id: glyph.id,
                    x: pen + glyph.x,
                    y: -glyph.y,
                });
                pen += glyph.advance;
            }
            let advance = pen + style.letter_spacing + if space { 0.0 } else { extra };
            clusters.push(Cluster { glyphs, advance, space });
        });
    });
    clusters
}

impl TextLayout {
    pub fn new(font: &Font, text: &str, style: &TextStyle) -> TextLayout {
        let reference = font.reference();
        let metrics = reference.metrics(&[]).scale(style.size);
        let natural = metrics.ascent + metrics.descent + metrics.leading;
        let line_height = (natural * style.line_height.max(0.0)).max(0.0);
        let baseline = (line_height - (metrics.ascent + metrics.descent)) * 0.5 + metrics.ascent;
        let thickness = if metrics.stroke_size > 0.0 {
            metrics.stroke_size
        } else {
            (style.size / 14.0).max(1.0)
        };
        let underline_offset = if metrics.underline_offset != 0.0 {
            metrics.underline_offset
        } else {
            -style.size * 0.1
        };
        let strikeout_offset = if metrics.strikeout_offset != 0.0 {
            metrics.strikeout_offset
        } else {
            metrics.x_height * 0.5
        };

        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let paragraph = paragraph.strip_suffix('\r').unwrap_or(paragraph);
            let clusters = shape(reference, paragraph, style);
            let mut line = Line::default();
            let mut pen = 0.0_f32;
            let mut index = 0;
            while index < clusters.len() {
                let start = index;
                while index < clusters.len() && !clusters[index].space {
                    index += 1;
                }
                let word_end = index;
                while index < clusters.len() && clusters[index].space {
                    index += 1;
                }
                let word: f32 = clusters[start..word_end].iter().map(|cluster| cluster.advance).sum();
                if let Some(limit) = style.wrap
                    && pen > 0.0
                    && pen + word > limit + 1e-3
                {
                    lines.push(std::mem::take(&mut line));
                    pen = 0.0;
                }
                for cluster in &clusters[start..index] {
                    for glyph in &cluster.glyphs {
                        line.glyphs.push(PlacedGlyph {
                            id: glyph.id,
                            x: pen + glyph.x,
                            y: glyph.y,
                        });
                    }
                    pen += cluster.advance;
                    if !cluster.space {
                        line.width = (pen - style.letter_spacing).max(0.0);
                    }
                }
            }
            lines.push(line);
        }

        let width = lines.iter().map(|line| line.width).fold(0.0, f32::max);
        let height = line_height * lines.len() as f32;
        TextLayout {
            lines,
            line_height,
            baseline,
            underline: style.underline.then_some((-underline_offset, thickness)),
            strikethrough: style.strikethrough.then_some((-strikeout_offset, thickness)),
            width,
            height,
        }
    }

    pub fn bounds(&self) -> [f32; 2] {
        [self.width, self.height]
    }

    pub fn place(&self, size: [f32; 2], x: Align, y: Align) -> PlacedText {
        let offset = |align: Align, room: f32, used: f32| match align {
            Align::Start => 0.0,
            Align::Center => (room - used) * 0.5,
            Align::End => room - used,
        };
        let top = offset(y, size[1], self.height);
        let mut placed = PlacedText::default();
        for (index, line) in self.lines.iter().enumerate() {
            let left = offset(x, size[0], line.width);
            let baseline = top + self.line_height * index as f32 + self.baseline;
            placed.glyphs.extend(line.glyphs.iter().map(|glyph| PlacedGlyph {
                id: glyph.id,
                x: left + glyph.x,
                y: baseline + glyph.y,
            }));
            if line.width > 0.0 {
                for (position, thickness) in [self.underline, self.strikethrough].into_iter().flatten() {
                    placed.decorations.push([left, baseline + position, line.width, thickness]);
                }
            }
        }
        placed
    }
}
