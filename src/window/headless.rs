use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use super::{
    Controllers, InputDevices, RenderTarget, ResizePhase, Screen, ScreenReply, WindowChange, WindowEvent, WindowEvents,
    WindowId, WindowSettings, WindowSystem,
};
use crate::audio::AudioSystem;

struct HeadlessWindow {
    settings: WindowSettings,
    events: WindowEvents,
}

pub struct HeadlessWindows {
    windows: Mutex<BTreeMap<WindowId, HeadlessWindow>>,
    rendering: bool,
    controllers: Arc<Controllers>,
    input_devices: Arc<InputDevices>,
    audio: OnceLock<Arc<AudioSystem>>,
    screens: Mutex<Vec<Screen>>,
}

impl Default for HeadlessWindows {
    fn default() -> Self {
        Self {
            windows: Mutex::new(BTreeMap::new()),
            rendering: false,
            controllers: Arc::default(),
            input_devices: InputDevices::simulated(),
            audio: OnceLock::new(),
            screens: Mutex::new(vec![Screen {
                name: "Headless".to_owned(),
                position: (0.0, 0.0),
                size: (1920.0, 1080.0),
                pixel_size: (1920, 1080),
                work_position: (0.0, 0.0),
                work_size: (1920.0, 1040.0),
                scale: 1.0,
                refresh_rate: Some(60.0),
                primary: true,
                current: false,
            }]),
        }
    }
}

impl HeadlessWindows {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_rendering() -> Self {
        Self {
            rendering: true,
            ..Self::default()
        }
    }

    pub fn set_screens(&self, screens: Vec<Screen>) {
        *self.screens.lock().unwrap_or_else(PoisonError::into_inner) = screens;
    }

    pub fn windows(&self) -> Vec<(WindowId, WindowSettings)> {
        self.windows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(id, window)| (*id, window.settings.clone()))
            .collect()
    }

    pub fn find(&self, title: &str) -> Option<WindowId> {
        self.windows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(_, window)| window.settings.title == title)
            .map(|(id, _)| *id)
    }

    pub fn simulate(&self, id: WindowId, event: WindowEvent) -> bool {
        let windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        windows
            .get(&id)
            .is_some_and(|window| window.events.send((id, event)).is_ok())
    }
}

impl WindowSystem for HeadlessWindows {
    fn open(&self, id: WindowId, settings: WindowSettings, events: WindowEvents) {
        let _ = events.send((
            id,
            WindowEvent::Opened {
                width: settings.width,
                height: settings.height,
                focused: true,
                target: self.rendering.then_some(RenderTarget::Offscreen),
            },
        ));
        let (x, y) = settings.position.unwrap_or((0.0, 0.0));
        let _ = events.send((id, WindowEvent::Moved { x, y }));
        self.windows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, HeadlessWindow { settings, events });
    }

    fn change(&self, id: WindowId, change: WindowChange) {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(window) = windows.get_mut(&id) else { return };
        match change {
            WindowChange::Title(title) => window.settings.title = title,
            WindowChange::Size { width, height } => {
                window.settings.width = width;
                window.settings.height = height;
                let _ = window.events.send((
                    id,
                    WindowEvent::Resized {
                        width,
                        height,
                        phase: ResizePhase::Done,
                    },
                ));
            }
            WindowChange::Mode(mode) => window.settings.mode = mode,
            WindowChange::Resizable(resizable) => window.settings.resizable = resizable,
            WindowChange::Icon(icon) => window.settings.icon = icon,
            WindowChange::CursorIcon(icon) => window.settings.cursor_icon = icon,
            WindowChange::CursorVisible(visible) => window.settings.cursor_visible = visible,
            WindowChange::CursorLock(lock) => window.settings.cursor_lock = lock,
            WindowChange::Position { x, y } => {
                window.settings.position = Some((x, y));
                let _ = window.events.send((id, WindowEvent::Moved { x, y }));
            }
            WindowChange::SizeLocked(locked) => window.settings.size_locked = locked,
        }
    }

    fn close(&self, id: WindowId) {
        self.windows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
    }

    fn screens(&self, window: Option<WindowId>, reply: ScreenReply) {
        let open = window.is_some_and(|id| {
            self.windows
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains_key(&id)
        });
        let screens = self
            .screens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .map(|mut screen| {
                screen.current = open && screen.primary;
                screen
            })
            .collect();
        reply(screens);
    }

    fn controllers(&self) -> Arc<Controllers> {
        self.controllers.clone()
    }

    fn input_devices(&self) -> Arc<InputDevices> {
        self.input_devices.clone()
    }

    fn audio(&self) -> Arc<AudioSystem> {
        self.audio.get_or_init(AudioSystem::simulated).clone()
    }
}
