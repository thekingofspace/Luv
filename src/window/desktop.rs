use std::collections::HashMap;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition};
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, TouchPhase as OsTouchPhase,
    WindowEvent as OsEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::monitor::MonitorHandle;
use winit::window::{
    CursorGrabMode, CursorIcon, Fullscreen, Icon, Window, WindowButtons, WindowId as OsWindowId,
};

use super::{
    LiveResize, PendingSurface, RenderTarget, ResizePhase, Screen, ScreenReply, TouchPhase, WindowChange, WindowEvent,
    WindowEvents, WindowIcon, WindowId, WindowMode, WindowSettings, WindowSystem,
};

use crate::graphics::gpu;

const LINE_PIXELS: f64 = 40.0;

pub enum WindowCommand {
    Open(WindowId, WindowSettings, WindowEvents),
    Change(WindowId, WindowChange),
    Close(WindowId),
    Screens(Option<WindowId>, ScreenReply),
    Exit,
}

#[derive(Clone)]
pub struct DesktopWindows {
    proxy: EventLoopProxy<WindowCommand>,
}

impl DesktopWindows {
    pub fn event_loop() -> Result<EventLoop<WindowCommand>, String> {
        let event_loop = EventLoop::with_user_event().build().map_err(|error| error.to_string())?;
        gpu::warm_up();
        Ok(event_loop)
    }

    pub fn new(event_loop: &EventLoop<WindowCommand>) -> Self {
        Self {
            proxy: event_loop.create_proxy(),
        }
    }

    pub fn run(event_loop: EventLoop<WindowCommand>) -> Result<(), String> {
        event_loop.set_control_flow(ControlFlow::Wait);
        event_loop
            .run_app(&mut DesktopApp::default())
            .map_err(|error| error.to_string())
    }

    pub fn shutdown(&self) {
        let _ = self.proxy.send_event(WindowCommand::Exit);
    }

    fn send(&self, command: WindowCommand) {
        let _ = self.proxy.send_event(command);
    }
}

impl WindowSystem for DesktopWindows {
    fn open(&self, id: WindowId, settings: WindowSettings, events: WindowEvents) {
        self.send(WindowCommand::Open(id, settings, events));
    }

    fn change(&self, id: WindowId, change: WindowChange) {
        self.send(WindowCommand::Change(id, change));
    }

    fn close(&self, id: WindowId) {
        self.send(WindowCommand::Close(id));
    }

    fn screens(&self, window: Option<WindowId>, reply: ScreenReply) {
        self.send(WindowCommand::Screens(window, reply));
    }
}

struct Entry {
    id: WindowId,
    window: Arc<Window>,
    events: WindowEvents,
    resize: Arc<LiveResize>,
    locked: bool,
    recenter: bool,
    mode: WindowMode,
    resizable: bool,
    size_lock: Option<LogicalSize<f64>>,
}

