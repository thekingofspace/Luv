pub mod aliases;
mod bus;
mod containers;
mod engine;
pub(crate) mod imports;
mod require;
mod scheduler;

pub use bus::{Bus, Mailbox, Message, Packet, Payload, decode, encode, encode_value};
pub use containers::{Containers, LoadedContainer};
pub use engine::{AssetCache, Engine, EngineBuilder, WeakCache};
pub use require::{VfsRequirer, module_of};
pub use scheduler::{Activity, Scheduler, Tracker, Wait, Waiter};
pub(crate) use require::CONFIG_FILES;

use std::cell::RefCell;
use std::io::Write;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use mlua::chunk::{ChunkMode, Compiler};
use mlua::{AnyUserData, Function, IntoLuaMulti, Lua, LuaString, MultiValue, Result, Value};
use tokio::sync::mpsc;
use tokio::task::LocalSet;
use tokio::time::{self, MissedTickBehavior, Sleep};

use crate::api;
use crate::datatypes;
use crate::objects::{Messenger, Signal, Window, WindowHost};
use crate::script::{self, ENTER, EXIT, HOOK};
use crate::vfs::Vfs;
use crate::window::{WindowEvent, WindowId};

pub const THREAD_STACK_SIZE: usize = 16 * 1024 * 1024;
pub const HEARTBEAT_RATE: f64 = 60.0;

#[cfg(windows)]
fn raise_timer_resolution() {
    #[link(name = "winmm")]
    unsafe extern "system" {
        fn timeBeginPeriod(period: u32) -> u32;
    }
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        timeBeginPeriod(1);
    });
}

#[cfg(not(windows))]
fn raise_timer_resolution() {}

pub fn compiler() -> Compiler {
    Compiler::new()
        .set_optimization_level(2)
        .set_debug_level(1)
        .set_type_info_level(1)
}

pub fn load_unit(lua: &Lua, vfs: &dyn Vfs, path: &str, unit: usize) -> Result<Function> {
    let data = vfs.read(path).map_err(mlua::Error::external)?;
    let mode = vfs.chunk_mode(path);
    let missing = || mlua::Error::runtime(format!("{path} does not contain parallel block #{unit}"));
    let code = match mode {
        ChunkMode::Binary => script::bundle_unit(&data, unit).ok_or_else(missing)?.to_vec(),
        ChunkMode::Text => script::units(&data)
            .map_err(|error| mlua::Error::SyntaxError {
                message: format!("{path}:{error}"),
                incomplete_input: false,
            })?
            .into_iter()
            .nth(unit)
            .ok_or_else(missing)?,
    };
    lua.load(code)
        .set_name(format!("@{path}"))
        .set_mode(mode)
        .set_compiler(compiler())
        .into_function()
}

pub fn import(lua: &Lua, name: &str) -> Result<Value> {
    imports::get(lua, name)
}

pub const CLOSE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct CloseCallbacks(RefCell<Vec<Function>>);

impl CloseCallbacks {
    pub fn push(&self, callback: Function) {
        self.0.borrow_mut().push(callback);
    }

    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    fn take(&self) -> Vec<Function> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

enum Tick {
    Finished,
    Closing,
    Deadline,
    Message(Option<Message>),
    Window(Option<(WindowId, WindowEvent)>),
    Idle,
}

pub struct Runtime {
    engine: Arc<Engine>,
    lua: Lua,
    scheduler: Scheduler,
    messenger: AnyUserData,
    heartbeat: AnyUserData,
    window_events: RefCell<Option<mpsc::UnboundedReceiver<(WindowId, WindowEvent)>>>,
}

impl Runtime {
    pub fn new(engine: Arc<Engine>) -> Result<Self> {
        Self::create(engine, None)
    }

    pub(crate) fn parallel(engine: Arc<Engine>, label: String) -> Result<Self> {
        Self::create(engine, Some(label))
    }

