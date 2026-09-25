mod common;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use common::font::block_font;
use common::{run_with, workspace};
use luv::graphics::gpu::Gpu;
use luv::graphics::protocol::Capture;
use luv::objects::Window;
use luv::project::Project;
use luv::window::HeadlessWindows;
use mlua::{AnyUserData, Table};
use tempfile::TempDir;

type Captures = Arc<Mutex<HashMap<String, Capture>>>;

struct Run {
    outcome: common::Outcome,
    captures: HashMap<String, Capture>,
}

fn gpu() -> bool {
    match Gpu::get() {
        Ok(_) => true,
        Err(error) => {
            eprintln!("skipping a rendering test: {error}");
            false
        }
    }
}

fn project(script: &str) -> TempDir {
    let dir = workspace(&[("src/main.luau", script)]);
    let assets = dir.path().join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::write(assets.join("block.ttf"), block_font()).unwrap();
    let mut quad = image::RgbaImage::new(2, 2);
    quad.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    quad.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
    quad.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
    quad.put_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
    quad.save(assets.join("quad.png")).unwrap();
    std::fs::write(assets.join("notes.txt"), "not an image").unwrap();
    let mut sheet = image::RgbaImage::new(4, 2);
    for y in 0..2 {
        for x in 0..4 {
            let solid = x >= 2 || (x == 0 && y == 0);
            let alpha = if solid { 255 } else { 0 };
            sheet.put_pixel(x, y, image::Rgba([255, 255, 255, alpha]));
        }
    }
    sheet.save(assets.join("sheet.png")).unwrap();
    dir
}

async fn run(root: &Path, rendering: bool) -> Run {
    let headless = Arc::new(if rendering {
        HeadlessWindows::with_rendering()
    } else {
        HeadlessWindows::new()
    });
    let captures: Captures = Arc::default();
    let store = captures.clone();
    let project = Project::load(root).unwrap();
    let outcome = run_with(Arc::new(project.source_vfs()), "src/main.luau", move |builder| {
        builder.game("Fixture", ".").windows(headless).setup(move |lua| {
            let store = store.clone();
            let capture = lua.create_async_function(move |_, (window, label): (AnyUserData, String)| {
                let store = store.clone();
                async move {
                    let capture = Window::capture(&window).await?;
                    store.lock().unwrap().insert(label, capture);
                    Ok(())
                }
            })?;
            lua.globals().set("capture", capture)
        })
    })
    .await;
    let captures = std::mem::take(&mut *captures.lock().unwrap());
    if let Ok(directory) = std::env::var("LUV_CAPTURE_DIR") {
        for (label, capture) in &captures {
            image::RgbaImage::from_raw(capture.width, capture.height, capture.rgba.clone())
                .unwrap()
                .save(Path::new(&directory).join(format!("{label}.png")))
                .unwrap();
        }
    }
    Run { outcome, captures }
}

fn pixel(capture: &Capture, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * capture.width + x) * 4) as usize;
    capture.rgba[index..index + 4].try_into().unwrap()
}

#[track_caller]
fn assert_pixel(capture: &Capture, x: u32, y: u32, expected: [u8; 4]) {
    let actual = pixel(capture, x, y);
    let close = actual
        .iter()
        .zip(expected)
        .all(|(actual, expected)| (i32::from(*actual) - i32::from(expected)).abs() <= 4);
    assert!(close, "pixel ({x}, {y}) is {actual:?}, expected {expected:?}");
}

const HEADER: &str = r#"
local Window = import("Window")
local Asset = import("Asset")
local Shader = import("Shader")
local Bulk = import("Bulk")
local window = Window.new({ Title = "Scene", Size = udim.new(200, 100), BackgroundColor = color.new(0, 0, 0, 1) })
local Renderable = window:GetAPI("Renderable")
local function ready()
    window.AfterFrame:Wait()
end
"#;

fn script(body: &str) -> String {
    format!("{HEADER}\n{body}\nwindow:Close()\n")
}

#[tokio::test]
async fn renderables_expose_their_properties() {
    let dir = project(&script(
        r#"
local font = Asset.Load("block.ttf")
local picture = Asset.Load("quad.png")
local base = Renderable.new("Renderable", { Name = "Custom", ZIndex = 3 })
local box = Renderable.new("RenderableShape", {
    Shape = enum.ShapeType.Circle,
    Position = udim.new(10, 20),
    Size = udim.new(30, 40),
    Rotation = 45,
    Color = color.new(1, 0, 0, 1),
    StrokeThickness = 2,
})
local sprite = Renderable.new("RenderableImage", { Image = picture, OffsetSize = udim.new(1, 1), ResampleMode = enum.ResampleMode.Pixelated })
local label = Renderable.new("RenderableText", { Font = font, Text = "AB", TextSize = 100, Bold = true, TextXAlignment = enum.TextXAlignment.Center })
results = {
    classes = { base.ClassName, box.ClassName, sprite.ClassName, label.ClassName },
    name = base.Name,
    zIndex = base.ZIndex,
    vertexCount = base.VertexCount,
    sink = base.SinkUpdates,
    blend = tostring(base.BlendMode),
    shape = tostring(box.Shape),
    position = tostring(box.Position),
    size = tostring(box.Size),
    anchor = tostring(box.AnchorPoint),
    rotation = box.Rotation,
    color = box.Color:ToHex(),
    stroke = box.StrokeThickness,
    imageSize = tostring(sprite.ImageSize),
    spriteSize = tostring(sprite.Size),
    resample = tostring(sprite.ResampleMode),
    imageIsAsset = sprite.Image == picture,
    text = label.Text,
    bounds = tostring(label.TextBounds),
    bold = label.Bold,
    alignment = tostring(label.TextXAlignment),
    fontIsAsset = label.Font == font,
    typeName = typeof(label),
    count = #Renderable.GetRenderables(),
}
local function failure(action)
    local ok, message = pcall(action)
    assert(not ok, "expected a failure")
    return tostring(message)
end
errors = {
    missing = failure(function() return base.Position end),
    shapeFont = failure(function() box.Font = font end),
    badEnum = failure(function() box.Shape = enum.WindowType.FullScreen end),
    badClass = failure(function() Renderable.new("Sprite") end),
    noImage = failure(function() Renderable.new("RenderableImage") end),
    noFont = failure(function() Renderable.new("RenderableText", {}) end),
    notImage = failure(function() Renderable.new("RenderableImage", { Image = Asset.Load("notes.txt") }) end),
    notFont = failure(function() Renderable.new("RenderableText", { Font = picture }) end),
    readOnly = failure(function() sprite.ImageSize = udim.new(1, 1) end),
    badSize = failure(function() label.TextSize = 0 end),
}
countAfterFailures = #Renderable.GetRenderables()
box:Destroy()
destroyedRead = failure(function() return box.Position end)
countAfterDestroy = #Renderable.GetRenderables()
"#,
    ));
    let run = run(dir.path(), false).await;
    run.outcome.assert_clean();
    let results: Table = run.outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap();
    assert_eq!(
        results.get::<Vec<String>>("classes").unwrap(),
        ["Renderable", "RenderableShape", "RenderableImage", "RenderableText"]
    );
    assert_eq!(text("name"), "Custom");
    assert_eq!(results.get::<f64>("zIndex").unwrap(), 3.0);
    assert_eq!(results.get::<u32>("vertexCount").unwrap(), 6);
    assert!(results.get::<bool>("sink").unwrap());
    assert_eq!(text("blend"), "enum.BlendMode.Alpha");
    assert_eq!(text("shape"), "enum.ShapeType.Circle");
    assert_eq!(text("position"), "UDim(10, 20, 0)");
    assert_eq!(text("size"), "UDim(30, 40, 0)");
    assert_eq!(text("anchor"), "UDim(0.5, 0.5, 0)");
    assert_eq!(results.get::<f64>("rotation").unwrap(), 45.0);
    assert_eq!(text("color"), "#ff0000");
    assert_eq!(results.get::<f64>("stroke").unwrap(), 2.0);
    assert_eq!(text("imageSize"), "UDim(2, 2, 0)");
    assert_eq!(text("spriteSize"), "UDim(2, 2, 0)");
    assert_eq!(text("resample"), "enum.ResampleMode.Pixelated");
    assert_eq!(text("text"), "AB");
    assert_eq!(text("bounds"), "UDim(128.3333, 100, 0)");
    assert_eq!(text("alignment"), "enum.TextXAlignment.Center");
    assert_eq!(text("typeName"), "Renderable");
    for key in ["imageIsAsset", "bold", "fontIsAsset"] {
        assert!(results.get::<bool>(key).unwrap(), "{key}");
    }
    assert_eq!(results.get::<i64>("count").unwrap(), 4);

    let errors: Table = run.outcome.global("errors");
    let expected = [
        ("missing", "Position is not a valid member of Renderable 'Custom'"),
        ("shapeFont", "Font is not a valid member of RenderableShape"),
        ("badEnum", "expected an enum.ShapeType item, got enum.WindowType.FullScreen"),
        ("badClass", "'Sprite' is not a renderable class"),
        ("noImage", "needs an Image asset"),
        ("noFont", "needs a Font asset"),
        ("notImage", "is not an image the engine can draw"),
        ("notFont", "is not a font file"),
        ("readOnly", "ImageSize"),
        ("badSize", "TextSize must be a number greater than 0"),
    ];
    for (key, fragment) in expected {
        let message: String = errors.get(key).unwrap();
        assert!(message.contains(fragment), "{key}: {message:?} should contain {fragment:?}");
    }
    assert_eq!(run.outcome.global::<i64>("countAfterFailures"), 4);
    assert!(run.outcome.global::<String>("destroyedRead").contains("has been destroyed"));
    assert_eq!(run.outcome.global::<i64>("countAfterDestroy"), 3);
}