fn key_name(code: KeyCode) -> &'static str {
    match code {
        KeyCode::KeyA => "A",
        KeyCode::KeyB => "B",
        KeyCode::KeyC => "C",
        KeyCode::KeyD => "D",
        KeyCode::KeyE => "E",
        KeyCode::KeyF => "F",
        KeyCode::KeyG => "G",
        KeyCode::KeyH => "H",
        KeyCode::KeyI => "I",
        KeyCode::KeyJ => "J",
        KeyCode::KeyK => "K",
        KeyCode::KeyL => "L",
        KeyCode::KeyM => "M",
        KeyCode::KeyN => "N",
        KeyCode::KeyO => "O",
        KeyCode::KeyP => "P",
        KeyCode::KeyQ => "Q",
        KeyCode::KeyR => "R",
        KeyCode::KeyS => "S",
        KeyCode::KeyT => "T",
        KeyCode::KeyU => "U",
        KeyCode::KeyV => "V",
        KeyCode::KeyW => "W",
        KeyCode::KeyX => "X",
        KeyCode::KeyY => "Y",
        KeyCode::KeyZ => "Z",
        KeyCode::Digit0 => "Zero",
        KeyCode::Digit1 => "One",
        KeyCode::Digit2 => "Two",
        KeyCode::Digit3 => "Three",
        KeyCode::Digit4 => "Four",
        KeyCode::Digit5 => "Five",
        KeyCode::Digit6 => "Six",
        KeyCode::Digit7 => "Seven",
        KeyCode::Digit8 => "Eight",
        KeyCode::Digit9 => "Nine",
        KeyCode::F1 => "F1",
        KeyCode::F2 => "F2",
        KeyCode::F3 => "F3",
        KeyCode::F4 => "F4",
        KeyCode::F5 => "F5",
        KeyCode::F6 => "F6",
        KeyCode::F7 => "F7",
        KeyCode::F8 => "F8",
        KeyCode::F9 => "F9",
        KeyCode::F10 => "F10",
        KeyCode::F11 => "F11",
        KeyCode::F12 => "F12",
        KeyCode::F13 => "F13",
        KeyCode::F14 => "F14",
        KeyCode::F15 => "F15",
        KeyCode::F16 => "F16",
        KeyCode::F17 => "F17",
        KeyCode::F18 => "F18",
        KeyCode::F19 => "F19",
        KeyCode::F20 => "F20",
        KeyCode::F21 => "F21",
        KeyCode::F22 => "F22",
        KeyCode::F23 => "F23",
        KeyCode::F24 => "F24",
        KeyCode::Space => "Space",
        KeyCode::Enter => "Return",
        KeyCode::Escape => "Escape",
        KeyCode::Tab => "Tab",
        KeyCode::Backspace => "Backspace",
        KeyCode::Delete => "Delete",
        KeyCode::Insert => "Insert",
        KeyCode::Home => "Home",
        KeyCode::End => "End",
        KeyCode::PageUp => "PageUp",
        KeyCode::PageDown => "PageDown",
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Down",
        KeyCode::ArrowLeft => "Left",
        KeyCode::ArrowRight => "Right",
        KeyCode::ShiftLeft => "LeftShift",
        KeyCode::ShiftRight => "RightShift",
        KeyCode::ControlLeft => "LeftControl",
        KeyCode::ControlRight => "RightControl",
        KeyCode::AltLeft => "LeftAlt",
        KeyCode::AltRight => "RightAlt",
        KeyCode::SuperLeft => "LeftSuper",
        KeyCode::SuperRight => "RightSuper",
        KeyCode::CapsLock => "CapsLock",
        KeyCode::NumLock => "NumLock",
        KeyCode::ScrollLock => "ScrollLock",
        KeyCode::PrintScreen => "PrintScreen",
        KeyCode::Pause => "Pause",
        KeyCode::ContextMenu => "Menu",
        KeyCode::Backquote => "Backquote",
        KeyCode::Minus => "Minus",
        KeyCode::Equal => "Equals",
        KeyCode::BracketLeft => "LeftBracket",
        KeyCode::BracketRight => "RightBracket",
        KeyCode::Backslash => "Backslash",
        KeyCode::Semicolon => "Semicolon",
        KeyCode::Quote => "Quote",
        KeyCode::Comma => "Comma",
        KeyCode::Period => "Period",
        KeyCode::Slash => "Slash",
        KeyCode::Numpad0 => "KeypadZero",
        KeyCode::Numpad1 => "KeypadOne",
        KeyCode::Numpad2 => "KeypadTwo",
        KeyCode::Numpad3 => "KeypadThree",
        KeyCode::Numpad4 => "KeypadFour",
        KeyCode::Numpad5 => "KeypadFive",
        KeyCode::Numpad6 => "KeypadSix",
        KeyCode::Numpad7 => "KeypadSeven",
        KeyCode::Numpad8 => "KeypadEight",
        KeyCode::Numpad9 => "KeypadNine",
        KeyCode::NumpadDecimal => "KeypadPeriod",
        KeyCode::NumpadDivide => "KeypadDivide",
        KeyCode::NumpadMultiply => "KeypadMultiply",
        KeyCode::NumpadSubtract => "KeypadMinus",
        KeyCode::NumpadAdd => "KeypadPlus",
        KeyCode::NumpadEnter => "KeypadEnter",
        KeyCode::NumpadEqual => "KeypadEquals",
        _ => "Unknown",
    }
}

