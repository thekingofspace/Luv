use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use mlua::{AnyUserData, IntoLuaMulti, Lua, MultiValue, Result, Table, UserData, UserDataFields, UserDataMethods, Value};
use tokio::time::Instant;

use super::input::{INPUT_APIS, Inputs};
use super::sound::Sounds;
use super::{Asset, BaseGameObject, GameObject, Renderable, Scene, Signal};
use crate::api::load_asset;
use crate::datatypes::enums::WINDOW_TYPE;
use crate::datatypes::{Color, EnumItem, UDim};
use crate::graphics::picture;
use crate::graphics::protocol::{Capture, FrameInfo};
use crate::runtime::{Engine, Scheduler, Tracker};
use crate::window::{
    ResizePhase, WindowChange, WindowEvent, WindowEvents, WindowIcon, WindowId, WindowMode, WindowSettings,
    WindowSystem, next_id,
};

const SETTLE_DELAY: Duration = Duration::from_millis(200);

pub const DEFAULT_FPS: f64 = 60.0;
pub const MAX_FPS: f64 = 1000.0;
pub const DEFAULT_SIZE: (f64, f64) = (1280.0, 720.0);

type Registry = Rc<RefCell<HashMap<WindowId, AnyUserData>>>;

pub struct WindowHost {
    system: Option<Arc<dyn WindowSystem>>,
    events: WindowEvents,
    open: Registry,
}

