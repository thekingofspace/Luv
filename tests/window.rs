mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{main_script, run_with, workspace};
use luv::datatypes::EnumItem;
use luv::project::Project;
use luv::window::{
    ControllerEvent, HeadlessWindows, InputKind, ResizePhase, TouchPhase, WindowEvent, WindowMode, WindowSystem,
};
use mlua::Table;

fn enum_name(event: &Table, enum_type: &str, key: &str) -> mlua::Result<&'static str> {
    let value: String = event.get(key)?;
    EnumItem::find(enum_type, &value)
        .map(|item| item.name)
        .ok_or_else(|| mlua::Error::runtime(format!("{value} is not a {enum_type}")))
}

fn input_event(event: &Table) -> mlua::Result<WindowEvent> {
    let kind: String = event.get("Kind")?;
    let point = || -> mlua::Result<(f64, f64)> { Ok((event.get("X")?, event.get("Y")?)) };
    Ok(match kind.as_str() {
        "Key" => WindowEvent::Key {
            key: enum_name(event, "KeyCode", "Key")?,
            pressed: event.get("Pressed")?,
        },
        "Text" => WindowEvent::Text(event.get("Text")?),
        "MouseMoved" => {
            let (x, y) = point()?;
            WindowEvent::MouseMoved { x, y }
        }
        "MouseMotion" => {
            let (x, y) = point()?;
            WindowEvent::MouseMotion { x, y }
        }
        "MouseWheel" => {
            let (x, y) = point()?;
            WindowEvent::MouseWheel { x, y }
        }
        "MouseButton" => WindowEvent::MouseButton {
            button: enum_name(event, "MouseButton", "Button")?,
            pressed: event.get("Pressed")?,
        },
        "MouseInside" => WindowEvent::MouseInside(event.get("Inside")?),
        "Touch" => {
            let (x, y) = point()?;
            let phase = match event.get::<String>("Phase")?.as_str() {
                "Started" => TouchPhase::Started,
                "Moved" => TouchPhase::Moved,
                "Ended" => TouchPhase::Ended,
                _ => TouchPhase::Cancelled,
            };
            WindowEvent::Touch {
                id: event.get("Id")?,
                phase,
                x,
                y,
                force: event.get("Force")?,
            }
        }
        other => return Err(mlua::Error::runtime(format!("unknown input kind {other}"))),
    })
}

fn controller_event(event: &Table) -> mlua::Result<ControllerEvent> {
    let kind: String = event.get("Kind")?;
    Ok(match kind.as_str() {
        "Connected" => ControllerEvent::Connected {
            name: event.get("Name")?,
            vibrates: event.get::<Option<bool>>("Vibrates")?.unwrap_or(false),
        },
        "Disconnected" => ControllerEvent::Disconnected,
        "Button" => ControllerEvent::Button {
            button: enum_name(event, "ControllerButton", "Button")?,
            pressed: event.get("Pressed")?,
        },
        "Axis" => ControllerEvent::Axis {
            axis: enum_name(event, "ControllerAxis", "Axis")?,
            value: event.get("Value")?,
        },
        other => return Err(mlua::Error::runtime(format!("unknown controller event {other}"))),
    })
}