    fn create(engine: Arc<Engine>, label: Option<String>) -> Result<Self> {
        raise_timer_resolution();
        let lua = Lua::new();

        let reporter = {
            let engine = engine.clone();
            move |error: mlua::Error| match &label {
                Some(label) => engine.report(&format!("[{label}] {error}")),
                None => engine.report(&error.to_string()),
            }
        };
        let scheduler = Scheduler::new(Tracker::new(engine.bus().activity().clone()), reporter);
        lua.set_app_data(scheduler.clone());
        lua.set_app_data(engine.clone());
        let (window_sender, window_receiver) = mpsc::unbounded_channel();
        lua.set_app_data(WindowHost::new(engine.windows().cloned(), window_sender));
        lua.set_app_data(CloseCallbacks::default());

        let globals = lua.globals();
        let tostring: Function = globals.get("tostring")?;
        globals.set(
            "print",
            lua.create_function(move |_, values: MultiValue| {
                let mut line = Vec::new();
                for (index, value) in values.into_iter().enumerate() {
                    if index > 0 {
                        line.push(b'\t');
                    }
                    line.extend_from_slice(&tostring.call::<LuaString>(value)?.as_bytes());
                }
                line.push(b'\n');
                let _ = std::io::stdout().lock().write_all(&line);
                Ok(())
            })?,
        )?;
        let requirer = VfsRequirer::new(engine.vfs().clone()).with_containers(engine.containers().clone());
        globals.set("require", lua.create_require_function(requirer)?)?;
        let marker = lua.create_function(|_, ()| Ok(()))?;
        globals.set(ENTER, marker.clone())?;
        globals.set(EXIT, marker)?;
        globals.set(HOOK, lua.create_function(parallel_hook)?)?;
        scheduler::install_coroutine_library(&lua)?;
        datatypes::install(&lua)?;

        let messenger = lua.create_userdata(Messenger::new(engine.bus().clone()))?;
        let heartbeat = lua.create_userdata(
            Signal::named("Heartbeat").keep_alive_while_listened(scheduler.tracker().clone()),
        )?;
        let mut libraries = api::libraries(&lua, &engine, &heartbeat)?;
        libraries.push(("Messenger", Value::UserData(messenger.clone())));
        imports::install(&lua, libraries)?;

        engine.apply_setup(&lua)?;

        Ok(Self {
            engine,
            lua,
            scheduler,
            messenger,
            heartbeat,
            window_events: RefCell::new(Some(window_receiver)),
        })
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    pub fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }

    pub async fn run(&self, entry: &str) {
        let containers = self.engine.containers().clone();
        let _ = tokio::task::spawn_blocking(move || containers.refresh()).await;
        let mailbox = self.engine.bus().open();
        LocalSet::new()
            .run_until(async {
                self.beat();
                self.scheduler.tracker().enter();
                self.start(entry, 0, MultiValue::new());
                self.drive(mailbox, false).await;
            })
            .await;
        self.engine.join().await;
    }

    pub(crate) async fn run_cluster(&self, mailbox: Mailbox, path: &str, unit: usize, captures: &[Packet]) {
        self.beat();
        self.scheduler.tracker().adopt();
        match decode(&self.lua, captures) {
            Ok(captures) => self.start(path, unit, captures),
            Err(error) => {
                self.scheduler.report(error);
                self.scheduler.tracker().exit();
            }
        }
        self.drive(mailbox, true).await;
    }

    fn start(&self, path: &str, unit: usize, args: impl IntoLuaMulti) {
        match load_unit(&self.lua, self.engine.vfs().as_ref(), path, unit) {
            Ok(function) => self.scheduler.spawn_entered(&self.lua, function, args),
            Err(error) => {
                self.scheduler.report(error);
                self.scheduler.tracker().exit();
            }
        }
    }

    fn beat(&self) {
        let lua = self.lua.clone();
        let heartbeat = self.heartbeat.clone();
        let scheduler = self.scheduler.clone();
        tokio::task::spawn_local(async move {
            let mut interval = time::interval(Duration::from_secs_f64(1.0 / HEARTBEAT_RATE));
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            let mut last = interval.tick().await;
            loop {
                let now = interval.tick().await;
                let delta = now.duration_since(last).as_secs_f64();
                last = now;
                if !heartbeat.borrow::<Signal>().is_ok_and(|signal| signal.is_listened()) {
                    continue;
                }
                let fired = delta
                    .into_lua_multi(&lua)
                    .and_then(|delta| Signal::fire(&lua, &heartbeat, delta));
                if let Err(error) = fired {
                    scheduler.report(error);
                }
            }
        });
    }

    fn listening(&self) -> bool {
        let messages = self
            .messenger
            .borrow::<Messenger>()
            .is_ok_and(|messenger| messenger.is_listening());
        let closing = self
            .lua
            .app_data_ref::<CloseCallbacks>()
            .is_some_and(|callbacks| !callbacks.is_empty());
        messages || closing
    }