fn mouse_button(button: MouseButton) -> Option<&'static str> {
    match button {
        MouseButton::Left => Some("Left"),
        MouseButton::Right => Some("Right"),
        MouseButton::Middle => Some("Middle"),
        MouseButton::Back => Some("Back"),
        MouseButton::Forward => Some("Forward"),
        MouseButton::Other(_) => None,
    }
}

fn cursor_icon(name: &str) -> CursorIcon {
    match name {
        "Pointer" => CursorIcon::Pointer,
        "Text" => CursorIcon::Text,
        "Crosshair" => CursorIcon::Crosshair,
        "Wait" => CursorIcon::Wait,
        "Progress" => CursorIcon::Progress,
        "Move" => CursorIcon::Move,
        "NotAllowed" => CursorIcon::NotAllowed,
        "Grab" => CursorIcon::Grab,
        "Grabbing" => CursorIcon::Grabbing,
        "Help" => CursorIcon::Help,
        "ResizeHorizontal" => CursorIcon::EwResize,
        "ResizeVertical" => CursorIcon::NsResize,
        "ResizeDiagonalDown" => CursorIcon::NwseResize,
        "ResizeDiagonalUp" => CursorIcon::NeswResize,
        "ZoomIn" => CursorIcon::ZoomIn,
        "ZoomOut" => CursorIcon::ZoomOut,
        "Cell" => CursorIcon::Cell,
        "Copy" => CursorIcon::Copy,
        "ContextMenu" => CursorIcon::ContextMenu,
        _ => CursorIcon::Default,
    }
}

#[cfg(windows)]
fn work_area(monitor: &MonitorHandle) -> Option<((f64, f64), (f64, f64))> {
    use winit::platform::windows::MonitorHandleExtWindows;

    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct MonitorInfo {
        size: u32,
        monitor: Rect,
        work: Rect,
        flags: u32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfo) -> i32;
    }

    let mut info = MonitorInfo {
        size: size_of::<MonitorInfo>() as u32,
        ..MonitorInfo::default()
    };
    if unsafe { GetMonitorInfoW(monitor.hmonitor(), &mut info) } == 0 {
        return None;
    }
    let scale = monitor.scale_factor();
    let work = &info.work;
    Some((
        (f64::from(work.left) / scale, f64::from(work.top) / scale),
        (f64::from(work.right - work.left) / scale, f64::from(work.bottom - work.top) / scale),
    ))
}

#[cfg(not(windows))]
fn work_area(_: &MonitorHandle) -> Option<((f64, f64), (f64, f64))> {
    None
}

fn screen(monitor: &MonitorHandle, primary: bool, current: bool) -> Screen {
    let scale = monitor.scale_factor();
    let position = monitor.position().to_logical::<f64>(scale);
    let pixels = monitor.size();
    let size = pixels.to_logical::<f64>(scale);
    let (work_position, work_size) =
        work_area(monitor).unwrap_or(((position.x, position.y), (size.width, size.height)));
    Screen {
        name: monitor.name().unwrap_or_default(),
        position: (position.x, position.y),
        size: (size.width, size.height),
        pixel_size: (pixels.width, pixels.height),
        work_position,
        work_size,
        scale,
        refresh_rate: monitor.refresh_rate_millihertz().map(|rate| f64::from(rate) / 1000.0),
        primary,
        current,
    }
}

fn covering(window: &Window, size: LogicalSize<f64>) -> Option<MonitorHandle> {
    let monitor = window.current_monitor()?;
    let wanted = size.to_physical::<f64>(window.scale_factor());
    let screen = monitor.size();
    (wanted.width + 0.5 >= f64::from(screen.width) && wanted.height + 0.5 >= f64::from(screen.height)).then_some(monitor)
}