async fn run_windowed(root: &Path) -> (common::Outcome, Arc<HeadlessWindows>) {
    let headless = Arc::new(HeadlessWindows::new());
    let project = Project::load(root).unwrap();
    let simulator = headless.clone();
    let icon = project.manifest.game.icon.clone();
    let outcome = run_with(Arc::new(project.source_vfs()), "src/main.luau", move |builder| {
        let windows = simulator.clone();
        builder.game("Fixture", ".").icon(icon.as_deref()).windows(simulator.clone()).setup(move |lua| {
            let find = {
                let windows = windows.clone();
                move |title: &str| {
                    windows
                        .find(title)
                        .ok_or_else(|| mlua::Error::runtime(format!("no window titled {title}")))
                }
            };
            let resize = {
                let (windows, find) = (windows.clone(), find.clone());
                lua.create_function(
                    move |_, (title, width, height, phase): (String, f64, f64, Option<String>)| {
                        let phase = match phase.as_deref() {
                            Some("live") => ResizePhase::Live,
                            Some("settling") => ResizePhase::Settling,
                            _ => ResizePhase::Done,
                        };
                        Ok(windows.simulate(find(&title)?, WindowEvent::Resized { width, height, phase }))
                    },
                )?
            };
            let ended = {
                let (windows, find) = (windows.clone(), find.clone());
                lua.create_function(move |_, title: String| {
                    Ok(windows.simulate(find(&title)?, WindowEvent::ResizeEnded))
                })?
            };
            let focus = {
                let (windows, find) = (windows.clone(), find.clone());
                lua.create_function(move |_, (title, focused): (String, bool)| {
                    Ok(windows.simulate(find(&title)?, WindowEvent::Focused(focused)))
                })?
            };
            let close = {
                let (windows, find) = (windows.clone(), find.clone());
                lua.create_function(move |_, title: String| {
                    Ok(windows.simulate(find(&title)?, WindowEvent::CloseRequested))
                })?
            };
            let input = {
                let (windows, find) = (windows.clone(), find.clone());
                lua.create_function(move |_, (title, event): (String, Table)| {
                    Ok(windows.simulate(find(&title)?, input_event(&event)?))
                })?
            };
            let controller = {
                let windows = windows.clone();
                lua.create_function(move |_, (id, event): (usize, Table)| {
                    windows.controllers().publish(id, controller_event(&event)?);
                    Ok(())
                })?
            };
            let vibrations = {
                let windows = windows.clone();
                lua.create_function(move |lua, ()| {
                    let list = lua.create_table()?;
                    for vibration in windows.controllers().take_vibrations() {
                        let entry = lua.create_table()?;
                        entry.set("Id", vibration.id)?;
                        entry.set("Strength", vibration.strength)?;
                        entry.set("Duration", vibration.duration.as_secs_f64())?;
                        list.raw_push(entry)?;
                    }
                    Ok(list)
                })?
            };
            let inspect = {
                let windows = windows.clone();
                lua.create_function(move |lua, title: String| {
                    let Some((_, settings)) = windows.windows().into_iter().find(|(_, settings)| settings.title == title)
                    else {
                        return Ok(None);
                    };
                    let table = lua.create_table()?;
                    table.set("width", settings.width)?;
                    table.set("height", settings.height)?;
                    table.set("mode", settings.mode.name())?;
                    table.set("resizable", settings.resizable)?;
                    table.set("iconWidth", settings.icon.as_ref().map(|icon| icon.width))?;
                    table.set("cursorIcon", settings.cursor_icon)?;
                    table.set("cursorVisible", settings.cursor_visible)?;
                    table.set("cursorLock", settings.cursor_lock)?;
                    table.set("sizeLocked", settings.size_locked)?;
                    table.set("x", settings.position.map(|(x, _)| x))?;
                    table.set("y", settings.position.map(|(_, y)| y))?;
                    Ok(Some(table))
                })?
            };
            let device = {
                let windows = windows.clone();
                lua.create_function(move |_, (kind, present): (String, bool)| {
                    let kind = InputKind::from_name(&kind)
                        .ok_or_else(|| mlua::Error::runtime(format!("{kind} is not an input kind")))?;
                    windows.input_devices().set(kind, present);
                    Ok(())
                })?
            };
            lua.globals().set("simulateDevice", device)?;
            lua.globals().set("simulateResize", resize)?;
            lua.globals().set("simulateResizeEnded", ended)?;
            lua.globals().set("simulateFocus", focus)?;
            lua.globals().set("simulateClose", close)?;
            lua.globals().set("simulateInput", input)?;
            lua.globals().set("simulateController", controller)?;
            lua.globals().set("takeVibrations", vibrations)?;
            lua.globals().set("inspect", inspect)
        })
    })
    .await;
    (outcome, headless)
}

#[tokio::test]
async fn windows_open_with_their_config_and_defaults() {
    let dir = main_script(
        r#"
local Window = import("Window")
local main = Window.new({
    Title = "Main",
    Size = udim.new(800, 600, 0),
    FPS = 30,
    Type = enum.WindowType.Borderless,
    Resizable = false,
})
local other = Window.new()
local backend = inspect("Main")
results = {
    className = main.ClassName,
    title = main.Title,
    size = tostring(main.Size),
    fps = main.FPS,
    windowType = tostring(main.Type),
    resizable = main.Resizable,
    isOpen = main.IsOpen,
    api = type(main:GetAPI("Renderable").new) == "function",
    unknownApi = not pcall(main.GetAPI, main, "Nope"),
    otherTitle = other.Title,
    otherSize = tostring(other.Size),
    otherType = other.Type == enum.WindowType.Windowed,
    backendMode = backend.mode,
    backendResizable = backend.resizable,
    backendWidth = backend.width,
}
main:Close()
other:Close()
results.closed = not main.IsOpen
"#,
    );
    let (outcome, headless) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap();
    assert_eq!(text("className"), "Window");
    assert_eq!(text("title"), "Main");
    assert_eq!(text("size"), "UDim(800, 600, 0)");
    assert_eq!(results.get::<f64>("fps").unwrap(), 30.0);
    assert_eq!(text("windowType"), "enum.WindowType.Borderless");
    assert!(!results.get::<bool>("resizable").unwrap());
    assert_eq!(text("otherTitle"), "Fixture");
    assert_eq!(text("otherSize"), "UDim(1280, 720, 0)");
    assert_eq!(text("backendMode"), "Borderless");
    assert!(!results.get::<bool>("backendResizable").unwrap());
    assert_eq!(results.get::<f64>("backendWidth").unwrap(), 800.0);
    for key in ["isOpen", "api", "unknownApi", "otherType", "closed"] {
        assert!(results.get::<bool>(key).unwrap(), "{key}");
    }
    assert!(headless.windows().is_empty());
}

