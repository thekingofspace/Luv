use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use image::{ImageFormat, ImageReader};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree, fontdb};

pub type Pixels = (u32, u32, Vec<u8>);

const MAX_SIDE: u32 = 16_384;
const FORMATS: &str = "PNG, JPEG, GIF, WebP, BMP, ICO, TIFF, TGA, DDS, HDR, EXR, PNM, QOI, farbfeld or SVG";

fn fonts() -> Arc<fontdb::Database> {
    static FONTS: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut database = fontdb::Database::new();
            database.load_system_fonts();
            Arc::new(database)
        })
        .clone()
}

fn looks_like_svg(data: &[u8]) -> bool {
    if data.starts_with(&[0x1f, 0x8b]) {
        return true;
    }
    let head = String::from_utf8_lossy(&data[..data.len().min(4096)]);
    let start = head.trim_start_matches('\u{feff}').trim_start();
    start.starts_with('<') && head.contains("<svg")
}

fn svg(data: &[u8]) -> Result<(Tree, u32, u32), String> {
    let mut options = Options::default();
    if String::from_utf8_lossy(data).contains("<text") || data.starts_with(&[0x1f, 0x8b]) {
        options.fontdb = fonts();
    }
    let tree = Tree::from_data(data, &options).map_err(|error| format!("the SVG cannot be read: {error}"))?;
    let size = tree.size();
    let width = size.width().ceil().max(1.0) as u32;
    let height = size.height().ceil().max(1.0) as u32;
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!("the SVG is {width}x{height}, images can be at most {MAX_SIDE} pixels on a side"));
    }
    Ok((tree, width, height))
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(index) = rest.find(name) {
        let spaced = rest[..index].chars().last().is_some_and(char::is_whitespace);
        let after = &rest[index + name.len()..];
        if spaced && let Some(value) = after.trim_start().strip_prefix('=') {
            let value = value.trim_start();
            let quote = value.chars().next()?;
            if quote == '"' || quote == '\'' {
                let inner = &value[1..];
                return inner.find(quote).map(|end| &inner[..end]);
            }
        }
        rest = after;
    }
    None
}

fn length(value: &str) -> Option<f32> {
    let value = value.trim();
    value
        .strip_suffix("px")
        .unwrap_or(value)
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|number| number.is_finite() && *number > 0.0)
}

fn quick_size(data: &[u8]) -> Option<(f32, f32)> {
    let text = String::from_utf8_lossy(&data[..data.len().min(65_536)]);
    let start = text.find("<svg")?;
    let end = text[start..].find('>')? + start;
    let tag = &text[start..end];
    let view = attribute(tag, "viewBox").and_then(|view| {
        let parts = view
            .split(|character: char| character == ',' || character.is_whitespace())
            .filter(|part| !part.is_empty())
            .map(|part| part.parse::<f32>().ok())
            .collect::<Option<Vec<f32>>>()?;
        (parts.len() == 4 && parts[2] > 0.0 && parts[3] > 0.0).then(|| (parts[2], parts[3]))
    });
    let width = attribute(tag, "width").map(length);
    let height = attribute(tag, "height").map(length);
    match (width, height, view) {
        (Some(Some(width)), Some(Some(height)), _) => Some((width, height)),
        (None, None, Some(view)) => Some(view),
        (Some(Some(width)), None, Some((view_width, view_height))) => Some((width, width * view_height / view_width)),
        (None, Some(Some(height)), Some((view_width, view_height))) => Some((height * view_width / view_height, height)),
        (None, None, None) => Some((100.0, 100.0)),
        _ => None,
    }
}

fn raster<'a>(data: &'a [u8], name: Option<&str>) -> Option<ImageReader<Cursor<&'a [u8]>>> {
    let reader = ImageReader::new(Cursor::new(data)).with_guessed_format().ok()?;
    if reader.format().is_some() {
        return Some(reader);
    }
    let format = Path::new(name?).extension().and_then(ImageFormat::from_extension)?;
    let mut reader = ImageReader::new(Cursor::new(data));
    reader.set_format(format);
    Some(reader)
}

pub fn dimensions(data: &[u8], name: Option<&str>) -> Result<(u32, u32), String> {
    if let Some(reader) = raster(data, name) {
        return reader.into_dimensions().map_err(|error| error.to_string());
    }
    if looks_like_svg(data) {
        if let Some((width, height)) = quick_size(data) {
            let width = width.ceil().max(1.0) as u32;
            let height = height.ceil().max(1.0) as u32;
            if width <= MAX_SIDE && height <= MAX_SIDE {
                return Ok((width, height));
            }
        }
        let (_, width, height) = svg(data)?;
        return Ok((width, height));
    }
    Err(format!("the format is not recognised, images can be {FORMATS}"))
}

fn render(tree: &Tree, width: u32, height: u32) -> Result<Pixels, String> {
    let mut pixmap = Pixmap::new(width, height).ok_or_else(|| "the SVG has no drawable area".to_owned())?;
    let size = tree.size();
    let transform = Transform::from_scale(width as f32 / size.width(), height as f32 / size.height());
    resvg::render(tree, transform, &mut pixmap.as_mut());
    Ok((width, height, pixmap.take_demultiplied()))
}

pub fn decode(data: &[u8], name: Option<&str>) -> Result<Pixels, String> {
    if let Some(reader) = raster(data, name) {
        let image = reader.decode().map_err(|error| error.to_string())?.to_rgba8();
        return Ok((image.width(), image.height(), image.into_raw()));
    }
    if looks_like_svg(data) {
        let (tree, width, height) = svg(data)?;
        return render(&tree, width, height);
    }
    Err(format!("the format is not recognised, images can be {FORMATS}"))
}

pub fn decode_fitted(data: &[u8], name: Option<&str>, side: u32) -> Result<Pixels, String> {
    if raster(data, name).is_none() && looks_like_svg(data) {
        let (tree, width, height) = svg(data)?;
        let scale = side as f32 / width.max(height) as f32;
        let fitted = |length: u32| ((length as f32 * scale).round() as u32).clamp(1, side);
        return render(&tree, fitted(width), fitted(height));
    }
    decode(data, name)
}

pub fn square((width, height, rgba): Pixels) -> Pixels {
    if width == height {
        return (width, height, rgba);
    }
    let side = width.max(height) as usize;
    let row = width as usize * 4;
    let left = (side - width as usize) / 2;
    let top = (side - height as usize) / 2;
    let mut output = vec![0; side * side * 4];
    for (y, line) in rgba.chunks_exact(row).enumerate() {
        let start = ((top + y) * side + left) * 4;
        output[start..start + row].copy_from_slice(line);
    }
    (side as u32, side as u32, output)
}