fn apply_limits(window: &Window, mode: WindowMode, resizable: bool, size_lock: Option<LogicalSize<f64>>) {
    let windowed = matches!(mode, WindowMode::Windowed | WindowMode::Borderless) && window.fullscreen().is_none();
    match size_lock.filter(|_| windowed) {
        Some(size) => {
            window.set_min_inner_size(Some(size));
            window.set_max_inner_size(Some(size));
        }
        None => {
            window.set_min_inner_size(None::<LogicalSize<f64>>);
            window.set_max_inner_size(None::<LogicalSize<f64>>);
        }
    }
    window.set_resizable(resizable && size_lock.is_none());
    let mut buttons = WindowButtons::CLOSE | WindowButtons::MINIMIZE;
    if size_lock.is_none() {
        buttons |= WindowButtons::MAXIMIZE;
    }
    window.set_enabled_buttons(buttons);
}

fn center(window: &Window) -> PhysicalPosition<f64> {
    let size = window.inner_size();
    PhysicalPosition::new(f64::from(size.width) / 2.0, f64::from(size.height) / 2.0)
}

#[cfg(windows)]
mod sizing {
    use std::sync::Arc;
    use std::time::Duration;

    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    use super::LiveResize;

    const WM_SIZE: u32 = 0x0005;
    const WM_ENTERSIZEMOVE: u32 = 0x0231;
    const WM_EXITSIZEMOVE: u32 = 0x0232;
    const WM_NCDESTROY: u32 = 0x0082;
    const SIZE_MINIMIZED: usize = 1;
    const SUBCLASS: usize = 0x006c_7576;
    const FRAME_WAIT: Duration = Duration::from_millis(50);

    type Procedure = unsafe extern "system" fn(isize, u32, usize, isize, usize, usize) -> isize;

    #[link(name = "comctl32")]
    unsafe extern "system" {
        fn SetWindowSubclass(window: isize, procedure: Procedure, id: usize, data: usize) -> i32;
        fn RemoveWindowSubclass(window: isize, procedure: Procedure, id: usize) -> i32;
        fn DefSubclassProc(window: isize, message: u32, wparam: usize, lparam: isize) -> isize;
    }

    struct Watch {
        resize: Arc<LiveResize>,
        ended: Box<dyn Fn() + Send>,
    }

    unsafe extern "system" fn procedure(
        window: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        id: usize,
        data: usize,
    ) -> isize {
        match message {
            WM_ENTERSIZEMOVE => {
                let watch = unsafe { &*(data as *const Watch) };
                watch.resize.set_dragging(true);
            }
            WM_EXITSIZEMOVE => {
                let watch = unsafe { &*(data as *const Watch) };
                if watch.resize.set_dragging(false) {
                    (watch.ended)();
                }
            }
            WM_SIZE => {
                let watch = unsafe { &*(data as *const Watch) };
                let result = unsafe { DefSubclassProc(window, message, wparam, lparam) };
                if watch.resize.is_dragging() && wparam != SIZE_MINIMIZED {
                    let width = (lparam & 0xffff) as u32;
                    let height = ((lparam >> 16) & 0xffff) as u32;
                    if width > 0 && height > 0 {
                        watch.resize.wait_for([width, height], FRAME_WAIT);
                    }
                }
                return result;
            }
            WM_NCDESTROY => unsafe {
                RemoveWindowSubclass(window, procedure, id);
                drop(Box::from_raw(data as *mut Watch));
            },
            _ => {}
        }
        unsafe { DefSubclassProc(window, message, wparam, lparam) }
    }

    pub fn watch(window: &Window, resize: Arc<LiveResize>, ended: Box<dyn Fn() + Send>) {
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let data = Box::into_raw(Box::new(Watch { resize, ended }));
        if unsafe { SetWindowSubclass(handle.hwnd.get(), procedure, SUBCLASS, data as usize) } == 0 {
            drop(unsafe { Box::from_raw(data) });
        }
    }
}

#[derive(Default)]
struct DesktopApp {
    entries: HashMap<OsWindowId, Entry>,
    ids: HashMap<WindowId, OsWindowId>,
    focused: Option<OsWindowId>,
}