#[tokio::test]
async fn properties_update_the_window() {
    let dir = main_script(
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Editable" })
window.Title = "Renamed"
window.Type = enum.WindowType.FullScreen
window.Resizable = false
window.FPS = 5000
window.Size = udim.new(1024, 768)
local size = window.SizeChanged:Wait()
local backend = inspect("Renamed")
results = {
    changedTo = tostring(size),
    size = tostring(window.Size),
    fps = window.FPS,
    backendMode = backend.mode,
    backendResizable = backend.resizable,
    backendHeight = backend.height,
    badType = not pcall(function() window.Type = "FullScreen" end),
    badSize = not pcall(function() window.Size = udim.new(0, 10) end),
    badFps = not pcall(function() window.FPS = 0 end),
}
window:Close()
"#,
    );
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(results.get::<String>("changedTo").unwrap(), "UDim(1024, 768, 0)");
    assert_eq!(results.get::<String>("size").unwrap(), "UDim(1024, 768, 0)");
    assert_eq!(results.get::<f64>("fps").unwrap(), 1000.0);
    assert_eq!(results.get::<String>("backendMode").unwrap(), WindowMode::FullScreen.name());
    assert!(!results.get::<bool>("backendResizable").unwrap());
    assert_eq!(results.get::<f64>("backendHeight").unwrap(), 768.0);
    for key in ["badType", "badSize", "badFps"] {
        assert!(results.get::<bool>(key).unwrap(), "{key}");
    }
}

#[tokio::test]
async fn os_events_fire_window_signals() {
    let dir = main_script(
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Events" })
log = {}
window.FocusLost:BindHandler("log", function() table.insert(log, "lost") end)
window.FocusGained:BindHandler("log", function() table.insert(log, "gained") end)
window.SizeChanged:BindHandler("log", function(size) table.insert(log, `size {size.X}x{size.Y}`) end)

simulateResize("Events", 640, 480)
window.SizeChanged:Wait()
simulateFocus("Events", false)
window.FocusLost:Wait()
focusedAfterLoss = window.Focused
simulateFocus("Events", true)
window.FocusGained:Wait()
focusedAfterGain = window.Focused
window:Close()
"#,
    );
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["gained", "size 640x480", "lost", "gained"]);
    assert!(!outcome.global::<bool>("focusedAfterLoss"));
    assert!(outcome.global::<bool>("focusedAfterGain"));
}

#[tokio::test]
async fn live_resizes_update_continuously_and_change_size_once() {
    let dir = main_script(
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Drag", Size = udim.new(400, 300) })
updates = {}
changes = {}
window.WindowUpdate:BindHandler("log", function(size) table.insert(updates, `{size.X}x{size.Y}`) end)
window.SizeChanged:BindHandler("log", function(size) table.insert(changes, `{size.X}x{size.Y}`) end)
sleep(30)

simulateResize("Drag", 410, 300, "live")
simulateResize("Drag", 420, 300, "live")
simulateResize("Drag", 430, 305, "live")
sleep(60)
duringDrag = #changes
liveSize = tostring(window.Size)
simulateResizeEnded("Drag")
window.SizeChanged:Wait()

simulateResize("Drag", 500, 400, "settling")
simulateResize("Drag", 520, 410, "settling")
sleep(60)
beforeSettled = #changes
window.SizeChanged:Wait()

simulateResize("Drag", 640, 480)
window.SizeChanged:Wait()
simulateResizeEnded("Drag")
sleep(30)
window:Close()
"#,
    );
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let updates: Vec<String> = outcome.global("updates");
    let changes: Vec<String> = outcome.global("changes");
    assert_eq!(updates, ["410x300", "420x300", "430x305", "500x400", "520x410", "640x480"]);
    assert_eq!(changes, ["430x305", "520x410", "640x480"]);
    assert_eq!(outcome.global::<i64>("duringDrag"), 0);
    assert_eq!(outcome.global::<i64>("beforeSettled"), 1);
    assert_eq!(outcome.global::<String>("liveSize"), "UDim(430, 305, 0)");
}

#[tokio::test]
async fn closing_from_the_os_ends_the_game() {
    let started = Instant::now();
    let dir = main_script(
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Closable" })
window.Closed:BindHandler("log", function()
    closedFired = true
end)
coroutine.wrap(function()
    sleep(20)
    simulateClose("Closable")
end)()
"#,
    );
    let (outcome, headless) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("closedFired"));
    assert!(headless.windows().is_empty());
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn frames_fire_in_order_with_delta_time() {
    let dir = main_script(
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Frames", FPS = 120 })
log = {}
deltas = {}
local frames = 0
window.PreFrame:BindHandler("log", function(dt)
    table.insert(log, "pre")
    table.insert(deltas, dt)
end)
window.OnFrame:BindHandler("log", function() table.insert(log, "on") end)
window.AfterFrame:BindHandler("log", function()
    table.insert(log, "after")
    frames += 1
    if frames == 3 then
        window:Close()
    end
end)
"#,
    );
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["pre", "on", "after", "pre", "on", "after", "pre", "on", "after"]);
    let deltas: Vec<f64> = outcome.global("deltas");
    assert!(deltas.iter().all(|dt| *dt > 0.0 && *dt < 1.0), "{deltas:?}");
}

