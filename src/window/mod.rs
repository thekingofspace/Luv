mod controllers;
mod desktop;
mod devices;
mod headless;
mod resize;

pub use controllers::{ControllerEvent, ControllerId, ControllerListener, ControllerState, Controllers, Vibration};
pub use desktop::{DesktopWindows, WindowCommand};
pub use devices::{DeviceListener, InputDevices, InputKind};
pub use headless::HeadlessWindows;
pub use resize::LiveResize;

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::mpsc;

use crate::audio::AudioSystem;

pub type WindowId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowMode {
    Windowed,
    Borderless,
    Maximized,
    FullScreen,
    ExclusiveFullScreen,
}

impl WindowMode {
    pub const ALL: [WindowMode; 5] = [
        WindowMode::Windowed,
        WindowMode::Borderless,
        WindowMode::Maximized,
        WindowMode::FullScreen,
        WindowMode::ExclusiveFullScreen,
    ];

    pub fn name(self) -> &'static str {
        match self {
            WindowMode::Windowed => "Windowed",
            WindowMode::Borderless => "Borderless",
            WindowMode::Maximized => "Maximized",
            WindowMode::FullScreen => "FullScreen",
            WindowMode::ExclusiveFullScreen => "ExclusiveFullScreen",
        }
    }

    pub fn from_name(name: &str) -> Option<WindowMode> {
        Self::ALL.into_iter().find(|mode| mode.name() == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowIcon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowSettings {
    pub title: String,
    pub width: f64,
    pub height: f64,
    pub mode: WindowMode,
    pub resizable: bool,
    pub icon: Option<WindowIcon>,
    pub cursor_icon: &'static str,
    pub cursor_visible: bool,
    pub cursor_lock: &'static str,
    pub position: Option<(f64, f64)>,
    pub size_locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub name: String,
    pub position: (f64, f64),
    pub size: (f64, f64),
    pub pixel_size: (u32, u32),
    pub work_position: (f64, f64),
    pub work_size: (f64, f64),
    pub scale: f64,
    pub refresh_rate: Option<f64>,
    pub primary: bool,
    pub current: bool,
}

pub type ScreenReply = Box<dyn FnOnce(Vec<Screen>) + Send>;

#[derive(Clone, Debug, PartialEq)]
pub enum WindowChange {
    Title(String),
    Size { width: f64, height: f64 },
    Mode(WindowMode),
    Resizable(bool),
    Icon(Option<WindowIcon>),
    CursorIcon(&'static str),
    CursorVisible(bool),
    CursorLock(&'static str),
    Position { x: f64, y: f64 },
    SizeLocked(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

pub struct PendingSurface(Mutex<Option<Result<wgpu::Surface<'static>, String>>>);

impl PendingSurface {
    pub fn new(surface: Result<wgpu::Surface<'static>, String>) -> Self {
        Self(Mutex::new(Some(surface)))
    }

    pub fn take(&self) -> Result<wgpu::Surface<'static>, String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .unwrap_or_else(|| Err("the window surface is already in use".to_owned()))
    }
}

#[derive(Clone)]
pub enum RenderTarget {
    Window {
        window: Arc<winit::window::Window>,
        surface: Arc<PendingSurface>,
        resize: Arc<LiveResize>,
    },
    Offscreen,
}

impl fmt::Debug for RenderTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderTarget::Window { .. } => formatter.write_str("Window"),
            RenderTarget::Offscreen => formatter.write_str("Offscreen"),
        }
    }
}

impl PartialEq for RenderTarget {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (RenderTarget::Window { window: first, .. }, RenderTarget::Window { window: second, .. }) => {
                Arc::ptr_eq(first, second)
            }
            (RenderTarget::Offscreen, RenderTarget::Offscreen) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizePhase {
    Done,
    Live,
    Settling,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    Opened {
        width: f64,
        height: f64,
        focused: bool,
        target: Option<RenderTarget>,
    },
    Resized {
        width: f64,
        height: f64,
        phase: ResizePhase,
    },
    ResizeEnded,
    Moved {
        x: f64,
        y: f64,
    },
    Key {
        key: &'static str,
        pressed: bool,
    },
    Text(String),
    MouseMoved {
        x: f64,
        y: f64,
    },
    MouseMotion {
        x: f64,
        y: f64,
    },
    MouseButton {
        button: &'static str,
        pressed: bool,
    },
    MouseWheel {
        x: f64,
        y: f64,
    },
    MouseInside(bool),
    Touch {
        id: u64,
        phase: TouchPhase,
        x: f64,
        y: f64,
        force: Option<f64>,
    },
    Focused(bool),
    CloseRequested,
    Failed(String),
    RenderError(String),
}

pub type WindowEvents = mpsc::UnboundedSender<(WindowId, WindowEvent)>;

pub trait WindowSystem: Send + Sync {
    fn open(&self, id: WindowId, settings: WindowSettings, events: WindowEvents);

    fn change(&self, id: WindowId, change: WindowChange);

    fn close(&self, id: WindowId);

    fn screens(&self, window: Option<WindowId>, reply: ScreenReply);

    fn controllers(&self) -> Arc<Controllers> {
        Controllers::system()
    }

    fn input_devices(&self) -> Arc<InputDevices> {
        InputDevices::system()
    }

    fn audio(&self) -> Arc<AudioSystem> {
        AudioSystem::system()
    }
}

static NEXT_WINDOW: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> WindowId {
    NEXT_WINDOW.fetch_add(1, Ordering::Relaxed)
}