fn icon(icon: &WindowIcon) -> Option<Icon> {
    Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height).ok()
}

fn logical_size(window: &Window) -> (f64, f64) {
    let size = window.inner_size().to_logical::<f64>(window.scale_factor());
    (size.width, size.height)
}

fn logical(window: &Window) -> LogicalSize<f64> {
    window.inner_size().to_logical::<f64>(window.scale_factor())
}

fn apply_mode(window: &Window, mode: WindowMode, size: LogicalSize<f64>) {
    match mode {
        WindowMode::Windowed => {
            window.set_fullscreen(None);
            window.set_decorations(true);
            window.set_maximized(false);
            let _ = window.request_inner_size(size);
        }
        WindowMode::Borderless => {
            window.set_decorations(false);
            window.set_maximized(false);
            match covering(window, size) {
                Some(monitor) => window.set_fullscreen(Some(Fullscreen::Borderless(Some(monitor)))),
                None => {
                    window.set_fullscreen(None);
                    let _ = window.request_inner_size(size);
                }
            }
        }
        WindowMode::Maximized => {
            window.set_fullscreen(None);
            window.set_decorations(true);
            window.set_maximized(true);
        }
        WindowMode::FullScreen => {
            window.set_fullscreen(Some(Fullscreen::Borderless(window.current_monitor())));
        }
        WindowMode::ExclusiveFullScreen => {
            let video_mode = window.current_monitor().and_then(|monitor| {
                monitor.video_modes().max_by_key(|video_mode| {
                    let size = video_mode.size();
                    (u64::from(size.width) * u64::from(size.height), video_mode.refresh_rate_millihertz())
                })
            });
            let fullscreen = match video_mode {
                Some(video_mode) => Fullscreen::Exclusive(video_mode),
                None => Fullscreen::Borderless(window.current_monitor()),
            };
            window.set_fullscreen(Some(fullscreen));
        }
    }
}