    fn run_close_callbacks(&self) {
        let activity = self.engine.bus().activity().clone();
        let callbacks = match self.lua.app_data_ref::<CloseCallbacks>() {
            Some(callbacks) => callbacks.take(),
            None => Vec::new(),
        };
        for callback in callbacks {
            activity.closing_enter(1);
            let activity = activity.clone();
            self.scheduler.spawn_then(&self.lua, callback, (), move || activity.closing_exit());
        }
    }

    fn window_event(&self, id: WindowId, event: WindowEvent) {
        let window = self.lua.app_data_ref::<WindowHost>().and_then(|host| host.get(id));
        if let Some(window) = window
            && let Err(error) = Window::handle_event(&self.lua, &window, event)
        {
            self.scheduler.report(error);
        }
    }

    async fn drive(&self, mut mailbox: Mailbox, retire: bool) {
        let activity = self.engine.bus().activity().clone();
        let tracker = self.scheduler.tracker().clone();
        let mut windows = self.window_events.borrow_mut().take();
        let mut deadline: Option<Pin<Box<Sleep>>> = None;
        loop {
            if activity.is_finished() || (retire && tracker.active() == 0 && !self.listening()) {
                break;
            }
            let watching_close = !retire && deadline.is_none();
            let tick = tokio::select! {
                biased;
                _ = activity.finished() => Tick::Finished,
                _ = activity.closing(), if watching_close => Tick::Closing,
                _ = async {
                    match deadline.as_mut() {
                        Some(deadline) => deadline.await,
                        None => std::future::pending().await,
                    }
                } => Tick::Deadline,
                message = mailbox.receiver.recv() => Tick::Message(message),
                event = async {
                    match windows.as_mut() {
                        Some(windows) => windows.recv().await,
                        None => std::future::pending().await,
                    }
                } => Tick::Window(event),
                _ = tracker.idle(), if retire => Tick::Idle,
            };
            match tick {
                Tick::Finished => break,
                Tick::Closing => deadline = Some(Box::pin(time::sleep(CLOSE_TIMEOUT))),
                Tick::Deadline => {
                    activity.stop();
                    break;
                }
                Tick::Message(Some(Message::Topic { topic, payload })) => {
                    if let Err(error) = Messenger::deliver(&self.lua, &self.messenger, &topic, &payload) {
                        self.scheduler.report(error);
                    }
                    activity.exit();
                    tokio::task::yield_now().await;
                }
                Tick::Message(Some(Message::Close)) => {
                    self.run_close_callbacks();
                    activity.closing_exit();
                }
                Tick::Message(None) => break,
                Tick::Window(Some((id, event))) => self.window_event(id, event),
                Tick::Window(None) => windows = None,
                Tick::Idle => {}
            }
        }

        self.engine.bus().close(mailbox.id);
        mailbox.receiver.close();
        while let Ok(message) = mailbox.receiver.try_recv() {
            match message {
                Message::Topic { .. } => activity.exit(),
                Message::Close => activity.closing_exit(),
            }
        }
    }
}

pub fn caller_path(lua: &Lua) -> Option<String> {
    (1..)
        .map_while(|level| lua.inspect_stack(level, |debug| debug.source().source.map(|source| source.into_owned())))
        .flatten()
        .find_map(|source| source.strip_prefix('@').map(str::to_owned))
}

fn parallel_hook(lua: &Lua, (unit, names, captures): (usize, String, MultiValue)) -> Result<()> {
    let engine = lua
        .app_data_ref::<Arc<Engine>>()
        .map(|engine| engine.clone())
        .ok_or_else(|| mlua::Error::runtime("the luv engine is not running"))?;
    let path = caller_path(lua)
        .ok_or_else(|| mlua::Error::runtime("parallel blocks can only run from workspace scripts"))?;

    let names: Vec<&str> = names.split(',').collect();
    let captures = captures
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            encode_value(lua, value).map_err(|error| {
                mlua::Error::runtime(format!(
                    "cannot pass `{}` into parallel block #{unit}: {error}",
                    names.get(index).copied().unwrap_or("?")
                ))
            })
        })
        .collect::<Result<Vec<_>>>()?;

    engine
        .spawn_parallel(path, unit, captures.into())
        .map_err(mlua::Error::external)
}