impl WindowHost {
    pub fn new(system: Option<Arc<dyn WindowSystem>>, events: WindowEvents) -> Self {
        Self {
            system,
            events,
            open: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn get(&self, id: WindowId) -> Option<AnyUserData> {
        self.open.borrow().get(&id).cloned()
    }

    pub fn open_windows(&self) -> usize {
        self.open.borrow().len()
    }
}

struct State {
    title: RefCell<String>,
    width: Cell<f64>,
    height: Cell<f64>,
    fps: Cell<f64>,
    mode: Cell<WindowMode>,
    resizable: Cell<bool>,
    focused: Cell<bool>,
    open: Cell<bool>,
    background: Cell<Color>,
    opened: Instant,
    resizing: Cell<bool>,
    resize_generation: Cell<u64>,
    position: Cell<(f64, f64)>,
    size_locked: Cell<bool>,
}

impl State {
    fn frame(&self, delta: f64) -> FrameInfo {
        let background = self.background.get();
        FrameInfo {
            background: [
                background.r as f32,
                background.g as f32,
                background.b as f32,
                background.a as f32,
            ],
            time: self.opened.elapsed().as_secs_f64(),
            delta,
            width: self.width.get(),
            height: self.height.get(),
        }
    }
}

struct Signals {
    size_changed: AnyUserData,
    window_update: AnyUserData,
    moved: AnyUserData,
    pre_frame: AnyUserData,
    on_frame: AnyUserData,
    after_frame: AnyUserData,
    focus_gained: AnyUserData,
    focus_lost: AnyUserData,
    closed: AnyUserData,
}

impl Signals {
    fn create(lua: &Lua) -> Result<Self> {
        let signal = |name: &str| lua.create_userdata(Signal::named(name));
        Ok(Self {
            size_changed: signal("SizeChanged")?,
            window_update: signal("WindowUpdate")?,
            moved: signal("Moved")?,
            pre_frame: signal("PreFrame")?,
            on_frame: signal("OnFrame")?,
            after_frame: signal("AfterFrame")?,
            focus_gained: signal("FocusGained")?,
            focus_lost: signal("FocusLost")?,
            closed: signal("Closed")?,
        })
    }

    fn all(&self) -> [&AnyUserData; 9] {
        [
            &self.size_changed,
            &self.window_update,
            &self.moved,
            &self.pre_frame,
            &self.on_frame,
            &self.after_frame,
            &self.focus_gained,
            &self.focus_lost,
            &self.closed,
        ]
    }
}

pub struct Window {
    base: BaseGameObject,
    id: WindowId,
    system: Arc<dyn WindowSystem>,
    tracker: Tracker,
    registry: Registry,
    state: Rc<State>,
    signals: Signals,
    icon: Option<Value>,
    scene: Rc<Scene>,
    inputs: Rc<Inputs>,
    sounds: Rc<Sounds>,
    apis: RefCell<HashMap<&'static str, Table>>,
}

pub const WINDOW_APIS: [&str; 6] = ["Renderable", "Input", "Mouse", "Controller", "Touch", "Sound"];

fn clamp_fps(fps: f64) -> Result<f64> {
    if fps.is_nan() || fps <= 0.0 {
        return Err(mlua::Error::runtime("FPS must be a number greater than 0"));
    }
    Ok(fps.min(MAX_FPS))
}

fn finite_position(position: UDim) -> Result<(f64, f64)> {
    if position.x.is_finite() && position.y.is_finite() {
        Ok((position.x, position.y))
    } else {
        Err(mlua::Error::runtime("a window position must hold finite numbers"))
    }
}

fn window_mode(item: EnumItem) -> Result<WindowMode> {
    let item = item.of(WINDOW_TYPE)?;
    WindowMode::from_name(item.name)
        .ok_or_else(|| mlua::Error::runtime(format!("unknown window type '{}'", item.name)))
}

enum IconSource {
    Bytes(Arc<[u8]>, String),
    Asset(String),
    Game(String),
}

type IconBytes = std::result::Result<Option<(Arc<[u8]>, String)>, String>;

fn engine(lua: &Lua) -> Result<Arc<Engine>> {
    lua.app_data_ref::<Arc<Engine>>()
        .map(|engine| engine.clone())
        .ok_or_else(|| mlua::Error::runtime("the luv engine is not running"))
}

pub(super) fn fire(lua: &Lua, signal: &AnyUserData, args: impl IntoLuaMulti) -> Result<()> {
    if !signal.borrow::<Signal>().is_ok_and(|signal| signal.is_listened()) {
        return Ok(());
    }
    Signal::fire(lua, signal, args.into_lua_multi(lua)?)
}

impl Window {
    pub const CLASS_NAME: &'static str = "Window";

    pub fn open(lua: &Lua, config: Option<Table>) -> Result<AnyUserData> {
        let host = lua
            .app_data_ref::<WindowHost>()
            .ok_or_else(|| mlua::Error::runtime("the window system is not running"))?;
        let system = host
            .system
            .clone()
            .ok_or_else(|| mlua::Error::runtime("windows are not available because no display could be opened"))?;
        let engine = lua
            .app_data_ref::<Arc<Engine>>()
            .map(|engine| engine.clone())
            .ok_or_else(|| mlua::Error::runtime("the luv engine is not running"))?;
        let scheduler = Scheduler::get(lua)?;

        let config = match config {
            Some(config) => config,
            None => lua.create_table()?,
        };
        let title = config
            .get::<Option<String>>("Title")?
            .unwrap_or_else(|| engine.game_name().to_owned());
        let (width, height) = config
            .get::<Option<UDim>>("Size")?
            .map_or(DEFAULT_SIZE, |size| (size.x, size.y));
        let fps = match config.get::<Option<f64>>("FPS")? {
            Some(fps) => clamp_fps(fps)?,
            None => DEFAULT_FPS,
        };
        let mode = match config.get::<Option<EnumItem>>("Type")? {
            Some(item) => window_mode(item)?,
            None => WindowMode::Windowed,
        };
        let resizable = config.get::<Option<bool>>("Resizable")?.unwrap_or(true);
        let size_locked = config.get::<Option<bool>>("SizeLocked")?.unwrap_or(false);
        let position = match config.get::<Option<UDim>>("Position")? {
            Some(position) => Some(finite_position(position)?),
            None => None,
        };
        let icon = match config.get::<Value>("Icon")? {
            Value::Nil => None,
            icon => Some(icon),
        };
        let background = config.get::<Option<Color>>("BackgroundColor")?.unwrap_or(Color::BLACK);

        let id = next_id();
        let state = Rc::new(State {
            title: RefCell::new(title.clone()),
            width: Cell::new(width),
            height: Cell::new(height),
            fps: Cell::new(fps),
            mode: Cell::new(mode),
            resizable: Cell::new(resizable),
            focused: Cell::new(false),
            open: Cell::new(true),
            background: Cell::new(background),
            opened: Instant::now(),
            resizing: Cell::new(false),
            resize_generation: Cell::new(0),
            position: Cell::new(position.unwrap_or((0.0, 0.0))),
            size_locked: Cell::new(size_locked),
        });
        let window = Window {
            base: BaseGameObject::new(Self::CLASS_NAME),
            id,
            system: system.clone(),
            tracker: scheduler.tracker().clone(),
            registry: host.open.clone(),
            state,
            signals: Signals::create(lua)?,
            icon: None,
            scene: Scene::new(id, host.events.clone(), scheduler.clone()),
            inputs: Inputs::new(system.clone(), id),
            sounds: Sounds::new(system.clone()),
            apis: RefCell::new(HashMap::new()),
        };
        let userdata = lua.create_userdata(window)?;
        host.open.borrow_mut().insert(id, userdata.clone());
        scheduler.tracker().enter();
        system.open(
            id,
            WindowSettings {
                title,
                width,
                height,
                mode,
                resizable,
                icon: None,
                cursor_icon: "Default",
                cursor_visible: true,
                cursor_lock: "None",
                position,
                size_locked,
            },
            host.events.clone(),
        );
        drop(host);

        if let Some(icon) = icon {
            userdata.borrow_mut::<Window>()?.set_icon(lua, icon)?;
        } else if engine.game_icon().is_some() {
            userdata.borrow::<Window>()?.show_game_icon(lua)?;
        }
        Self::start_frames(lua, &userdata)?;
        Ok(userdata)
    }

    pub fn id(&self) -> WindowId {
        self.id
    }

    pub fn is_open(&self) -> bool {
        self.state.open.get()
    }

    fn set_icon(&mut self, lua: &Lua, icon: Value) -> Result<()> {
        let source = match &icon {
            Value::Nil => {
                self.icon = None;
                return self.show_game_icon(lua);
            }
            Value::UserData(userdata) if userdata.is::<Asset>() => {
                let asset = userdata.borrow::<Asset>()?;
                IconSource::Bytes(asset.data()?, asset.path().to_owned())
            }
            Value::String(path) => IconSource::Asset(path.to_str()?.to_string()),
            other => {
                return Err(mlua::Error::runtime(format!(
                    "Icon must be an Asset or an asset path, got {}",
                    other.type_name()
                )));
            }
        };
        self.icon = Some(icon);
        self.load_icon(lua, source)
    }

    fn show_game_icon(&self, lua: &Lua) -> Result<()> {
        match engine(lua)?.game_icon() {
            Some(path) => self.load_icon(lua, IconSource::Game(path.to_owned())),
            None => {
                self.system.change(self.id, WindowChange::Icon(None));
                Ok(())
            }
        }
    }

    fn load_icon(&self, lua: &Lua, source: IconSource) -> Result<()> {
        let engine = engine(lua)?;
        let scheduler = Scheduler::get(lua)?;
        let reporter = scheduler.clone();
        let system = self.system.clone();
        let id = self.id;
        scheduler.spawn_task(async move {
            let loaded: IconBytes = match source {
                IconSource::Bytes(bytes, name) => Ok(Some((bytes, name))),
                IconSource::Asset(path) => load_asset(engine.clone(), path)
                    .await
                    .map(|(name, data)| Some((data, name)))
                    .map_err(|error| error.to_string()),
                IconSource::Game(path) => {
                    let engine = engine.clone();
                    tokio::task::spawn_blocking(move || {
                        engine.vfs().read(&path).ok().map(|data| (Arc::<[u8]>::from(data), path))
                    })
                    .await
                    .map_err(|error| error.to_string())
                }
            };
            let decoded = match loaded {
                Ok(Some((bytes, name))) => tokio::task::spawn_blocking(move || {
                    picture::decode(&bytes, Some(&name))
                        .map(|(width, height, rgba)| Some(WindowIcon { width, height, rgba }))
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string())),
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            };
            match decoded {
                Ok(icon) => system.change(id, WindowChange::Icon(icon)),
                Err(error) => reporter.report(mlua::Error::runtime(format!("cannot use the window icon: {error}"))),
            }
        });
        Ok(())
    }

    pub fn scene(&self) -> &Rc<Scene> {
        &self.scene
    }

    pub async fn capture(window: &AnyUserData) -> Result<Capture> {
        let (scene, frame) = {
            let this = window.borrow::<Window>()?;
            (this.scene.clone(), this.state.frame(0.0))
        };
        scene.capture(frame).await.map_err(mlua::Error::runtime)
    }

    fn api(&self, lua: &Lua, name: &str) -> Result<Value> {
        if !self.state.open.get() {
            return Err(mlua::Error::runtime(format!(
                "the {name} API is not available because the window is closed"
            )));
        }
        let Some(name) = WINDOW_APIS.into_iter().find(|api| *api == name) else {
            return Err(mlua::Error::runtime(format!(
                "'{name}' is not a window API, the available APIs are {}",
                WINDOW_APIS.join(", ")
            )));
        };
        if INPUT_APIS.contains(&name) {
            return Ok(self.inputs.api(lua, name)?.map_or(Value::Nil, Value::UserData));
        }
        if name == "Sound" {
            return Ok(Value::UserData(self.sounds.api(lua)?));
        }
        if let Some(api) = self.apis.borrow().get(name) {
            return Ok(Value::Table(api.clone()));
        }
        let api = crate::api::renderable::create(lua, &self.scene)?;
        self.apis.borrow_mut().insert(name, api.clone());
        Ok(Value::Table(api))
    }

    fn start_frames(lua: &Lua, window: &AnyUserData) -> Result<()> {
        let (state, frames, scene) = {
            let this = window.borrow::<Window>()?;
            (
                this.state.clone(),
                [
                    this.signals.pre_frame.clone(),
                    this.signals.on_frame.clone(),
                    this.signals.after_frame.clone(),
                ],
                this.scene.clone(),
            )
        };
        let lua = lua.clone();
        let scheduler = Scheduler::get(&lua)?;
        tokio::task::spawn_local(async move {
            let mut last = Instant::now();
            let mut next = last;
            while state.open.get() {
                next += Duration::from_secs_f64(1.0 / state.fps.get());
                let now = Instant::now();
                if next < now {
                    next = now;
                }
                tokio::time::sleep_until(next).await;
                if !state.open.get() {
                    break;
                }
                let now = Instant::now();
                let delta = now.duration_since(last).as_secs_f64();
                last = now;
                for signal in &frames {
                    if let Err(error) = fire(&lua, signal, delta) {
                        scheduler.report(error);
                    }
                }
                if state.open.get() {
                    scene.present(state.frame(delta));
                }
            }
        });
        Ok(())
    }

    pub fn handle_event(lua: &Lua, window: &AnyUserData, event: WindowEvent) -> Result<()> {
        match event {
            WindowEvent::Opened {
                width,
                height,
                focused,
                target,
            } => {
                let scene = window.borrow::<Window>()?.scene.clone();
                scene.attach(target);
                Self::resized(lua, window, width, height, ResizePhase::Done)?;
                Self::focus(lua, window, focused)
            }
            WindowEvent::Resized { width, height, phase } => Self::resized(lua, window, width, height, phase),
            WindowEvent::ResizeEnded => Self::finish_resize(lua, window),
            WindowEvent::Moved { x, y } => {
                let (state, moved) = {
                    let this = window.borrow::<Window>()?;
                    (this.state.clone(), this.signals.moved.clone())
                };
                if state.position.replace((x, y)) == (x, y) {
                    return Ok(());
                }
                fire(lua, &moved, UDim::new(x, y, 0.0))
            }
            WindowEvent::Focused(focused) => Self::focus(lua, window, focused),
            WindowEvent::CloseRequested => Self::close(lua, window),
            WindowEvent::Failed(message) => {
                Self::close(lua, window)?;
                Err(mlua::Error::runtime(format!("the window could not be opened: {message}")))
            }
            WindowEvent::RenderError(message) => Err(mlua::Error::runtime(message)),
            input => {
                let inputs = window.borrow::<Window>()?.inputs.clone();
                inputs.handle(lua, input)
            }
        }
    }

    fn resized(lua: &Lua, window: &AnyUserData, width: f64, height: f64, phase: ResizePhase) -> Result<()> {
        let (state, update, changed) = {
            let this = window.borrow::<Window>()?;
            (
                this.state.clone(),
                this.signals.window_update.clone(),
                this.signals.size_changed.clone(),
            )
        };
        if state.width.get() == width && state.height.get() == height {
            if phase == ResizePhase::Done {
                return Self::finish_resize(lua, window);
            }
            return Ok(());
        }
        state.width.set(width);
        state.height.set(height);
        let size = UDim::new(width, height, 0.0);
        let updated = fire(lua, &update, size);
        let settled = match phase {
            ResizePhase::Done => {
                state.resizing.set(false);
                fire(lua, &changed, size)
            }
            ResizePhase::Live => {
                state.resizing.set(true);
                Ok(())
            }
            ResizePhase::Settling => {
                state.resizing.set(true);
                let generation = state.resize_generation.get() + 1;
                state.resize_generation.set(generation);
                let lua = lua.clone();
                tokio::task::spawn_local(async move {
                    tokio::time::sleep(SETTLE_DELAY).await;
                    if state.resize_generation.get() != generation || !state.resizing.replace(false) {
                        return;
                    }
                    let size = UDim::new(state.width.get(), state.height.get(), 0.0);
                    if let Err(error) = fire(&lua, &changed, size)
                        && let Ok(scheduler) = Scheduler::get(&lua)
                    {
                        scheduler.report(error);
                    }
                });
                Ok(())
            }
        };
        updated.and(settled)
    }

    fn finish_resize(lua: &Lua, window: &AnyUserData) -> Result<()> {
        let (state, changed) = {
            let this = window.borrow::<Window>()?;
            (this.state.clone(), this.signals.size_changed.clone())
        };
        if !state.resizing.replace(false) {
            return Ok(());
        }
        state.resize_generation.set(state.resize_generation.get() + 1);
        fire(lua, &changed, UDim::new(state.width.get(), state.height.get(), 0.0))
    }

    fn focus(lua: &Lua, window: &AnyUserData, focused: bool) -> Result<()> {
        let (signal, inputs) = {
            let this = window.borrow::<Window>()?;
            if this.state.focused.get() == focused {
                return Ok(());
            }
            this.state.focused.set(focused);
            this.sounds.focus(focused);
            let signal = if focused {
                this.signals.focus_gained.clone()
            } else {
                this.signals.focus_lost.clone()
            };
            (signal, this.inputs.clone())
        };
        let released = inputs.focus(lua, focused);
        fire(lua, &signal, MultiValue::new()).and(released)
    }

    pub fn close(lua: &Lua, window: &AnyUserData) -> Result<()> {
        let (closed, tracker) = {
            let this = window.borrow::<Window>()?;
            if !this.shut() {
                return Ok(());
            }
            (this.signals.closed.clone(), this.tracker.clone())
        };
        let fired = fire(lua, &closed, MultiValue::new());
        tracker.exit();
        fired
    }

    fn shut(&self) -> bool {
        if !self.state.open.get() {
            return false;
        }
        self.state.open.set(false);
        self.scene.close();
        self.inputs.close();
        self.sounds.close();
        self.apis.borrow_mut().clear();
        self.system.close(self.id);
        self.registry.borrow_mut().remove(&self.id);
        true
    }
}

impl GameObject for Window {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.inputs.destroy();
        if self.shut() {
            self.tracker.exit();
        }
        for signal in self.signals.all() {
            if let Ok(mut signal) = signal.borrow_mut::<Signal>() {
                signal.destroy();
            }
        }
    }
}

impl UserData for Window {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);