impl DesktopApp {
    fn open(&mut self, event_loop: &ActiveEventLoop, id: WindowId, settings: WindowSettings, events: WindowEvents) {
        let size = LogicalSize::new(settings.width, settings.height);
        let mut attributes = Window::default_attributes()
            .with_title(settings.title.clone())
            .with_inner_size(size)
            .with_decorations(settings.mode != WindowMode::Borderless)
            .with_resizable(settings.resizable && !settings.size_locked)
            .with_window_icon(settings.icon.as_ref().and_then(icon));
        if let Some((x, y)) = settings.position {
            attributes = attributes.with_position(LogicalPosition::new(x, y));
        }
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window = Arc::new(window);
                if settings.mode != WindowMode::Windowed {
                    apply_mode(&window, settings.mode, size);
                }
                let size_lock = settings.size_locked.then_some(size);
                apply_limits(&window, settings.mode, settings.resizable, size_lock);
                let (width, height) = logical_size(&window);
                let surface = gpu::instance()
                    .create_surface(window.clone())
                    .map_err(|error| format!("cannot draw into the window: {error}"));
                let resize = Arc::new(LiveResize::new());
                let opened = WindowEvent::Opened {
                    width,
                    height,
                    focused: window.has_focus(),
                    target: Some(RenderTarget::Window {
                        window: window.clone(),
                        surface: Arc::new(PendingSurface::new(surface)),
                        resize: resize.clone(),
                    }),
                };
                if events.send((id, opened)).is_err() {
                    return;
                }
                if let Ok(position) = window.outer_position() {
                    let position = position.to_logical::<f64>(window.scale_factor());
                    let _ = events.send((id, WindowEvent::Moved { x: position.x, y: position.y }));
                }
                #[cfg(windows)]
                {
                    let ended = events.clone();
                    sizing::watch(
                        &window,
                        resize.clone(),
                        Box::new(move || {
                            let _ = ended.send((id, WindowEvent::ResizeEnded));
                        }),
                    );
                }
                self.ids.insert(id, window.id());
                self.entries.insert(
                    window.id(),
                    Entry {
                        id,
                        window,
                        events,
                        resize,
                        locked: false,
                        recenter: false,
                        mode: settings.mode,
                        resizable: settings.resizable,
                        size_lock,
                    },
                );
            }
            Err(error) => {
                let _ = events.send((id, WindowEvent::Failed(error.to_string())));
            }
        }
    }

    fn change(&mut self, id: WindowId, change: WindowChange) {
        let Some(entry) = self.ids.get(&id).and_then(|os_id| self.entries.get_mut(os_id)) else {
            return;
        };
        let window = entry.window.clone();
        match change {
            WindowChange::Title(title) => window.set_title(&title),
            WindowChange::Size { width, height } => {
                let size = LogicalSize::new(width, height);
                if entry.size_lock.is_some() {
                    entry.size_lock = Some(size);
                    apply_limits(&window, entry.mode, entry.resizable, entry.size_lock);
                }
                if entry.mode == WindowMode::Borderless {
                    apply_mode(&window, WindowMode::Borderless, size);
                    apply_limits(&window, entry.mode, entry.resizable, entry.size_lock);
                }
                if window.fullscreen().is_none() {
                    let _ = window.request_inner_size(size);
                }
            }
            WindowChange::Mode(mode) => {
                entry.mode = mode;
                apply_mode(&window, mode, entry.size_lock.unwrap_or_else(|| logical(&window)));
                apply_limits(&window, mode, entry.resizable, entry.size_lock);
            }
            WindowChange::Resizable(resizable) => {
                entry.resizable = resizable;
                apply_limits(&window, entry.mode, resizable, entry.size_lock);
            }
            WindowChange::SizeLocked(locked) => {
                entry.size_lock = locked.then(|| logical(&window));
                apply_limits(&window, entry.mode, entry.resizable, entry.size_lock);
            }
            WindowChange::Position { x, y } => {
                if window.fullscreen().is_none() {
                    window.set_outer_position(LogicalPosition::new(x, y));
                }
            }
            WindowChange::Icon(new_icon) => window.set_window_icon(new_icon.as_ref().and_then(icon)),
            WindowChange::CursorIcon(name) => window.set_cursor(cursor_icon(name)),
            WindowChange::CursorVisible(visible) => window.set_cursor_visible(visible),
            WindowChange::CursorLock(mode) => {
                entry.locked = mode == "Locked";
                entry.recenter = false;
                let grab = match mode {
                    "Confined" => CursorGrabMode::Confined,
                    "Locked" => CursorGrabMode::Locked,
                    _ => CursorGrabMode::None,
                };
                if window.set_cursor_grab(grab).is_err() && grab == CursorGrabMode::Locked {
                    entry.recenter = window.set_cursor_grab(CursorGrabMode::Confined).is_ok();
                    let _ = window.set_cursor_position(center(&window));
                }
            }
        }
    }

    fn close(&mut self, id: WindowId) {
        if let Some(os_id) = self.ids.remove(&id) {
            self.entries.remove(&os_id);
        }
    }

    fn screens(&self, event_loop: &ActiveEventLoop, window: Option<WindowId>) -> Vec<Screen> {
        let primary = event_loop.primary_monitor();
        let current = window
            .and_then(|id| self.ids.get(&id))
            .and_then(|os_id| self.entries.get(os_id))
            .and_then(|entry| entry.window.current_monitor());
        event_loop
            .available_monitors()
            .map(|monitor| {
                let is_primary = primary.as_ref() == Some(&monitor);
                let is_current = current.as_ref() == Some(&monitor);
                screen(&monitor, is_primary, is_current)
            })
            .collect()
    }
}

