use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::mem;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use mlua::Lua;
use tokio::sync::Notify;
use tokio::task::LocalSet;

use super::{Runtime, THREAD_STACK_SIZE};
use super::bus::{Bus, Mailbox, Packet, Payload};
use super::containers::Containers;
use super::scheduler::Activity;
use crate::audio::Pcm;
use crate::vfs::{LayeredVfs, Vfs};
use crate::window::WindowSystem;

const TEMP_TRIES: u32 = 64;

static TEMP_COUNT: AtomicU64 = AtomicU64::new(0);

fn temp_name() -> String {
    format!("luv-{}-{}", std::process::id(), TEMP_COUNT.fetch_add(1, Ordering::Relaxed))
}

type Setup = Arc<dyn Fn(&Lua) -> mlua::Result<()> + Send + Sync>;
type Reporter = Arc<dyn Fn(&str) + Send + Sync>;

pub struct WeakCache<T: ?Sized> {
    entries: Mutex<HashMap<String, Weak<T>>>,
}

pub type AssetCache = WeakCache<[u8]>;

impl<T: ?Sized> Default for WeakCache<T> {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl<T: ?Sized> WeakCache<T> {
    pub fn get(&self, path: &str) -> Option<Arc<T>> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(path)
            .and_then(Weak::upgrade)
    }

    pub fn share(&self, path: &str, data: Arc<T>) -> Arc<T> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(existing) = entries.get(path).and_then(Weak::upgrade) {
            return existing;
        }
        entries.retain(|_, entry| entry.strong_count() > 0);
        entries.insert(path.to_owned(), Arc::downgrade(&data));
        data
    }

    pub fn resident(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .filter(|entry| entry.strong_count() > 0)
            .count()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BootScripts {
    pub start: Vec<String>,
    pub start_async: Vec<String>,
    pub boot: Vec<String>,
    pub boot_ready: Vec<String>,
}

impl BootScripts {
    pub fn all(&self) -> impl Iterator<Item = &String> {
        self.start
            .iter()
            .chain(&self.start_async)
            .chain(&self.boot)
            .chain(&self.boot_ready)
    }

    pub fn is_empty(&self) -> bool {
        self.all().next().is_none()
    }
}

pub struct EngineBuilder {
    vfs: Arc<dyn Vfs>,
    boot: BootScripts,
    setup: Vec<Setup>,
    reporter: Reporter,
    args: Vec<String>,
    game_name: String,
    game_icon: Option<String>,
    game_dir: Option<PathBuf>,
    library_dirs: Vec<PathBuf>,
    container_dirs: Vec<PathBuf>,
    windows: Option<Arc<dyn WindowSystem>>,
}

impl EngineBuilder {
    pub fn setup(mut self, setup: impl Fn(&Lua) -> mlua::Result<()> + Send + Sync + 'static) -> Self {
        self.setup.push(Arc::new(setup));
        self
    }

    pub fn reporter(mut self, reporter: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.reporter = Arc::new(reporter);
        self
    }

    pub fn boot(mut self, boot: BootScripts) -> Self {
        self.boot = boot;
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn windows(mut self, windows: Arc<dyn WindowSystem>) -> Self {
        self.windows = Some(windows);
        self
    }

    pub fn game(mut self, name: impl Into<String>, directory: impl Into<PathBuf>) -> Self {
        self.game_name = name.into();
        self.game_dir = Some(directory.into());
        self
    }

    pub fn icon(mut self, icon: Option<&str>) -> Self {
        self.game_icon = icon.and_then(crate::vfs::normalize).filter(|icon| !icon.is_empty());
        self
    }

    pub fn library_dirs(mut self, directories: impl IntoIterator<Item = impl Into<PathBuf>>) -> Self {
        self.library_dirs = directories.into_iter().map(Into::into).collect();
        self
    }

    pub fn container_dirs(mut self, directories: impl IntoIterator<Item = impl Into<PathBuf>>) -> Self {
        self.container_dirs = directories.into_iter().map(Into::into).collect();
        self
    }

    pub fn build(self) -> Arc<Engine> {
        let bus = Arc::new(Bus::new(Arc::new(Activity::new())));
        let closer = Arc::downgrade(&bus);
        bus.activity().on_idle(move || {
            if let Some(bus) = closer.upgrade() {
                bus.begin_close();
            }
        });
        let boot_paths: HashSet<String> = self.boot.all().cloned().collect();
        let layered = Arc::new(LayeredVfs::new(self.vfs));
        let containers = Arc::new(Containers::new(layered.clone(), self.container_dirs));
        Arc::new(Engine {
            vfs: layered.clone(),
            layers: layered,
            containers,
            assets: AssetCache::default(),
            sounds: WeakCache::default(),
            bus,
            setup: self.setup,
            reporter: self.reporter,
            boot: self.boot,
            args: self.args,
            game_name: self.game_name,
            game_icon: self.game_icon,
            game_dir: self.game_dir,
            library_dirs: self.library_dirs,
            windows: self.windows,
            errors: AtomicUsize::new(0),
            exit_code: Mutex::new(None),
            threads: Mutex::new(Vec::new()),
            temp: Mutex::new(None),
            started: Instant::now(),
            main_mailbox: AtomicU64::new(NO_MAILBOX),
            thread_list: Threads::default(),
            protected: Mutex::new(boot_paths),
            registries: Default::default(),
        })
    }
}

pub struct Engine {
    vfs: Arc<dyn Vfs>,
    layers: Arc<LayeredVfs>,
    containers: Arc<Containers>,
    assets: AssetCache,
    sounds: WeakCache<Pcm>,
    bus: Arc<Bus>,
    setup: Vec<Setup>,
    reporter: Reporter,
    boot: BootScripts,
    args: Vec<String>,
    game_name: String,
    game_icon: Option<String>,
    game_dir: Option<PathBuf>,
    library_dirs: Vec<PathBuf>,
    windows: Option<Arc<dyn WindowSystem>>,
    errors: AtomicUsize,
    exit_code: Mutex<Option<i32>>,
    threads: Mutex<Vec<JoinHandle<()>>>,
    temp: Mutex<Option<PathBuf>>,
    started: Instant,
    main_mailbox: AtomicU64,
    thread_list: Threads,
    protected: Mutex<HashSet<String>>,
    registries: crate::api::registry::SafeRegistries,
}

pub const NO_MAILBOX: u64 = u64::MAX;

#[derive(Clone, Debug)]
pub enum Launch {
    Block,
    Once(Payload),
    Bound,
}

#[derive(Clone, Debug)]
pub struct ThreadEntry {
    pub id: u64,
    pub name: String,
    pub main: bool,
    pub state: Option<String>,
    pub data: Option<Packet>,
    pub ready: bool,
}

#[derive(Default)]
pub struct Threads {
    entries: Mutex<Vec<ThreadEntry>>,
    changed: Notify,
}

impl Threads {
    pub fn add(&self, id: u64, name: String, main: bool) {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner).push(ThreadEntry {
            id,
            name,
            main,
            state: None,
            data: None,
            ready: false,
        });
        self.changed.notify_waiters();
    }

    pub fn remove(&self, id: u64) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|entry| entry.id != id);
        self.changed.notify_waiters();
    }

    pub fn get(&self, id: u64) -> Option<ThreadEntry> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
    }

    pub fn list(&self) -> Vec<ThreadEntry> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn update(&self, id: u64, change: impl FnOnce(&mut ThreadEntry)) -> bool {
        let changed = {
            let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            match entries.iter_mut().find(|entry| entry.id == id) {
                Some(entry) => {
                    change(entry);
                    true
                }
                None => false,
            }
        };
        if changed {
            self.changed.notify_waiters();
        }
        changed
    }

    pub fn changed(&self) -> &Notify {
        &self.changed
    }
}

