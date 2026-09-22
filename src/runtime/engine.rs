use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::thread::{self, JoinHandle};

use mlua::Lua;
use tokio::task::LocalSet;

use super::{Runtime, THREAD_STACK_SIZE};
use super::bus::{Bus, Mailbox, Payload};
use super::containers::Containers;
use super::scheduler::Activity;
use crate::audio::Pcm;
use crate::vfs::{LayeredVfs, Vfs};
use crate::window::WindowSystem;

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

pub struct EngineBuilder {
    vfs: Arc<dyn Vfs>,
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
        let layered = Arc::new(LayeredVfs::new(self.vfs));
        let containers = Arc::new(Containers::new(layered.clone(), self.container_dirs));
        Arc::new(Engine {
            vfs: layered,
            containers,
            assets: AssetCache::default(),
            sounds: WeakCache::default(),
            bus,
            setup: self.setup,
            reporter: self.reporter,
            args: self.args,
            game_name: self.game_name,
            game_icon: self.game_icon,
            game_dir: self.game_dir,
            library_dirs: self.library_dirs,
            windows: self.windows,
            errors: AtomicUsize::new(0),
            exit_code: Mutex::new(None),
            threads: Mutex::new(Vec::new()),
        })
    }
}

pub struct Engine {
    vfs: Arc<dyn Vfs>,
    containers: Arc<Containers>,
    assets: AssetCache,
    sounds: WeakCache<Pcm>,
    bus: Arc<Bus>,
    setup: Vec<Setup>,
    reporter: Reporter,
    args: Vec<String>,
    game_name: String,
    game_icon: Option<String>,
    game_dir: Option<PathBuf>,
    library_dirs: Vec<PathBuf>,
    windows: Option<Arc<dyn WindowSystem>>,
    errors: AtomicUsize,
    exit_code: Mutex<Option<i32>>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Engine {
    pub fn builder(vfs: Arc<dyn Vfs>) -> EngineBuilder {
        EngineBuilder {
            vfs,
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
        let mailbox = self.bus.open();
        let id = mailbox.id;
        self.bus.activity().enter(1);

        let engine = self.clone();
        let label = format!("parallel block #{unit} of {path}");
        let spawned = thread::Builder::new()
            .name(label.clone())
            .stack_size(THREAD_STACK_SIZE)
            .spawn(move || engine.run_parallel(label, path, unit, captures, mailbox));

        match spawned {
            Ok(handle) => {
                self.threads.lock().unwrap_or_else(PoisonError::into_inner).push(handle);
                Ok(())
            }
            Err(error) => {
                self.bus.close(id);
                self.bus.activity().exit();
                Err(error)
            }
        }
    }

    fn run_parallel(self: Arc<Self>, label: String, path: String, unit: usize, captures: Payload, mailbox: Mailbox) {
        let started = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
            .and_then(|runtime| {
                let vm = Runtime::parallel(self.clone(), label.clone()).map_err(|error| error.to_string())?;
                Ok((runtime, vm))
            });

        match started {
            Ok((runtime, vm)) => LocalSet::new().block_on(&runtime, vm.run_cluster(mailbox, &path, unit, &captures)),
            Err(error) => {
                self.report(&format!("[{label}] failed to start: {error}"));
                self.bus.close(mailbox.id);
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