impl ApplicationHandler<WindowCommand> for DesktopApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, command: WindowCommand) {
        match command {
            WindowCommand::Open(id, settings, events) => self.open(event_loop, id, settings, events),
            WindowCommand::Change(id, change) => self.change(id, change),
            WindowCommand::Close(id) => self.close(id),
            WindowCommand::Screens(window, reply) => reply(self.screens(event_loop, window)),
            WindowCommand::Exit => {
                self.entries.clear();
                self.ids.clear();
                event_loop.exit();
            }
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device: DeviceId, event: DeviceEvent) {
        let DeviceEvent::MouseMotion { delta: (x, y) } = event else {
            return;
        };
        let Some(entry) = self.focused.and_then(|focused| self.entries.get(&focused)) else {
            return;
        };
        if entry.locked {
            let scale = entry.window.scale_factor();
            let _ = entry.events.send((
                entry.id,
                WindowEvent::MouseMotion {
                    x: x / scale,
                    y: y / scale,
                },
            ));
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, window_id: OsWindowId, event: OsEvent) {
        if let OsEvent::Focused(focused) = event {
            if focused {
                self.focused = Some(window_id);
            } else if self.focused == Some(window_id) {
                self.focused = None;
            }
        }
        let Some(entry) = self.entries.get(&window_id) else { return };
        let scale = entry.window.scale_factor();
        let forwarded = match event {
            OsEvent::Resized(_) | OsEvent::ScaleFactorChanged { .. } => {
                let (width, height) = logical_size(&entry.window);
                let phase = if !cfg!(windows) {
                    ResizePhase::Settling
                } else if entry.resize.is_dragging() {
                    ResizePhase::Live
                } else {
                    ResizePhase::Done
                };
                WindowEvent::Resized { width, height, phase }
            }
            OsEvent::Focused(focused) => WindowEvent::Focused(focused),
            OsEvent::Moved(position) => {
                let position = position.to_logical::<f64>(scale);
                WindowEvent::Moved {
                    x: position.x,
                    y: position.y,
                }
            }
            OsEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                if !event.repeat {
                    let key = WindowEvent::Key {
                        key: key_name(code),
                        pressed,
                    };
                    if entry.events.send((entry.id, key)).is_err() {
                        return;
                    }
                }
                match event.text {
                    Some(text) if pressed && text.chars().any(|character| !character.is_control()) => {
                        WindowEvent::Text(text.chars().filter(|character| !character.is_control()).collect())
                    }
                    _ => return,
                }
            }
            OsEvent::CursorMoved { position, .. } => {
                if entry.locked {
                    if entry.recenter && self.focused == Some(window_id) {
                        let middle = center(&entry.window);
                        if (position.x - middle.x).abs() > 1.0 || (position.y - middle.y).abs() > 1.0 {
                            let _ = entry.window.set_cursor_position(middle);
                        }
                    }
                    return;
                }
                WindowEvent::MouseMoved {
                    x: position.x / scale,
                    y: position.y / scale,
                }
            }
            OsEvent::CursorEntered { .. } => WindowEvent::MouseInside(true),
            OsEvent::CursorLeft { .. } => WindowEvent::MouseInside(false),
            OsEvent::MouseInput { state, button, .. } => {
                let Some(button) = mouse_button(button) else {
                    return;
                };
                WindowEvent::MouseButton {
                    button,
                    pressed: state == ElementState::Pressed,
                }
            }
            OsEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(x, y) => WindowEvent::MouseWheel {
                    x: f64::from(x),
                    y: f64::from(y),
                },
                MouseScrollDelta::PixelDelta(pixels) => WindowEvent::MouseWheel {
                    x: pixels.x / scale / LINE_PIXELS,
                    y: pixels.y / scale / LINE_PIXELS,
                },
            },
            OsEvent::Touch(touch) => WindowEvent::Touch {
                id: touch.id,
                phase: match touch.phase {
                    OsTouchPhase::Started => TouchPhase::Started,
                    OsTouchPhase::Moved => TouchPhase::Moved,
                    OsTouchPhase::Ended => TouchPhase::Ended,
                    OsTouchPhase::Cancelled => TouchPhase::Cancelled,
                },
                x: touch.location.x / scale,
                y: touch.location.y / scale,
                force: touch.force.map(|force| force.normalized()),
            },
            OsEvent::CloseRequested => WindowEvent::CloseRequested,
            _ => return,
        };
        if entry.events.send((entry.id, forwarded)).is_err() {
            let id = entry.id;
            self.close(id);
        }
    }
}