#[tokio::test]
async fn bulk_updates_move_many_renderables_and_text_wraps() {
    let dir = project(&script(
        r#"
local font = Asset.Load("block.ttf")
local boxes = {}
local updates = {}
for index = 1, 50 do
    local box = Renderable.new("RenderableShape")
    boxes[index] = box
    updates[box] = { Position = udim.new(index, index * 2), Color = color.fromHSV(index / 50, 1, 1) }
end
Bulk.BulkUpdate(updates)
local label = Renderable.new("RenderableText", { Font = font, Text = "AB AB AB", TextSize = 10, TextWrapped = true, Size = udim.new(20, 0) })
results = {
    last = tostring(boxes[50].Position),
    first = tostring(boxes[1].Position),
    wrapped = tostring(label.TextBounds),
}
label.TextWrapped = false
results.unwrapped = tostring(label.TextBounds)
local ok, message = pcall(Bulk.BulkUpdate, { [boxes[1]] = { Font = font } })
results.bulkError = tostring(message)
"#,
    ));
    let run = run(dir.path(), false).await;
    run.outcome.assert_clean();
    let results: Table = run.outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap();
    assert_eq!(text("last"), "UDim(50, 100, 0)");
    assert_eq!(text("first"), "UDim(1, 2, 0)");
    assert_eq!(text("wrapped"), "UDim(12, 30, 0)");
    assert_eq!(text("unwrapped"), "UDim(46, 10, 0)");
    assert!(text("bulkError").contains("Font is not a valid member of RenderableShape"), "{}", text("bulkError"));
}