#[tokio::test]
async fn open_windows_keep_the_game_running() {
    let started = Instant::now();
    let dir = main_script(
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Alive" })
coroutine.wrap(function()
    sleep(100)
    window:Close()
end)()
"#,
    );
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    assert!(started.elapsed() >= Duration::from_millis(100));
}

#[tokio::test]
async fn destroying_a_window_closes_it_silently() {
    let dir = main_script(
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Destroyed" })
window.Closed:BindHandler("log", function() closedFired = true end)
window:Destroy()
isOpen = window.IsOpen
"#,
    );
    let (outcome, headless) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    assert!(!outcome.global::<bool>("isOpen"));
    assert_eq!(outcome.global::<Option<bool>>("closedFired"), None);
    assert!(headless.windows().is_empty());
}

#[tokio::test]
async fn parallel_blocks_can_open_windows() {
    let dir = main_script(
        r#"
local Messenger = import("Messenger")
Messenger:Subscribe("Worker", function(frames, thread)
    worker = { frames = frames, thread = thread }
end)

EnterParallel()
local Window = import("Window")
local window = Window.new({ Title = "Worker", FPS = 200 })
local frames = 0
window.OnFrame:BindHandler("count", function()
    frames += 1
    if frames == 2 then
        window:Close()
        Messenger:Fire("Worker", frames, threadName())
    end
end)
ExitParallel()
"#,
    );
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let worker: Table = outcome.global("worker");
    assert_eq!(worker.get::<i64>("frames").unwrap(), 2);
    assert_eq!(worker.get::<String>("thread").unwrap(), "parallel block #1 of src/main.luau");
}

#[tokio::test]
async fn window_icons_load_from_assets() {
    let dir = workspace(&[(
        "src/main.luau",
        r#"
local Window = import("Window")
local Asset = import("Asset")
local window = Window.new({ Title = "Iconic", Icon = "icon.png" })
local other = Window.new({ Title = "Loaded" })
other.Icon = Asset.Load("icon")
for _ = 1, 200 do
    local first, second = inspect("Iconic"), inspect("Loaded")
    if first.iconWidth and second.iconWidth then
        iconWidths = { first.iconWidth, second.iconWidth }
        break
    end
    sleep(5)
end
iconIsAsset = other.Icon.ClassName == "Asset"
window:Close()
other:Close()
"#,
    )]);
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    image::RgbaImage::from_pixel(3, 2, image::Rgba([255, 0, 0, 255]))
        .save(dir.path().join("assets").join("icon.png"))
        .unwrap();
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let widths: Vec<u32> = outcome.global("iconWidths");
    assert_eq!(widths, [3, 3]);
    assert!(outcome.global::<bool>("iconIsAsset"));
}