        fields.add_field_method_get("Title", |_, this| Ok(this.state.title.borrow().clone()));
        fields.add_field_method_set("Title", |_, this, title: String| {
            this.state.title.replace(title.clone());
            this.system.change(this.id, WindowChange::Title(title));
            Ok(())
        });

        fields.add_field_method_get("Size", |_, this| {
            Ok(UDim::new(this.state.width.get(), this.state.height.get(), 0.0))
        });
        fields.add_field_method_set("Size", |_, this, size: UDim| {
            if !(size.x > 0.0 && size.y > 0.0) {
                return Err(mlua::Error::runtime("window sizes must be greater than 0"));
            }
            this.system.change(
                this.id,
                WindowChange::Size {
                    width: size.x,
                    height: size.y,
                },
            );
            Ok(())
        });

        fields.add_field_method_get("FPS", |_, this| Ok(this.state.fps.get()));
        fields.add_field_method_set("FPS", |_, this, fps: f64| {
            this.state.fps.set(clamp_fps(fps)?);
            Ok(())
        });

        fields.add_field_method_get("Type", |lua, this| {
            EnumItem::find(WINDOW_TYPE, this.state.mode.get().name())
                .map_or(Ok(Value::Nil), |item| item.canonical(lua))
        });
        fields.add_field_method_set("Type", |_, this, item: EnumItem| {
            let mode = window_mode(item)?;
            this.state.mode.set(mode);
            this.system.change(this.id, WindowChange::Mode(mode));
            Ok(())
        });