#[tokio::test]
async fn parallel_blocks_render_in_their_own_windows() {
    if !gpu() {
        return;
    }
    let dir = project(
        r#"
local Messenger = import("Messenger")
Messenger:Subscribe("Rendered", function(hits, thread)
    rendered = { hits = hits, thread = thread }
end)

EnterParallel()
local Window = import("Window")
local window = Window.new({ Title = "Worker", Size = udim.new(100, 100) })
local Renderable = window:GetAPI("Renderable")
Renderable.new("RenderableShape", { Name = "Worker Box", Position = udim.new(50, 50), Size = udim.new(20, 20), Color = color.new(0, 1, 0, 1) })
window.AfterFrame:Wait()
capture(window, "worker")
local hits = Renderable.QueryPoint(udim.new(50, 50))
Messenger:Fire("Rendered", #hits, threadName())
window:Close()
ExitParallel()
"#,
    );
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    let rendered: Table = run.outcome.global("rendered");
    assert_eq!(rendered.get::<i64>("hits").unwrap(), 1);
    assert_eq!(rendered.get::<String>("thread").unwrap(), "parallel block #1 of src/main.luau");
    let worker = &run.captures["worker"];
    assert_pixel(worker, 50, 50, [0, 255, 0, 255]);
    assert_pixel(worker, 5, 5, [0, 0, 0, 255]);
}

#[tokio::test]
async fn shapes_render_in_their_windows() {
    if !gpu() {
        return;
    }
    let dir = project(&script(
        r#"
Renderable.new("RenderableShape", { Position = udim.new(50, 50), Size = udim.new(60, 60), Color = color.new(1, 0, 0, 1) })
Renderable.new("RenderableShape", { Shape = enum.ShapeType.Circle, Position = udim.new(150, 50), Size = udim.new(60, 60), Color = color.new(0, 1, 0, 1) })
local top = Renderable.new("RenderableShape", { Position = udim.new(80, 50), Size = udim.new(20, 20), Color = color.new(0, 0, 1, 1), ZIndex = 5 })
ready()
capture(window, "shapes")
top.ZIndex = -5
top.Rotation = 45
window.BackgroundColor = color.new(1, 1, 1, 1)
capture(window, "reordered")
"#,
    ));
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    let shapes = &run.captures["shapes"];
    assert_eq!((shapes.width, shapes.height), (200, 100));
    assert_pixel(shapes, 5, 5, [0, 0, 0, 255]);
    assert_pixel(shapes, 40, 50, [255, 0, 0, 255]);
    assert_pixel(shapes, 150, 50, [0, 255, 0, 255]);
    assert_pixel(shapes, 124, 24, [0, 0, 0, 255]);
    assert_pixel(shapes, 80, 50, [0, 0, 255, 255]);
    let reordered = &run.captures["reordered"];
    assert_pixel(reordered, 5, 5, [255, 255, 255, 255]);
    assert_pixel(reordered, 75, 50, [255, 0, 0, 255]);
    assert_pixel(reordered, 85, 50, [0, 0, 255, 255]);
}

#[tokio::test]
async fn custom_outlines_draw_their_own_shape() {
    if !gpu() {
        return;
    }
    let dir = project(&script(
        r#"
Renderable.new("RenderableShape", {
    Position = udim.new(50, 50),
    Size = udim.new(60, 60),
    Color = color.new(1, 0, 0, 1),
    Outline = {
        udim.new(-0.5, -0.5),
        udim.new(0, -0.5),
        udim.new(0, 0),
        udim.new(0.5, 0),
        udim.new(0.5, 0.5),
        udim.new(-0.5, 0.5),
    },
})
ready()
capture(window, "ell")
"#,
    ));
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    let ell = &run.captures["ell"];
    assert_pixel(ell, 35, 35, [255, 0, 0, 255]);
    assert_pixel(ell, 35, 65, [255, 0, 0, 255]);
    assert_pixel(ell, 65, 65, [255, 0, 0, 255]);
    assert_pixel(ell, 65, 35, [0, 0, 0, 255]);
    assert_pixel(ell, 5, 5, [0, 0, 0, 255]);
}

const ALPHA_SCENE: &str = r#"
local Asset = import("Asset")
local sheet = Asset.Load("sheet.png")
local sprite = Renderable.new("RenderableImage", {
    Name = "Sprite",
    Image = sheet,
    Position = udim.new(100, 100),
    Size = udim.new(40, 40),
    OffsetPosition = udim.new(0, 0),
    OffsetSize = udim.new(2, 2),
})
local function names(list)
    local found = {}
    for _, item in list do
        table.insert(found, item.Name)
    end
    return table.concat(found, ",")
end
local function distance(hit)
    if not hit then
        return -1
    end
    return math.round(hit.Distance * 100) / 100
end
results = {}
results.boxSolid = names(Renderable.QueryPoint(udim.new(90, 90)))
results.boxClear = names(Renderable.QueryPoint(udim.new(110, 90)))
sprite.HitThreshold = 0.5
results.threshold = sprite.HitThreshold
results.solid = names(Renderable.QueryPoint(udim.new(90, 90)))
results.clear = names(Renderable.QueryPoint(udim.new(110, 90)))
results.below = names(Renderable.QueryPoint(udim.new(90, 110)))
results.rayIntoSolid = distance(Renderable.Raycast(udim.new(60, 90), udim.new(100, 0)))
results.rayThroughClear = distance(Renderable.Raycast(udim.new(60, 110), udim.new(100, 0)))
sprite.OffsetPosition = udim.new(2, 0)
results.secondCell = names(Renderable.QueryPoint(udim.new(110, 90)))
results.secondBelow = names(Renderable.QueryPoint(udim.new(90, 110)))
sprite.HitThreshold = nil
results.offAgain = names(Renderable.QueryPoint(udim.new(90, 110)))
"#;

fn assert_alpha_results(results: &Table) {
    let text = |key: &str| results.get::<String>(key).unwrap();
    let number = |key: &str| results.get::<f64>(key).unwrap();
    assert_eq!(text("boxSolid"), "Sprite");
    assert_eq!(text("boxClear"), "Sprite", "without a threshold the whole box is hit");
    assert!((number("threshold") - 0.5).abs() < 1e-9);
    assert_eq!(text("solid"), "Sprite");
    assert_eq!(text("clear"), "", "a clear pixel of the sprite must not be hit");
    assert_eq!(text("below"), "");
    assert!((number("rayIntoSolid") - 20.0).abs() < 1.0, "rayIntoSolid {}", number("rayIntoSolid"));
    assert!((number("rayThroughClear") + 1.0).abs() < 1e-9, "a ray over clear pixels must miss");
    assert_eq!(text("secondCell"), "Sprite", "the second cell of the sheet is solid");
    assert_eq!(text("secondBelow"), "Sprite");
    assert_eq!(text("offAgain"), "Sprite", "clearing HitThreshold goes back to the whole box");
}

#[tokio::test]
async fn image_queries_follow_clear_pixels_on_the_cpu() {
    let dir = project(&script(ALPHA_SCENE));
    let run = run(dir.path(), false).await;
    run.outcome.assert_clean();
    assert_alpha_results(&run.outcome.global("results"));
}

#[tokio::test]
async fn image_queries_follow_clear_pixels_on_the_gpu() {
    if !gpu() {
        return;
    }
    let dir = project(&script(&format!("ready()
{ALPHA_SCENE}")));
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    assert_alpha_results(&run.outcome.global("results"));
}

const FORMATS: [&str; 12] = [
    "quad.png", "quad.jpg", "quad.gif", "quad.webp", "quad.bmp", "quad.tga", "quad.tiff", "quad.qoi", "quad.ico",
    "quad.ppm", "quad.ff", "quad.svg",
];

fn write_formats(assets: &Path) {
    let mut quad = image::RgbaImage::new(2, 2);
    quad.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    quad.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
    quad.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
    quad.put_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
    let picture = image::DynamicImage::ImageRgba8(quad.clone());
    for name in ["quad.gif", "quad.webp", "quad.bmp", "quad.tga", "quad.tiff", "quad.qoi", "quad.ico"] {
        quad.save(assets.join(name)).unwrap();
    }
    picture.to_rgb8().save(assets.join("quad.jpg")).unwrap();
    picture.to_rgb8().save(assets.join("quad.ppm")).unwrap();
    picture.to_rgba16().save(assets.join("quad.ff")).unwrap();
    std::fs::write(
        assets.join("quad.svg"),
        r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2">
<rect x="0" y="0" width="1" height="1" fill="#ff0000"/>
<rect x="1" y="0" width="1" height="1" fill="#00ff00"/>
<rect x="0" y="1" width="1" height="1" fill="#0000ff"/>
<rect x="1" y="1" width="1" height="1" fill="#ffffff"/>
</svg>"##,
    )
    .unwrap();
}

#[tokio::test]
async fn images_load_from_many_formats() {
    let rendering = gpu();
    let draw = if rendering { "ready()\ncapture(window, \"formats\")" } else { "" };
    let dir = project(&script(&format!(
        r#"
local names = {{ "{}" }}
sizes = {{}}
for index, name in names do
    local ok, result = pcall(function()
        return Renderable.new("RenderableImage", {{
            Image = Asset.Load(name),
            Position = udim.new((index - 1) * 16, 0),
            Size = udim.new(16, 16),
            AnchorPoint = udim.new(0, 0),
            ResampleMode = enum.ResampleMode.Pixelated,
        }})
    end)
    sizes[name] = if ok then tostring(result.ImageSize) else tostring(result)
end
{draw}
"#,
        FORMATS.join("\", \"")
    )));
    write_formats(&dir.path().join("assets"));
    let run = run(dir.path(), rendering).await;
    run.outcome.assert_clean();
    let sizes: Table = run.outcome.global("sizes");
    for name in FORMATS {
        assert_eq!(sizes.get::<String>(name).unwrap(), "UDim(2, 2, 0)", "{name}");
    }
    if !rendering {
        return;
    }
    let formats = &run.captures["formats"];
    for (index, name) in FORMATS.iter().enumerate() {
        if *name == "quad.jpg" {
            continue;
        }
        let x = index as u32 * 16;
        for (offset, expected) in [
            ((4, 4), [255, 0, 0, 255]),
            ((12, 4), [0, 255, 0, 255]),
            ((4, 12), [0, 0, 255, 255]),
            ((12, 12), [255, 255, 255, 255]),
        ] {
            let actual = pixel(formats, x + offset.0, offset.1);
            let close = actual
                .iter()
                .zip(expected)
                .all(|(actual, expected)| (i32::from(*actual) - expected).abs() <= 4);
            assert!(close, "{name} at {offset:?} is {actual:?}, expected {expected:?}");
        }
    }
}

#[tokio::test]
async fn images_render_with_tints_offsets_and_flips() {
    if !gpu() {
        return;
    }
    let dir = project(&script(
        r#"
local picture = Asset.Load("quad.png")
local function image(config)
    config.Image = picture
    config.AnchorPoint = udim.new(0, 0)
    config.ResampleMode = enum.ResampleMode.Pixelated
    return Renderable.new("RenderableImage", config)
end
image({ Position = udim.new(0, 0), Size = udim.new(100, 100) })
image({ Position = udim.new(100, 0), Size = udim.new(50, 50), OffsetPosition = udim.new(1, 0), OffsetSize = udim.new(1, 1) })
image({ Position = udim.new(150, 0), Size = udim.new(50, 50), FlipX = true })
image({ Position = udim.new(100, 50), Size = udim.new(50, 50), Color = color.new(0.5, 0.5, 0.5, 1) })
ready()
capture(window, "images")
"#,
    ));
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    let images = &run.captures["images"];
    assert_pixel(images, 25, 25, [255, 0, 0, 255]);
    assert_pixel(images, 75, 25, [0, 255, 0, 255]);
    assert_pixel(images, 25, 75, [0, 0, 255, 255]);
    assert_pixel(images, 75, 75, [255, 255, 255, 255]);
    assert_pixel(images, 110, 10, [0, 255, 0, 255]);
    assert_pixel(images, 140, 40, [0, 255, 0, 255]);
    assert_pixel(images, 160, 10, [0, 255, 0, 255]);
    assert_pixel(images, 190, 10, [255, 0, 0, 255]);
    assert_pixel(images, 110, 60, [128, 0, 0, 255]);
    assert_pixel(images, 140, 90, [128, 128, 128, 255]);
}

#[tokio::test]
async fn text_renders_glyphs_backgrounds_and_decorations() {
    if !gpu() {
        return;
    }
    let dir = project(&script(
        r#"
local font = Asset.Load("block.ttf")
local label = Renderable.new("RenderableText", {
    Font = font,
    Text = "AB",
    TextSize = 50,
    Position = udim.new(10, 10),
    AnchorPoint = udim.new(0, 0),
    Color = color.new(1, 1, 0, 1),
    BackgroundColor = color.new(0, 0, 1, 1),
    Underline = true,
})
bounds = tostring(label.TextBounds)
ready()
capture(window, "text")
label.Text = "A B"
label.TextXAlignment = enum.TextXAlignment.Right
label.Size = udim.new(180, 0)
capture(window, "aligned")
"#,
    ));
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    assert_eq!(run.outcome.global::<String>("bounds"), "UDim(60, 50, 0)");
    let text = &run.captures["text"];
    assert_pixel(text, 5, 5, [0, 0, 0, 255]);
    assert_pixel(text, 25, 30, [255, 255, 0, 255]);
    assert_pixel(text, 40, 30, [0, 0, 255, 255]);
    assert_pixel(text, 55, 30, [255, 255, 0, 255]);
    assert_pixel(text, 30, 56, [255, 255, 0, 255]);
    assert_pixel(text, 75, 30, [0, 0, 0, 255]);
    let aligned = &run.captures["aligned"];
    assert_pixel(aligned, 25, 30, [0, 0, 255, 255]);
    assert_pixel(aligned, 180, 30, [255, 255, 0, 255]);
}

const CUSTOM_SHADER: &str = r#"
struct Params {
    tint: vec4<f32>,
    origin: vec2<f32>,
    extent: vec2<f32>,
}
@group(1) @binding(0) var<uniform> params: Params;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((0x32u >> index) & 1u), f32((0x2cu >> index) & 1u));
    let world = params.origin + corner * params.extent;
    return vec4<f32>(world.x / frame.resolution.x * 2.0 - 1.0, 1.0 - world.y / frame.resolution.y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return params.tint;
}
"#;

const STRIPES_SHADER: &str = r#"
@fragment
fn stripes(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.local.x < 0.0 {
        return vec4<f32>(1.0, 1.0, 0.0, 1.0);
    }
    return vec4<f32>(0.0, 1.0, 1.0, 1.0);
}
"#;

const LENS_SHADER: &str = r#"
struct Hide {
    lens: u32,
}
@group(1) @binding(0) var<uniform> hide: Hide;

@fragment
fn hidden(in: VertexOutput) -> @location(0) vec4<f32> {
    if object_contains(hide.lens, world_position(in.position)) {
        discard;
    }
    return in.color;
}
"#;

const INVERT_SHADER: &str = r#"
@fragment
fn invert(in: VertexOutput) -> @location(0) vec4<f32> {
    let behind = textureLoad(backdrop, vec2<i32>(in.position.xy), 0);
    return vec4<f32>(1.0 - behind.rgb, 1.0);
}
"#;

fn shader_script(body: &str) -> String {
    script(&format!(
        "local CUSTOM = [==[{CUSTOM_SHADER}]==]\nlocal STRIPES = [==[{STRIPES_SHADER}]==]\nlocal LENS = [==[{LENS_SHADER}]==]\nlocal INVERT = [==[{INVERT_SHADER}]==]\n{body}"
    ))
}

#[tokio::test]
async fn custom_shaders_draw_renderables() {
    if !gpu() {
        return;
    }
    let dir = project(&shader_script(
        r#"
local custom = Shader.Compile(Shader.Combine({ Shader.Prelude, CUSTOM }, { Name = "custom" }))
local stripes = Shader.Compile(Shader.Combine({ Shader.Prelude, STRIPES }, { Name = "stripes" }))
local base = Renderable.new("Renderable", { Shaders = { custom } })
base:WriteShaderData(custom, { tint = color.new(1, 0, 1, 1), origin = udim.new(10, 10), extent = udim.new(40, 30) })
local striped = Renderable.new("RenderableShape", { Position = udim.new(150, 50), Size = udim.new(40, 40), Shaders = { stripes } })
readTint = base:ReadShaderData(custom, "tint"):ToHex()
readOrigin = tostring(base:ReadShaderData(custom, "origin"))
ready()
capture(window, "custom")
striped:RemoveShader(stripes)
base.SinkUpdates = false
capture(window, "restored")
"#,
    ));
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    assert_eq!(run.outcome.global::<String>("readTint"), "#ff00ff");
    assert_eq!(run.outcome.global::<String>("readOrigin"), "UDim(10, 10, 0)");
    let custom = &run.captures["custom"];
    assert_pixel(custom, 30, 25, [255, 0, 255, 255]);
    assert_pixel(custom, 60, 25, [0, 0, 0, 255]);
    assert_pixel(custom, 140, 50, [255, 255, 0, 255]);
    assert_pixel(custom, 160, 50, [0, 255, 255, 255]);
    let restored = &run.captures["restored"];
    assert_pixel(restored, 30, 25, [0, 0, 0, 255]);
    assert_pixel(restored, 140, 50, [255, 255, 255, 255]);
}

#[tokio::test]
async fn lens_shaders_hide_parts_of_other_objects() {
    if !gpu() {
        return;
    }
    let dir = project(&shader_script(
        r#"
local lensShader = Shader.Compile(Shader.Prelude .. LENS)
local wall = Renderable.new("RenderableShape", { Position = udim.new(100, 50), Size = udim.new(200, 100), Color = color.new(1, 0, 0, 1) })
local floor = Renderable.new("RenderableShape", { Position = udim.new(100, 90), Size = udim.new(200, 20), Color = color.new(0, 1, 0, 1), ZIndex = 1 })
local lens = Renderable.new("RenderableShape", { Shape = enum.ShapeType.Circle, Position = udim.new(100, 50), Size = udim.new(40, 40), Color = color.new(1, 1, 1, 0) })
wall:LoadShader(lensShader)
floor:LoadShader(lensShader)
Bulk.BulkWriteShaderData(lensShader, {
    [wall] = { lens = lens },
    [floor] = { lens = lens },
})
readLens = wall:ReadShaderData(lensShader, "lens") == lens
ready()
capture(window, "lens")
lens.Position = udim.new(40, 80)
capture(window, "moved")
lens:Destroy()
capture(window, "gone")
"#,
    ));
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    assert!(run.outcome.global::<bool>("readLens"));
    let lens = &run.captures["lens"];
    assert_pixel(lens, 100, 50, [0, 0, 0, 255]);
    assert_pixel(lens, 20, 20, [255, 0, 0, 255]);
    assert_pixel(lens, 160, 90, [0, 255, 0, 255]);
    let moved = &run.captures["moved"];
    assert_pixel(moved, 100, 50, [255, 0, 0, 255]);
    assert_pixel(moved, 40, 72, [0, 0, 0, 255]);
    assert_pixel(moved, 40, 90, [0, 0, 0, 255]);
    let gone = &run.captures["gone"];
    assert_pixel(gone, 40, 72, [255, 0, 0, 255]);
    assert_pixel(gone, 40, 90, [0, 255, 0, 255]);
}

#[tokio::test]
async fn backdrop_shaders_see_what_is_behind_them() {
    if !gpu() {
        return;
    }
    let dir = project(&shader_script(
        r#"
local invert = Shader.Compile(Shader.Prelude .. INVERT)
Renderable.new("RenderableShape", { Position = udim.new(50, 50), Size = udim.new(100, 100), Color = color.new(1, 0, 0, 1) })
Renderable.new("RenderableShape", { Position = udim.new(100, 50), Size = udim.new(100, 100), ZIndex = 1, Shaders = { invert } })
ready()
capture(window, "backdrop")
"#,
    ));
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    let backdrop = &run.captures["backdrop"];
    assert_pixel(backdrop, 25, 50, [255, 0, 0, 255]);
    assert_pixel(backdrop, 75, 50, [0, 255, 255, 255]);
    assert_pixel(backdrop, 125, 50, [255, 255, 255, 255]);
    assert_pixel(backdrop, 175, 50, [0, 0, 0, 255]);
}

const DATA_SHADER: &str = r#"
struct Data {
    tint: vec4<f32>,
    offset: vec2<f32>,
    speed: f32,
    enabled: u32,
    lens: u32,
    signed: i32,
    transform: mat2x2<f32>,
}
struct Particle {
    position: vec2<f32>,
    life: f32,
    padding: f32,
}
struct Extra {
    codes: array<u32, 6>,
    particles: array<Particle>,
}
@group(1) @binding(0) var<uniform> data: Data;
@group(1) @binding(1) var<storage, read> extra: Extra;
@group(2) @binding(0) var sprite: texture_2d<f32>;
@group(2) @binding(1) var sprite_sampler: sampler;

@fragment
fn main() -> @location(0) vec4<f32> {
    return data.tint + vec4<f32>(extra.particles[0].life) + textureSample(sprite, sprite_sampler, vec2<f32>(0.5));
}
"#;

#[tokio::test]
async fn shader_data_holds_every_kind_of_value() {
    let dir = project(&script(&format!(
        r#"
local DATA = [==[{DATA_SHADER}]==]
local shader = Shader.Compile(DATA)
local other = Shader.Compile(DATA)
local picture = Asset.Load("quad.png")
local target = Renderable.new("RenderableShape", {{ Name = "Target" }})
local box = Renderable.new("RenderableShape", {{ Shaders = {{ shader }} }})
box:WriteShaderData(shader, "tint", color.new(1, 0, 0, 1))
box:WriteShaderData(shader, {{
    offset = udim.new(3, 4),
    speed = 2.5,
    enabled = true,
    lens = target,
    signed = -7,
    transform = {{ 1, 2, 3, 4 }},
    codes = "HELLO",
    particles = {{ {{ position = udim.new(1, 2), life = 0.5 }}, {{ position = vector.create(3, 4, 0), life = 1 }} }},
    sprite = picture,
    sprite_sampler = enum.ResampleMode.Pixelated,
}})
box:WriteShaderData(shader, "data.signed", -8)
local particles = box:ReadShaderData(shader, "particles")
results = {{
    tint = box:ReadShaderData(shader, "tint"):ToHex(),
    offset = tostring(box:ReadShaderData(shader, "offset")),
    speed = box:ReadShaderData(shader, "speed"),
    enabled = box:ReadShaderData(shader, "enabled"),
    lens = box:ReadShaderData(shader, "lens") == target,
    signed = box:ReadShaderData(shader, "signed"),
    transform = box:ReadShaderData(shader, "transform"),
    codes = box:ReadShaderData(shader, "codes"),
    particleCount = #particles,
    secondPosition = tostring(particles[2].position),
    secondLife = particles[2].life,
    sprite = box:ReadShaderData(shader, "sprite") == picture,
    sampler = tostring(box:ReadShaderData(shader, "sprite_sampler")),
    loaded = box:HasShader(shader),
    shaderCount = #box:GetShaders(),
}}
local function failure(action)
    local ok, message = pcall(action)
    assert(not ok, "expected a failure")
    return tostring(message)
end
errors = {{
    unknown = failure(function() box:WriteShaderData(shader, "missing", 1) end),
    notNumber = failure(function() box:WriteShaderData(shader, "speed", "fast") end),
    notWhole = failure(function() box:WriteShaderData(shader, "enabled", 1.5) end),
    tooMany = failure(function() box:WriteShaderData(shader, "codes", {{ 1, 2, 3, 4, 5, 6, 7 }}) end),
    notLoaded = failure(function() box:WriteShaderData(other, "speed", 1) end),
    renderableInFloat = failure(function() box:WriteShaderData(shader, "speed", target) end),
    texture = failure(function() box:WriteShaderData(shader, "sprite", 5) end),
    member = failure(function() box:WriteShaderData(shader, "data.nope", 1) end),
}}
target:Destroy()
lensAfterDestroy = box:ReadShaderData(shader, "lens")
removed = box:RemoveShader(shader)
removedAgain = box:RemoveShader(shader)
"#
    )));
    let run = run(dir.path(), false).await;
    run.outcome.assert_clean();
    let results: Table = run.outcome.global("results");
    assert_eq!(results.get::<String>("tint").unwrap(), "#ff0000");
    assert_eq!(results.get::<String>("offset").unwrap(), "UDim(3, 4, 0)");
    assert_eq!(results.get::<f64>("speed").unwrap(), 2.5);
    assert_eq!(results.get::<i64>("enabled").unwrap(), 1);
    assert!(results.get::<bool>("lens").unwrap());
    assert_eq!(results.get::<i64>("signed").unwrap(), -8);
    assert_eq!(results.get::<Vec<f64>>("transform").unwrap(), [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(results.get::<Vec<i64>>("codes").unwrap(), [72, 69, 76, 76, 79, 0]);
    assert_eq!(results.get::<i64>("particleCount").unwrap(), 2);
    assert_eq!(results.get::<String>("secondPosition").unwrap(), "UDim(3, 4, 0)");
    assert_eq!(results.get::<f64>("secondLife").unwrap(), 1.0);
    assert!(results.get::<bool>("sprite").unwrap());
    assert_eq!(results.get::<String>("sampler").unwrap(), "enum.ResampleMode.Pixelated");
    assert!(results.get::<bool>("loaded").unwrap());
    assert_eq!(results.get::<i64>("shaderCount").unwrap(), 1);

    let errors: Table = run.outcome.global("errors");
    let expected = [
        ("unknown", "has no data named 'missing'"),
        ("notNumber", "expected a number, got string"),
        ("notWhole", "expected a whole number"),
        ("tooMany", "the array holds 6 items but 7 were given"),
        ("notLoaded", "the shader is not loaded"),
        ("renderableInFloat", "can only be written to a u32 or i32"),
        ("texture", "takes an image Asset"),
        ("member", "has no member 'nope'"),
    ];
    for (key, fragment) in expected {
        let message: String = errors.get(key).unwrap();
        assert!(message.contains(fragment), "{key}: {message:?} should contain {fragment:?}");
    }
    assert_eq!(run.outcome.global::<Option<bool>>("lensAfterDestroy"), None);
    assert!(run.outcome.global::<bool>("removed"));
    assert!(!run.outcome.global::<bool>("removedAgain"));
}

#[tokio::test]
async fn destroyed_renderables_are_freed_from_memory() {
    let dir = project(&script(
        r#"
local weak = setmetatable({}, { __mode = "v" })
local function make()
    local shader = Shader.Compile(Shader.Prelude .. [[
struct Hide { lens: u32 }
@group(1) @binding(0) var<uniform> hide: Hide;
@fragment fn hidden(in: VertexOutput) -> @location(0) vec4<f32> { return in.color; }
]])
    local picture = Asset.Load("quad.png")
    local font = Asset.Load("block.ttf")
    local sprite = Renderable.new("RenderableImage", { Image = picture, Shaders = { shader } })
    local label = Renderable.new("RenderableText", { Font = font, Text = "HI" })
    sprite:WriteShaderData(shader, "lens", label)
    weak.sprite, weak.label, weak.shader, weak.picture, weak.font = sprite, label, shader, picture, font
    sprite:Destroy()
    label:Destroy()
end
make()
collectgarbage("collect")
freed = {}
for _, key in { "sprite", "label", "shader", "picture", "font" } do
    freed[key] = weak[key] == nil
end
countAfterDestroy = #Renderable.GetRenderables()

local function keep()
    weak.owned = Renderable.new("RenderableShape")
end
keep()
collectgarbage("collect")
ownedWhileOpen = weak.owned ~= nil
window:Close()
collectgarbage("collect")
ownedAfterClose = weak.owned == nil
"#,
    ));
    let run = run(dir.path(), false).await;
    run.outcome.assert_clean();
    let freed: Table = run.outcome.global("freed");
    for key in ["sprite", "label", "shader", "picture", "font"] {
        assert!(freed.get::<bool>(key).unwrap(), "{key} was not freed");
    }
    assert_eq!(run.outcome.global::<i64>("countAfterDestroy"), 0);
    assert!(run.outcome.global::<bool>("ownedWhileOpen"));
    assert!(run.outcome.global::<bool>("ownedAfterClose"));
}

const QUERY_SCENE: &str = r#"
local function shape(name, config)
    config.Name = name
    return Renderable.new("RenderableShape", config)
end
local a = shape("A", { Position = udim.new(50, 50), Size = udim.new(40, 40) })
local b = shape("B", { Shape = enum.ShapeType.Circle, Position = udim.new(150, 50), Size = udim.new(40, 40), ZIndex = 2 })
local c = shape("C", { Shape = enum.ShapeType.Triangle, Position = udim.new(60, 50), Size = udim.new(40, 40), ZIndex = 1 })
shape("Hidden", { Position = udim.new(50, 50), Size = udim.new(10, 10), SinkUpdates = false })
Renderable.new("Renderable", { Name = "Custom" })
local function names(list)
    local found = {}
    for _, item in list do
        table.insert(found, item.Name)
    end
    return table.concat(found, ",")
end
local hit = Renderable.Raycast(udim.new(0, 50), udim.new(200, 0))
local all = Renderable.RaycastAll(udim.new(0, 50), udim.new(200, 0))
local distances = {}
for _, result in all do
    table.insert(distances, `{result.Renderable.Name}:{math.round(result.Distance * 100) / 100}`)
end
results = {
    point = names(Renderable.QueryPoint(udim.new(55, 55))),
    corner = names(Renderable.QueryPoint(udim.new(32, 32))),
    circleMiss = names(Renderable.QueryPoint(udim.new(133, 33))),
    area = names(Renderable.QueryArea(udim.new(150, 50), udim.new(10, 10))),
    gap = names(Renderable.QueryArea(udim.new(100, 50), udim.new(40, 10))),
    rotated = names(Renderable.QueryArea(udim.new(100, 50), udim.new(10, 200), 90)),
    radius = names(Renderable.QueryRadius(udim.new(100, 50), 29)),
    wider = names(Renderable.QueryRadius(udim.new(100, 50), 31)),
    first = hit.Renderable.Name,
    position = tostring(hit.Position),
    normal = tostring(hit.Normal),
    distance = hit.Distance,
    all = table.concat(distances, ","),
    inside = Renderable.Raycast(udim.new(40, 45), udim.new(160, 0)).Renderable.Name,
    excluded = Renderable.Raycast(udim.new(0, 50), udim.new(200, 0), { Exclude = { a } }).Renderable.Name,
    included = names(Renderable.QueryPoint(udim.new(55, 55), { Include = { a } })),
    missed = Renderable.Raycast(udim.new(0, 5), udim.new(200, 0)) == nil,
}
"#;

fn assert_query_results(results: &Table) {
    let text = |key: &str| results.get::<String>(key).unwrap();
    assert_eq!(text("point"), "C,A");
    assert_eq!(text("corner"), "A");
    assert_eq!(text("circleMiss"), "");
    assert_eq!(text("area"), "B");
    assert_eq!(text("gap"), "");
    assert_eq!(text("rotated"), "B,C,A");
    assert_eq!(text("radius"), "C");
    assert_eq!(text("wider"), "B,C,A");
    assert_eq!(text("first"), "A");
    assert_eq!(text("position"), "UDim(30, 50, 0)");
    assert_eq!(text("normal"), "UDim(-1, 0, 0)");
    assert!((results.get::<f64>("distance").unwrap() - 30.0).abs() < 1e-3);
    assert_eq!(text("all"), "A:30,C:50,B:130");
    assert_eq!(text("inside"), "C");
    assert_eq!(text("excluded"), "C");
    assert_eq!(text("included"), "A");
    assert!(results.get::<bool>("missed").unwrap());
}

#[tokio::test]
async fn queries_find_renderables_on_the_cpu() {
    let dir = project(&script(QUERY_SCENE));
    let run = run(dir.path(), false).await;
    run.outcome.assert_clean();
    assert_query_results(&run.outcome.global("results"));
}

#[tokio::test]
async fn queries_find_renderables_on_the_gpu() {
    if !gpu() {
        return;
    }
    let dir = project(&script(&format!("ready()\n{QUERY_SCENE}")));
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    assert_query_results(&run.outcome.global("results"));
}

const OUTLINE_SCENE: &str = r#"
local function point(x, y)
    return udim.new(x, y)
end
local shape = Renderable.new("RenderableShape", {
    Name = "Ell",
    Position = udim.new(100, 100),
    Size = udim.new(40, 40),
    Outline = {
        point(-0.5, -0.5),
        point(0, -0.5),
        point(0, 0),
        point(0.5, 0),
        point(0.5, 0.5),
        point(-0.5, 0.5),
    },
})
local function names(list)
    local found = {}
    for _, item in list do
        table.insert(found, item.Name)
    end
    return table.concat(found, ",")
end
local function distance(hit)
    if not hit then
        return -1
    end
    return math.round(hit.Distance * 100) / 100
end
local kept = {}
for _, entry in shape.Outline do
    table.insert(kept, tostring(entry))
end
results = {
    arm = names(Renderable.QueryPoint(udim.new(90, 90))),
    notch = names(Renderable.QueryPoint(udim.new(110, 90))),
    foot = names(Renderable.QueryPoint(udim.new(110, 110))),
    throughArm = distance(Renderable.Raycast(udim.new(60, 90), udim.new(100, 0))),
    pastNotch = distance(Renderable.Raycast(udim.new(110, 60), udim.new(0, 100))),
    intoArm = distance(Renderable.Raycast(udim.new(90, 60), udim.new(0, 100))),
    notchArea = names(Renderable.QueryArea(udim.new(112, 88), udim.new(8, 8))),
    footArea = names(Renderable.QueryArea(udim.new(112, 112), udim.new(8, 8))),
    notchRadius = names(Renderable.QueryRadius(udim.new(112, 88), 3)),
    points = #kept,
    first = kept[1],
}
"#;

fn assert_outline_results(results: &Table) {
    let text = |key: &str| results.get::<String>(key).unwrap();
    let number = |key: &str| results.get::<f64>(key).unwrap();
    assert_eq!(text("arm"), "Ell");
    assert_eq!(text("notch"), "", "the notch of a concave outline must not be hit");
    assert_eq!(text("foot"), "Ell");
    assert!((number("throughArm") - 20.0).abs() < 1e-3, "throughArm {}", number("throughArm"));
    assert!((number("pastNotch") - 40.0).abs() < 1e-3, "pastNotch {}", number("pastNotch"));
    assert!((number("intoArm") - 20.0).abs() < 1e-3, "intoArm {}", number("intoArm"));
    assert_eq!(text("notchArea"), "", "an area inside the notch must not overlap");
    assert_eq!(text("footArea"), "Ell");
    assert_eq!(text("notchRadius"), "");
    assert!((number("points") - 6.0).abs() < 1e-9);
    assert_eq!(text("first"), "UDim(-0.5, -0.5, 0)");
}

#[tokio::test]
async fn custom_outlines_query_as_drawn_on_the_cpu() {
    let dir = project(&script(OUTLINE_SCENE));
    let run = run(dir.path(), false).await;
    run.outcome.assert_clean();
    assert_outline_results(&run.outcome.global("results"));
}

#[tokio::test]
async fn custom_outlines_query_as_drawn_on_the_gpu() {
    if !gpu() {
        return;
    }
    let dir = project(&script(&format!("ready()
{OUTLINE_SCENE}")));
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    assert_outline_results(&run.outcome.global("results"));
}

const RANDOM_SCENE: &str = r#"
local seed = 12345
local function random()
    seed = (seed * 1103515245 + 12345) % 2147483648
    return seed / 2147483648
end
local shapes = enum.ShapeType:GetEnumItems()
for index = 1, 300 do
    Renderable.new("RenderableShape", {
        Name = tostring(index),
        Shape = shapes[math.floor(random() * #shapes) + 1],
        Position = udim.new(random() * 200, random() * 100),
        Size = udim.new(2 + random() * 30, 2 + random() * 30),
        AnchorPoint = udim.new(random(), random()),
        Rotation = random() * 360,
    })
end
local function names(list)
    local found = {}
    for _, item in list do
        table.insert(found, item.Name)
    end
    table.sort(found)
    return table.concat(found, ",")
end
results = {}
for probe = 1, 40 do
    local x, y = random() * 200, random() * 100
    table.insert(results, "p" .. names(Renderable.QueryPoint(udim.new(x, y))))
    table.insert(results, "a" .. names(Renderable.QueryArea(udim.new(x, y), udim.new(random() * 40, random() * 40), random() * 360)))
    table.insert(results, "r" .. names(Renderable.QueryRadius(udim.new(x, y), random() * 20)))
    local hit = Renderable.Raycast(udim.new(x, y), udim.new(random() * 200 - 100, random() * 200 - 100))
    table.insert(results, "c" .. (if hit then `{hit.Renderable.Name}@{math.round(hit.Distance)}` else "none"))
end
"#;

#[tokio::test]
async fn gpu_queries_agree_with_cpu_queries() {
    if !gpu() {
        return;
    }
    let cpu_dir = project(&script(RANDOM_SCENE));
    let gpu_dir = project(&script(&format!("ready()\n{RANDOM_SCENE}")));
    let cpu = run(cpu_dir.path(), false).await;
    let gpu = run(gpu_dir.path(), true).await;
    cpu.outcome.assert_clean();
    gpu.outcome.assert_clean();
    let cpu_results: Vec<String> = cpu.outcome.global("results");
    let gpu_results: Vec<String> = gpu.outcome.global("results");
    assert_eq!(cpu_results.len(), 160);
    let differences: Vec<_> = cpu_results
        .iter()
        .zip(&gpu_results)
        .filter(|(cpu, gpu)| cpu != gpu)
        .collect();
    assert!(differences.is_empty(), "{differences:#?}");
    assert!(cpu_results.iter().any(|result| result.len() > 1 && result.starts_with('p')));
}

#[tokio::test]
async fn render_errors_are_reported_to_the_script() {
    if !gpu() {
        return;
    }
    let dir = project(&script(
        r#"
local broken = Shader.Compile([[
struct Odd { value: f32 }
@group(1) @binding(0) var<uniform> odd: Odd;
@fragment fn main(@location(7) missing: vec4<f32>) -> @location(0) vec4<f32> { return missing * odd.value; }
]])
Renderable.new("RenderableShape", { Shaders = { broken } })
ready()
capture(window, "broken")
"#,
    ));
    let run = run(dir.path(), true).await;
    assert_eq!(run.outcome.errors.len(), 1, "{:#?}", run.outcome.errors);
    assert!(run.outcome.errors[0].contains("cannot draw with shader"), "{}", run.outcome.errors[0]);
}

const POST_INVERT: &str = r#"
@fragment
fn post_invert(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSampleLevel(image, image_sampler, in.uv, 0.0);
    return vec4<f32>(vec3<f32>(1.0) - color.rgb, 1.0);
}
"#;

const POST_TINT: &str = r#"
struct Tint {
    color: vec4<f32>,
}
@group(1) @binding(0) var<uniform> tint: Tint;

@fragment
fn post_tint(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSampleLevel(image, image_sampler, in.uv, 0.0);
    return vec4<f32>(color.rgb * tint.color.rgb, 1.0);
}
"#;

#[tokio::test]
async fn windows_run_post_process_shaders_in_order() {
    if !gpu() {
        return;
    }
    let dir = project(&script(&format!(
        r#"
local INVERT = [==[{POST_INVERT}]==]
local TINT = [==[{POST_TINT}]==]
Renderable.new("RenderableShape", {{ Position = udim.new(100, 50), Size = udim.new(40, 40), Color = color.new(1, 0, 0, 1) }})
local invertShader = Shader.Compile(Shader.Prelude .. INVERT)
local tintShader = Shader.Compile(Shader.Prelude .. TINT)
local invert = window:AddPostProcess(invertShader)
ready()
capture(window, "inverted")
local tint = window:AddPostProcess(tintShader)
tint:WriteShaderData(tintShader, "color", color.new(1, 0.5, 0, 1))
ready()
capture(window, "tinted")
tint.Order = invert.Order - 1
ready()
capture(window, "reordered")
invert.Enabled = false
ready()
capture(window, "disabled")
results = {{
    count = #window:GetPostProcesses(),
    first = window:GetPostProcesses()[1] == tint,
    class = invert.ClassName,
    renderables = #Renderable.GetRenderables(),
    noPosition = not pcall(function()
        return invert.Position
    end),
    loaded = invert:HasShader(invertShader),
}}
window:ClearPostProcesses()
ready()
capture(window, "cleared")
results.cleared = #window:GetPostProcesses()
results.destroyed = not pcall(function()
    return invert.Enabled
end)
"#
    )));
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    let results: Table = run.outcome.global("results");
    assert_eq!(results.get::<i64>("count").unwrap(), 2);
    assert_eq!(results.get::<String>("class").unwrap(), "PostProcess");
    assert_eq!(results.get::<i64>("renderables").unwrap(), 1);
    assert_eq!(results.get::<i64>("cleared").unwrap(), 0);
    for key in ["first", "noPosition", "loaded", "destroyed"] {
        assert!(results.get::<bool>(key).unwrap(), "{key}");
    }
    let inverted = &run.captures["inverted"];
    assert_pixel(inverted, 20, 50, [255, 255, 255, 255]);
    assert_pixel(inverted, 100, 50, [0, 255, 255, 255]);
    let tinted = &run.captures["tinted"];
    assert_pixel(tinted, 20, 50, [255, 128, 0, 255]);
    assert_pixel(tinted, 100, 50, [0, 128, 0, 255]);
    let reordered = &run.captures["reordered"];
    assert_pixel(reordered, 20, 50, [255, 255, 255, 255]);
    assert_pixel(reordered, 100, 50, [0, 255, 255, 255]);
    let disabled = &run.captures["disabled"];
    assert_pixel(disabled, 20, 50, [0, 0, 0, 255]);
    assert_pixel(disabled, 100, 50, [255, 0, 0, 255]);
    let cleared = &run.captures["cleared"];
    assert_pixel(cleared, 20, 50, [0, 0, 0, 255]);
    assert_pixel(cleared, 100, 50, [255, 0, 0, 255]);
}

#[tokio::test]
async fn waitfor_holds_until_an_image_is_ready_to_draw() {
    if Gpu::get().is_err() {
        eprintln!("skipping a rendering test: no GPU");
        return;
    }
    let dir = project(
        r##"
local Asset = import("Asset")
local Window = import("Window")

local window = Window.new({ Title = "Ready", Size = udim.new(40, 40), BackgroundColor = color.new(0, 0, 0, 1) })
local Renderable = window:GetAPI("Renderable")

results = {}

local icon = Asset.Load("quad.png")
results.assetBack = Renderable.WaitFor(icon) == icon

local image = Renderable.new("RenderableImage", {
    Image = icon,
    Position = udim.new(20, 20),
    Size = udim.new(40, 40),
})
capture(window, "ready")

local shape = Renderable.new("RenderableShape", { Position = udim.new(20, 20), Size = udim.new(2, 2) })
results.shapeIsInstant = Renderable.WaitFor(shape) == shape
results.imageBack = Renderable.WaitFor(image) == image
results.manyAtOnce = select("#", Renderable.WaitFor(icon, image, shape)) == 3
results.badArgument = tostring(select(2, pcall(Renderable.WaitFor, 12)))

window:Close()
"##,
    );
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    let results: Table = run.outcome.global("results");
    assert!(results.get::<bool>("assetBack").unwrap(), "WaitFor gives back what it was given");
    assert!(results.get::<bool>("shapeIsInstant").unwrap());
    assert!(results.get::<bool>("imageBack").unwrap());
    assert!(results.get::<bool>("manyAtOnce").unwrap());
    let message: String = results.get("badArgument").unwrap();
    assert!(message.contains("WaitFor takes renderables and assets"), "{message}");

    let capture = &run.captures["ready"];
    let pixel = |x: u32, y: u32| {
        let index = ((y * capture.width + x) * 4) as usize;
        capture.rgba[index..index + 4].to_vec()
    };
    let corner = |x: u32, y: u32, channel: usize| {
        let pixel = pixel(x, y);
        let others: Vec<u8> = (0..3).filter(|index| *index != channel).map(|index| pixel[index]).collect();
        assert!(
            pixel[channel] > 200 && others.iter().all(|value| *value < 60),
            "the image drew its own pixels on its first frame, got {pixel:?} at {x},{y}"
        );
    };
    corner(6, 6, 0);
    corner(33, 6, 1);
    corner(6, 33, 2);
}

#[tokio::test]
async fn waitfor_warms_a_shader_pipeline_before_it_draws() {
    if Gpu::get().is_err() {
        eprintln!("skipping a rendering test: no GPU");
        return;
    }
    let dir = project(
        r##"
local Shader = import("Shader")
local Window = import("Window")

local window = Window.new({ Title = "Warm", Size = udim.new(40, 40), BackgroundColor = color.new(0, 0, 0, 1) })
local Renderable = window:GetAPI("Renderable")

local shader = Shader.Compile(Shader.Combine({ Shader.Prelude, [==[
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((0x32u >> index) & 1u), f32((0x2cu >> index) & 1u));
    let world = vec2<f32>(4.0, 4.0) + corner * vec2<f32>(32.0, 32.0);
    return vec4<f32>(world.x / frame.resolution.x * 2.0 - 1.0, 1.0 - world.y / frame.resolution.y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 1.0, 1.0, 1.0);
}
]==] }, { Name = "warm" }))

local painted = Renderable.new("Renderable", { Shaders = { shader }, VertexCount = 6 })

local started = os.clock()
Renderable.WaitFor(painted)
warmSeconds = os.clock() - started

capture(window, "warm")
window:Close()
"##,
    );
    let run = run(dir.path(), true).await;
    run.outcome.assert_clean();
    assert!(run.outcome.global::<f64>("warmSeconds") >= 0.0);

    let capture = &run.captures["warm"];
    let index = ((20 * capture.width + 20) * 4) as usize;
    let pixel = &capture.rgba[index..index + 4];
    assert_eq!(pixel, [0, 255, 255, 255], "the shader drew on the first frame after WaitFor");
}