#[tokio::test]
async fn window_icons_load_from_svg_and_other_formats() {
    let dir = workspace(&[(
        "src/main.luau",
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Vector", Icon = "badge.svg" })
local other = Window.new({ Title = "Animated", Icon = "badge.gif" })
for _ = 1, 200 do
    local first, second = inspect("Vector"), inspect("Animated")
    if first.iconWidth and second.iconWidth then
        iconWidths = { first.iconWidth, second.iconWidth }
        break
    end
    sleep(5)
end
window:Close()
other:Close()
"#,
    )]);
    let assets = dir.path().join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::write(
        assets.join("badge.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 16"><rect width="24" height="16" fill="#3070ff"/></svg>"##,
    )
    .unwrap();
    image::RgbaImage::from_pixel(5, 4, image::Rgba([0, 255, 0, 255]))
        .save(assets.join("badge.gif"))
        .unwrap();
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let widths: Vec<u32> = outcome.global("iconWidths");
    assert_eq!(widths, [24, 5]);
}

#[tokio::test]
async fn closing_a_window_lets_go_of_its_handlers() {
    let dir = main_script(
        r#"
local Window = import("Window")

local held = setmetatable({}, { __mode = "v" })
local window = Window.new({ Title = "Cleanup" })
local input = window:GetAPI("Input")

local function bind(signal, key)
	local kept = {}
	held[key] = kept
	signal:BindHandler("hold", function()
		return kept
	end)
end

bind(window.PreFrame, "frame")
bind(window.Closed, "closed")
bind(input.KeyDown, "key")
sleep(30)
collectgarbage("collect")
results = { before = held.frame ~= nil and held.key ~= nil and held.closed ~= nil }

window:Close()
collectgarbage("collect")
collectgarbage("collect")
results.frame = held.frame == nil
results.closed = held.closed == nil
results.key = held.key == nil
"#,
    );
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    for key in ["before", "frame", "closed", "key"] {
        assert!(results.get::<bool>(key).unwrap(), "{key} failed");
    }
}

#[tokio::test]
async fn windows_use_the_game_icon_until_a_script_sets_one() {
    let dir = workspace(&[(
        "src/main.luau",
        r#"
local Window = import("Window")
local plain = Window.new({ Title = "Plain" })
local custom = Window.new({ Title = "Custom", Icon = "badge.gif" })
local function iconWidth(title, width)
    for _ = 1, 200 do
        if inspect(title).iconWidth == width then
            return true
        end
        sleep(5)
    end
    return false
end
results = { plain = iconWidth("Plain", 7), custom = iconWidth("Custom", 5) }
custom.Icon = nil
results.reset = iconWidth("Custom", 7)
results.property = custom.Icon == nil
plain:Close()
custom:Close()
"#,
    )]);
    std::fs::write(
        dir.path().join("build.toml"),
        "[game]\nname = \"Fixture\"\nicon = \"assets/game.png\"\n",
    )
    .unwrap();
    let assets = dir.path().join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    image::RgbaImage::from_pixel(7, 3, image::Rgba([255, 0, 128, 255]))
        .save(assets.join("game.png"))
        .unwrap();
    image::RgbaImage::from_pixel(5, 4, image::Rgba([0, 255, 0, 255]))
        .save(assets.join("badge.gif"))
        .unwrap();
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    for key in ["plain", "custom", "reset", "property"] {
        assert!(results.get::<bool>(key).unwrap(), "{key} failed");
    }
}

#[tokio::test]
async fn input_apis_report_when_devices_come_and_go() {
    let dir = main_script(
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Devices" })
sleep(30)
local input = window:GetAPI("Input")
local mouse = window:GetAPI("Mouse")
local touch = window:GetAPI("Touch")
local controller = window:GetAPI("Controller")
changes = {}
for name, api in { Input = input, Mouse = mouse, Touch = touch, Controller = controller } do
    api.ActivationChanged:BindHandler("log", function(connected)
        table.insert(changes, `{name}={connected}`)
    end)
end
results = {
    keyboard = input.IsConnected,
    mouse = mouse.IsConnected,
    touch = touch.IsConnected,
    controller = controller.IsConnected,
}
simulateDevice("Keyboard", false)
simulateDevice("Mouse", false)
simulateDevice("Touch", false)
simulateDevice("Touch", false)
sleep(30)
results.keyboardGone = not input.IsConnected
results.mouseGone = not mouse.IsConnected
results.touchGone = not touch.IsConnected
simulateController(1, { Kind = "Connected", Name = "Pad" })
simulateController(2, { Kind = "Connected", Name = "Second" })
sleep(30)
results.controllerHere = controller.IsConnected
simulateController(1, { Kind = "Disconnected" })
sleep(30)
results.controllerStill = controller.IsConnected
simulateController(2, { Kind = "Disconnected" })
sleep(30)
results.controllerGone = not controller.IsConnected
simulateDevice("Keyboard", true)
sleep(30)
results.keyboardBack = input.IsConnected
window:Close()
"#,
    );
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    for key in [
        "keyboard",
        "mouse",
        "touch",
        "keyboardGone",
        "mouseGone",
        "touchGone",
        "controllerHere",
        "controllerStill",
        "controllerGone",
        "keyboardBack",
    ] {
        assert!(results.get::<bool>(key).unwrap(), "{key}");
    }
    assert!(!results.get::<bool>("controller").unwrap());
    let changes: Vec<String> = outcome.global("changes");
    assert_eq!(
        changes,
        [
            "Input=false",
            "Mouse=false",
            "Touch=false",
            "Controller=true",
            "Controller=false",
            "Input=true"
        ]
    );
}

#[tokio::test]
async fn windows_need_a_window_system() {
    let dir = main_script(
        r#"
local Window = import("Window")
local ok, message = pcall(Window.new)
windowError = tostring(message)
"#,
    );
    let outcome = common::run_source(dir.path()).await;
    outcome.assert_clean();
    assert!(outcome.global::<String>("windowError").contains("windows are not available"));
}

#[tokio::test]
async fn keyboard_mouse_and_touch_only_reach_the_focused_window() {
    let dir = main_script(
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Input" })
local input = window:GetAPI("Input")
local mouse = window:GetAPI("Mouse")
local touch = window:GetAPI("Touch")
log = {}
local function add(text) table.insert(log, text) end
input.KeyDown:BindHandler("log", function(key) add(`down {key.Name}`) end)
input.KeyUp:BindHandler("log", function(key) add(`up {key.Name}`) end)
input.TextInput:BindHandler("log", function(text) add(`text {text}`) end)
mouse.Moved:BindHandler("log", function(position, delta) add(`moved {position.X},{position.Y} by {delta.X},{delta.Y}`) end)
mouse.ButtonDown:BindHandler("log", function(button, position) add(`press {button.Name} at {position.X},{position.Y}`) end)
mouse.ButtonUp:BindHandler("log", function(button) add(`release {button.Name}`) end)
mouse.Scrolled:BindHandler("log", function(delta) add(`scroll {delta.Y}`) end)
mouse.Entered:BindHandler("log", function() add("entered") end)
mouse.Left:BindHandler("log", function() add("left") end)
touch.Started:BindHandler("log", function(id, position) add(`touch {id} at {position.X},{position.Y}`) end)
touch.Moved:BindHandler("log", function(id, _, delta) add(`drag {id} by {delta.X},{delta.Y}`) end)
touch.Ended:BindHandler("log", function(id) add(`lift {id}`) end)
touch.Cancelled:BindHandler("log", function(id) add(`cancel {id}`) end)

simulateInput("Input", { Kind = "Key", Key = "W", Pressed = true })
simulateInput("Input", { Kind = "Key", Key = "W", Pressed = true })
simulateInput("Input", { Kind = "Key", Key = "W", Pressed = true })
simulateInput("Input", { Kind = "Text", Text = "w" })
simulateInput("Input", { Kind = "MouseInside", Inside = true })
simulateInput("Input", { Kind = "MouseMoved", X = 10, Y = 20 })
simulateInput("Input", { Kind = "MouseMoved", X = 15, Y = 25 })
simulateInput("Input", { Kind = "MouseButton", Button = "Left", Pressed = true })
simulateInput("Input", { Kind = "MouseWheel", X = 0, Y = -2 })
simulateInput("Input", { Kind = "Touch", Id = 7, Phase = "Started", X = 1, Y = 2 })
simulateInput("Input", { Kind = "Touch", Id = 7, Phase = "Moved", X = 4, Y = 6 })
sleep(30)
held = {
    w = input:IsKeyDown(enum.KeyCode.W),
    a = input:IsKeyDown(enum.KeyCode.A),
    keys = #input:GetKeysDown(),
    left = mouse:IsButtonDown(enum.MouseButton.Left),
    buttons = #mouse:GetButtonsDown(),
    position = tostring(mouse.Position),
    inside = mouse.IsInside,
    touches = #touch:GetTouches(),
}

simulateFocus("Input", false)
window.FocusLost:Wait()
blurred = {
    w = input:IsKeyDown(enum.KeyCode.W),
    left = mouse:IsButtonDown(enum.MouseButton.Left),
    touches = #touch:GetTouches(),
}
simulateInput("Input", { Kind = "Key", Key = "A", Pressed = true })
simulateInput("Input", { Kind = "MouseMoved", X = 50, Y = 60 })
simulateInput("Input", { Kind = "MouseButton", Button = "Right", Pressed = true })
sleep(30)
ignored = input:IsKeyDown(enum.KeyCode.A) or mouse:IsButtonDown(enum.MouseButton.Right)
blurredPosition = tostring(mouse.Position)

simulateFocus("Input", true)
window.FocusGained:Wait()
simulateInput("Input", { Kind = "Key", Key = "W", Pressed = false })
simulateInput("Input", { Kind = "Key", Key = "Space", Pressed = true })
simulateInput("Input", { Kind = "Key", Key = "Space", Pressed = false })
sleep(30)

mouse.Icon = enum.MouseIcon.Pointer
mouse.Visible = false
mouse.LockMode = enum.MouseLockMode.Locked
simulateInput("Input", { Kind = "MouseMotion", X = 3, Y = -4 })
sleep(30)
local backend = inspect("Input")
cursor = {
    icon = backend.cursorIcon,
    visible = backend.cursorVisible,
    lock = backend.cursorLock,
    apiIcon = mouse.Icon == enum.MouseIcon.Pointer,
    apiVisible = mouse.Visible,
    apiLock = tostring(mouse.LockMode),
}
badIcon = not pcall(function() mouse.Icon = enum.MouseButton.Left end)
sameApi = window:GetAPI("Input") == input and window:GetAPI("Mouse") == mouse
kinds = `{typeof(input)} {typeof(mouse)} {typeof(touch)}`
window:Close()
closedError = not pcall(function() return input:IsKeyDown(enum.KeyCode.W) end)
"#,
    );
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(
        log,
        [
            "down W",
            "text w",
            "entered",
            "moved 10,20 by 10,20",
            "moved 15,25 by 5,5",
            "press Left at 15,25",
            "scroll -2",
            "touch 7 at 1,2",
            "drag 7 by 3,4",
            "up W",
            "release Left",
            "cancel 7",
            "down Space",
            "up Space",
            "moved 50,60 by 3,-4",
        ]
    );
    let held: Table = outcome.global("held");
    assert!(held.get::<bool>("w").unwrap());
    assert!(!held.get::<bool>("a").unwrap());
    assert_eq!(held.get::<i64>("keys").unwrap(), 1);
    assert!(held.get::<bool>("left").unwrap());
    assert_eq!(held.get::<i64>("buttons").unwrap(), 1);
    assert_eq!(held.get::<String>("position").unwrap(), "UDim(15, 25, 0)");
    assert!(held.get::<bool>("inside").unwrap());
    assert_eq!(held.get::<i64>("touches").unwrap(), 1);
    let blurred: Table = outcome.global("blurred");
    assert!(!blurred.get::<bool>("w").unwrap());
    assert!(!blurred.get::<bool>("left").unwrap());
    assert_eq!(blurred.get::<i64>("touches").unwrap(), 0);
    assert!(!outcome.global::<bool>("ignored"));
    assert_eq!(outcome.global::<String>("blurredPosition"), "UDim(50, 60, 0)");
    let cursor: Table = outcome.global("cursor");
    assert_eq!(cursor.get::<String>("icon").unwrap(), "Pointer");
    assert!(!cursor.get::<bool>("visible").unwrap());
    assert_eq!(cursor.get::<String>("lock").unwrap(), "Locked");
    assert!(cursor.get::<bool>("apiIcon").unwrap());
    assert!(!cursor.get::<bool>("apiVisible").unwrap());
    assert_eq!(cursor.get::<String>("apiLock").unwrap(), "enum.MouseLockMode.Locked");
    assert_eq!(outcome.global::<String>("kinds"), "InputAPI MouseAPI TouchAPI");
    for key in ["badIcon", "sameApi", "closedError"] {
        assert!(outcome.global::<bool>(key), "{key}");
    }
}

#[tokio::test]
async fn controllers_report_state_and_release_on_focus_loss() {
    let dir = main_script(
        r#"
local Window = import("Window")
local window = Window.new({ Title = "Pads" })
local controller = window:GetAPI("Controller")
log = {}
local function add(text) table.insert(log, text) end
controller.Connected:BindHandler("log", function(id, name) add(`connected {id} {name}`) end)
controller.Disconnected:BindHandler("log", function(id) add(`disconnected {id}`) end)
controller.ButtonDown:BindHandler("log", function(button, id) add(`down {button.Name} {id}`) end)
controller.ButtonUp:BindHandler("log", function(button, id) add(`up {button.Name} {id}`) end)
controller.AxisChanged:BindHandler("log", function(axis, value, id) add(`axis {axis.Name} {value} {id}`) end)

simulateController(0, { Kind = "Connected", Name = "Pad", Vibrates = true })
simulateController(0, { Kind = "Button", Button = "A", Pressed = true })
simulateController(0, { Kind = "Axis", Axis = "LeftStickX", Value = 0.5 })
simulateController(0, { Kind = "Axis", Axis = "LeftStickY", Value = -0.25 })
simulateController(1, { Kind = "Connected", Name = "Second" })
simulateController(1, { Kind = "Axis", Axis = "LeftStickX", Value = -0.75 })
sleep(30)
local pads = controller:GetControllers()
state = {
    count = #pads,
    name = pads[1].Name,
    vibrates = pads[1].CanVibrate,
    second = pads[2].Name,
    a = controller:IsButtonDown(enum.ControllerButton.A),
    aOnPad = controller:IsButtonDown(enum.ControllerButton.A, 0),
    aOnSecond = controller:IsButtonDown(enum.ControllerButton.A, 1),
    b = controller:IsButtonDown(enum.ControllerButton.B),
    anyX = controller:GetAxis(enum.ControllerAxis.LeftStickX),
    firstX = controller:GetAxis(enum.ControllerAxis.LeftStickX, 0),
    stick = tostring(controller:GetStick(enum.ControllerStick.Left, 0)),
    held = #controller:GetButtonsDown(),
    connected = controller:IsControllerConnected(1),
    missing = controller:IsControllerConnected(5),
}
controller:Vibrate(0.75, 0.5)
controller:StopVibrating(0)
vibrations = takeVibrations()
badVibration = not pcall(function() controller:Vibrate(1, -1) end)

simulateFocus("Pads", false)
window.FocusLost:Wait()
simulateController(0, { Kind = "Button", Button = "B", Pressed = true })
sleep(30)
blurred = {
    b = controller:IsButtonDown(enum.ControllerButton.B),
    stick = tostring(controller:GetStick(enum.ControllerStick.Left)),
}
simulateController(1, { Kind = "Disconnected" })
sleep(30)
remaining = #controller:GetControllers()
kind = typeof(controller)
window:Close()
"#,
    );
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(
        log,
        [
            "connected 0 Pad",
            "down A 0",
            "axis LeftStickX 0.5 0",
            "axis LeftStickY -0.25 0",
            "connected 1 Second",
            "axis LeftStickX -0.75 1",
            "up A 0",
            "axis LeftStickX 0 0",
            "axis LeftStickY 0 0",
            "axis LeftStickX 0 1",
            "disconnected 1",
        ]
    );
    let state: Table = outcome.global("state");
    assert_eq!(state.get::<i64>("count").unwrap(), 2);
    assert_eq!(state.get::<String>("name").unwrap(), "Pad");
    assert!(state.get::<bool>("vibrates").unwrap());
    assert_eq!(state.get::<String>("second").unwrap(), "Second");
    for key in ["a", "aOnPad", "connected"] {
        assert!(state.get::<bool>(key).unwrap(), "{key}");
    }
    for key in ["aOnSecond", "b", "missing"] {
        assert!(!state.get::<bool>(key).unwrap(), "{key}");
    }
    assert_eq!(state.get::<f64>("anyX").unwrap(), -0.75);
    assert_eq!(state.get::<f64>("firstX").unwrap(), 0.5);
    assert_eq!(state.get::<String>("stick").unwrap(), "UDim(0.5, -0.25, 0)");
    assert_eq!(state.get::<i64>("held").unwrap(), 1);
    let vibrations: Vec<Table> = outcome.global("vibrations");
    assert_eq!(vibrations.len(), 2);
    assert_eq!(vibrations[0].get::<Option<i64>>("Id").unwrap(), None);
    assert_eq!(vibrations[0].get::<f64>("Strength").unwrap(), 0.75);
    assert_eq!(vibrations[0].get::<f64>("Duration").unwrap(), 0.5);
    assert_eq!(vibrations[1].get::<Option<i64>>("Id").unwrap(), Some(0));
    assert_eq!(vibrations[1].get::<f64>("Strength").unwrap(), 0.0);
    assert!(outcome.global::<bool>("badVibration"));
    let blurred: Table = outcome.global("blurred");
    assert!(!blurred.get::<bool>("b").unwrap());
    assert_eq!(blurred.get::<String>("stick").unwrap(), "UDim(0, 0, 0)");
    assert_eq!(outcome.global::<i64>("remaining"), 1);
    assert_eq!(outcome.global::<String>("kind"), "ControllerAPI");
}

#[tokio::test]
async fn windows_can_be_placed_locked_and_see_the_screens() {
    let dir = main_script(
        r#"
local Window = import("Window")
local Viewport = import("Viewport")
local window = Window.new({ Title = "Placed", Position = udim.new(40, 60), SizeLocked = true })
moves = {}
window.Moved:BindHandler("log", function(position)
    table.insert(moves, `{position.X},{position.Y}`)
end)
sleep(30)
results = {
    start = tostring(window.Position),
    locked = window.SizeLocked,
    startLocked = inspect("Placed").sizeLocked,
}
window.Position = udim.new(100, 120)
window.Moved:Wait()
results.moved = tostring(window.Position)
local backend = inspect("Placed")
results.backendX = backend.x
results.backendY = backend.y
window.SizeLocked = false
results.unlocked = not inspect("Placed").sizeLocked
results.badPosition = not pcall(function()
    window.Position = udim.new(0 / 0, 1)
end)

local screens = Viewport.GetScreens()
local primary = Viewport.GetPrimaryScreen()
results.screens = #screens
results.name = primary.Name
results.size = tostring(primary.Size)
results.pixels = tostring(primary.PixelSize)
results.work = tostring(primary.WorkSize)
results.scale = primary.Scale
results.refresh = primary.RefreshRate
results.primary = primary.IsPrimary
results.screenSize = tostring(Viewport.GetScreenSize())
results.windowScreen = Viewport.GetWindowScreen(window).Name
window:Close()
"#,
    );
    let (outcome, _) = run_windowed(dir.path()).await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap_or_else(|error| panic!("{key}: {error}"));
    let flag = |key: &str| results.get::<bool>(key).unwrap_or_else(|error| panic!("{key}: {error}"));
    assert_eq!(text("start"), "UDim(40, 60, 0)");
    assert_eq!(text("moved"), "UDim(100, 120, 0)");
    assert_eq!(results.get::<f64>("backendX").unwrap(), 100.0);
    assert_eq!(results.get::<f64>("backendY").unwrap(), 120.0);
    for key in ["locked", "startLocked", "unlocked", "badPosition", "primary"] {
        assert!(flag(key), "{key}");
    }
    let moves: Vec<String> = outcome.global("moves");
    assert_eq!(moves, ["100,120"]);
    assert_eq!(results.get::<i64>("screens").unwrap(), 1);
    assert_eq!(text("name"), "Headless");
    assert_eq!(text("size"), "UDim(1920, 1080, 0)");
    assert_eq!(text("pixels"), "UDim(1920, 1080, 0)");
    assert_eq!(text("work"), "UDim(1920, 1040, 0)");
    assert_eq!(results.get::<f64>("scale").unwrap(), 1.0);
    assert_eq!(results.get::<f64>("refresh").unwrap(), 60.0);
    assert_eq!(text("screenSize"), "UDim(1920, 1080, 0)");
    assert_eq!(text("windowScreen"), "Headless");
}