        fields.add_field_method_get("Position", |_, this| {
            let (x, y) = this.state.position.get();
            Ok(UDim::new(x, y, 0.0))
        });
        fields.add_field_method_set("Position", |_, this, position: UDim| {
            let (x, y) = finite_position(position)?;
            this.system.change(this.id, WindowChange::Position { x, y });
            Ok(())
        });

        fields.add_field_method_get("SizeLocked", |_, this| Ok(this.state.size_locked.get()));
        fields.add_field_method_set("SizeLocked", |_, this, locked: bool| {
            if this.state.size_locked.replace(locked) != locked {
                this.system.change(this.id, WindowChange::SizeLocked(locked));
            }
            Ok(())
        });

        fields.add_field_method_get("Resizable", |_, this| Ok(this.state.resizable.get()));
        fields.add_field_method_set("Resizable", |_, this, resizable: bool| {
            this.state.resizable.set(resizable);
            this.system.change(this.id, WindowChange::Resizable(resizable));
            Ok(())
        });

        fields.add_field_method_get("Icon", |_, this| Ok(this.icon.clone()));
        fields.add_field_method_set("Icon", |lua, this, icon: Value| this.set_icon(lua, icon));

        fields.add_field_method_get("BackgroundColor", |_, this| Ok(this.state.background.get()));
        fields.add_field_method_set("BackgroundColor", |_, this, color: Color| {
            this.state.background.set(color);
            Ok(())
        });