impl Engine {
    pub fn builder(vfs: Arc<dyn Vfs>) -> EngineBuilder {
        EngineBuilder {
            vfs,
            boot: BootScripts::default(),
            setup: Vec::new(),
            reporter: Arc::new(|message| eprintln!("error: {message}")),
            args: Vec::new(),
            game_name: "Game".to_owned(),
            game_icon: None,
            game_dir: None,
            library_dirs: Vec::new(),
            container_dirs: Vec::new(),
            windows: None,
        }
    }

    pub fn new(vfs: Arc<dyn Vfs>) -> Arc<Engine> {
        Self::builder(vfs).build()
    }

    pub fn vfs(&self) -> &Arc<dyn Vfs> {
        &self.vfs
    }

    pub fn mount(&self, layer: Arc<dyn Vfs>) {
        self.layers.mount(layer);
    }

    pub fn containers(&self) -> &Arc<Containers> {
        &self.containers
    }

    pub fn assets(&self) -> &AssetCache {
        &self.assets
    }

    pub fn sounds(&self) -> &WeakCache<Pcm> {
        &self.sounds
    }

    pub fn bus(&self) -> &Arc<Bus> {
        &self.bus
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn game_name(&self) -> &str {
        &self.game_name
    }

    pub fn game_icon(&self) -> Option<&str> {
        self.game_icon.as_deref()
    }

    pub fn game_dir(&self) -> Option<&Path> {
        self.game_dir.as_deref()
    }

    pub fn library_dirs(&self) -> Vec<PathBuf> {
        let mut directories = self.library_dirs.clone();
        if let Some(game) = &self.game_dir
            && !directories.contains(game)
        {
            directories.push(game.clone());
        }
        directories
    }

    pub fn windows(&self) -> Option<&Arc<dyn WindowSystem>> {
        self.windows.as_ref()
    }

    pub fn request_close(&self) {
        self.bus.begin_close();
    }

    pub fn temp_root(&self) -> io::Result<PathBuf> {
        let mut held = self.temp.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(root) = held.as_ref()
            && root.is_dir()
        {
            return Ok(root.clone());
        }
        let base = std::env::temp_dir();
        for _ in 0..TEMP_TRIES {
            let root = base.join(temp_name());
            match fs::create_dir(&root) {
                Ok(()) => {
                    *held = Some(root.clone());
                    return Ok(root);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "a free name for the temporary folder could not be found",
        ))
    }

    pub fn temp_entry(&self, directory: bool) -> io::Result<PathBuf> {
        let root = self.temp_root()?;
        for _ in 0..TEMP_TRIES {
            let path = root.join(TEMP_COUNT.fetch_add(1, Ordering::Relaxed).to_string());
            let made = if directory {
                fs::create_dir(&path)
            } else {
                fs::OpenOptions::new().write(true).create_new(true).open(&path).map(drop)
            };
            match made {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "a free name inside the temporary folder could not be found",
        ))
    }

    pub fn clear_temp(&self) {
        let root = self.temp.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(root) = root {
            let _ = fs::remove_dir_all(root);
        }
    }

    pub fn request_exit(&self, code: i32) {
        self.exit_code
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert(code);
        self.bus.begin_close();
    }

    pub fn exit_code(&self) -> Option<i32> {
        *self.exit_code.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn boot(&self) -> &BootScripts {
        &self.boot
    }

    pub(crate) fn protect(&self, path: &str) {
        self.protected
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(path.to_owned());
    }

    pub(crate) fn protection(&self, path: &str) -> Option<&'static str> {
        if !self
            .protected
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(path)
        {
            return None;
        }
        Some(if self.boot.all().any(|script| script == path) {
            "is started by luv on its own"
        } else {
            "is the main script"
        })
    }

    pub(crate) fn registries(&self) -> &crate::api::registry::SafeRegistries {
        &self.registries
    }

    pub fn threads(&self) -> &Threads {
        &self.thread_list
    }

    pub fn uptime(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    pub(crate) fn set_main_mailbox(&self, id: u64) {
        self.main_mailbox.store(id, Ordering::Release);
    }

    pub(crate) fn main_mailbox(&self) -> Option<u64> {
        match self.main_mailbox.load(Ordering::Acquire) {
            NO_MAILBOX => None,
            id => Some(id),
        }
    }

    pub fn errors(&self) -> usize {
        self.errors.load(Ordering::SeqCst)
    }

    pub fn report(&self, message: &str) {
        self.errors.fetch_add(1, Ordering::SeqCst);
        (self.reporter)(message);
    }

    pub(crate) fn apply_setup(&self, lua: &Lua) -> mlua::Result<()> {
        self.setup.iter().try_for_each(|setup| setup(lua))
    }

    pub(crate) fn spawn_parallel(self: &Arc<Self>, path: String, unit: usize, captures: Payload) -> io::Result<()> {
        let label = format!("parallel block #{unit} of {path}");
        self.spawn_thread(label, path, unit, captures, Launch::Block).map(|_| ())
    }

    pub(crate) fn spawn_thread(
        self: &Arc<Self>,
        label: String,
        path: String,
        unit: usize,
        captures: Payload,
        launch: Launch,
    ) -> io::Result<u64> {
        let mailbox = self.bus.open();
        let id = mailbox.id;
        self.bus.activity().enter(1);

        let engine = self.clone();
        self.thread_list.add(id, label.clone(), false);
        let spawned = thread::Builder::new()
            .name(label.clone())
            .stack_size(THREAD_STACK_SIZE)
            .spawn(move || engine.run_parallel(label, path, unit, captures, launch, mailbox));

        match spawned {
            Ok(handle) => {
                self.threads.lock().unwrap_or_else(PoisonError::into_inner).push(handle);
                Ok(id)
            }
            Err(error) => {
                self.bus.close(id);
                self.thread_list.remove(id);
                self.bus.activity().exit();
                Err(error)
            }
        }
    }

    fn run_parallel(
        self: Arc<Self>,
        label: String,
        path: String,
        unit: usize,
        captures: Payload,
        launch: Launch,
        mailbox: Mailbox,
    ) {
        let started = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
            .and_then(|runtime| {
                let vm = Runtime::parallel(self.clone(), label.clone()).map_err(|error| error.to_string())?;
                Ok((runtime, vm))
            });

        match started {
            Ok((runtime, vm)) => {
                LocalSet::new().block_on(&runtime, vm.run_cluster(mailbox, &path, unit, &captures, launch))
            }
            Err(error) => {
                self.report(&format!("[{label}] failed to start: {error}"));
                self.bus.close(mailbox.id);
                self.thread_list.remove(mailbox.id);
                self.bus.activity().exit();
            }
        }
    }

    pub(crate) async fn join(self: &Arc<Self>) {
        if self.exit_code().is_some() {
            return;
        }
        let engine = self.clone();
        let _ = tokio::task::spawn_blocking(move || {
            loop {
                let handles = mem::take(&mut *engine.threads.lock().unwrap_or_else(PoisonError::into_inner));
                if handles.is_empty() {
                    break;
                }
                for handle in handles {
                    if handle.join().is_err() {
                        engine.report("a parallel thread panicked");
                    }
                }
            }
        })
        .await;
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.clear_temp();
    }
}