        fields.add_field_method_get("Focused", |_, this| Ok(this.state.focused.get()));
        fields.add_field_method_get("IsOpen", |_, this| Ok(this.state.open.get()));

        fields.add_field_method_get("SizeChanged", |_, this| Ok(this.signals.size_changed.clone()));
        fields.add_field_method_get("WindowUpdate", |_, this| Ok(this.signals.window_update.clone()));
        fields.add_field_method_get("Moved", |_, this| Ok(this.signals.moved.clone()));
        fields.add_field_method_get("PreFrame", |_, this| Ok(this.signals.pre_frame.clone()));
        fields.add_field_method_get("OnFrame", |_, this| Ok(this.signals.on_frame.clone()));
        fields.add_field_method_get("AfterFrame", |_, this| Ok(this.signals.after_frame.clone()));
        fields.add_field_method_get("FocusGained", |_, this| Ok(this.signals.focus_gained.clone()));
        fields.add_field_method_get("FocusLost", |_, this| Ok(this.signals.focus_lost.clone()));
        fields.add_field_method_get("Closed", |_, this| Ok(this.signals.closed.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_method("GetAPI", |lua, this, name: String| this.api(lua, &name));
        methods.add_method("AddPostProcess", |lua, this, shader: Option<AnyUserData>| {
            if !this.state.open.get() {
                return Err(mlua::Error::runtime("post processes cannot be added because the window is closed"));
            }
            Renderable::create_post(&this.scene, lua, shader)
        });
        methods.add_method("GetPostProcesses", |lua, this, ()| lua.create_sequence_from(this.scene.post_processes()));
        methods.add_method("ClearPostProcesses", |_, this, ()| {
            for post in this.scene.post_processes() {
                if let Ok(mut post) = post.borrow_mut::<Renderable>() {
                    post.destroy();
                }
            }
            Ok(())
        });
        methods.add_function("Close", |lua, window: AnyUserData| Window::close(lua, &window));
    }
}
