use std::cell::RefCell;
use std::os::raw::c_void;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use crossbeam_channel::{Receiver, Sender};
use libffi::middle::Cif;
use mlua::{
    AnyUserData, Lua, MetaMethod, MultiValue, Result, Table, UserData, UserDataFields, UserDataMethods, UserDataRef,
    Value,
};
use tokio::sync::oneshot;

use super::NativeHost;
use super::classes::{self, Exports};
use super::marshal::{self, Scratch, Writer};
use super::memory::{Block, Hold, Owner, Pointer};
use super::types::{CType, align_up};
use crate::objects::{BaseGameObject, GameObject};
use crate::runtime::Engine;

#[cfg(windows)]
pub const EXTENSION: &str = "dll";
#[cfg(not(windows))]
pub const EXTENSION: &str = "so";
const OPTIONS: [&str; 2] = ["ErrorCode", "Parallel"];

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

thread_local! {
    static SERVING: RefCell<Option<Receiver<Job>>> = const { RefCell::new(None) };
}

pub fn serving() -> Option<Receiver<Job>> {
    SERVING.with(|serving| serving.borrow().clone())
}

pub struct Worker {
    jobs: Mutex<Option<Sender<Job>>>,
}

impl Worker {
    pub fn new(jobs: Sender<Job>) -> Arc<Worker> {
        Arc::new(Worker {
            jobs: Mutex::new(Some(jobs)),
        })
    }

    pub fn detached() -> std::io::Result<Arc<Worker>> {
        let (sender, receiver) = crossbeam_channel::unbounded();
        thread::Builder::new()
            .name("dll calls".to_owned())
            .spawn(move || serve(receiver))?;
        Ok(Worker::new(sender))
    }

    pub(super) fn sender(&self) -> Option<Sender<Job>> {
        self.jobs.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn close(&self) {
        self.jobs.lock().unwrap_or_else(PoisonError::into_inner).take();
    }
}

fn serve(receiver: Receiver<Job>) {
    SERVING.with(|serving| *serving.borrow_mut() = Some(receiver.clone()));
    while let Ok(job) = receiver.recv() {
        job.run();
    }
    SERVING.with(|serving| serving.borrow_mut().take());
}

pub struct Frame {
    arena: Block,
    offsets: Vec<usize>,
    blocks: Vec<Block>,
    _holds: Vec<Hold>,
    result: Block,
    error: i32,
}

pub enum Job {
    Call {
        function: Arc<FunctionShared>,
        frame: Frame,
        reply: oneshot::Sender<Frame>,
    },
    Task(Box<dyn FnOnce() + Send>),
}

impl Job {
    pub fn run(self) {
        match self {
            Job::Task(task) => task(),
            Job::Call {
                function,
                mut frame,
                reply,
            } => {
                function.execute(&mut frame);
                let _ = reply.send(frame);
            }
        }
    }
}

#[cfg(windows)]
mod errors {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLastError() -> u32;
        fn SetLastError(code: u32);
    }

    pub fn clear() {
        unsafe { SetLastError(0) }
    }

    pub fn last() -> i32 {
        unsafe { GetLastError() as i32 }
    }
}

#[cfg(not(windows))]
mod errors {
    unsafe extern "C" {
        fn __errno_location() -> *mut i32;
    }

    pub fn clear() {
        unsafe { *__errno_location() = 0 }
    }

    pub fn last() -> i32 {
        unsafe { *__errno_location() }
    }
}

struct SharedCif(Cif);

unsafe impl Send for SharedCif {}
unsafe impl Sync for SharedCif {}

pub struct FunctionShared {
    name: String,
    cif: SharedCif,
    code: usize,
    arguments: Vec<CType>,
    result: CType,
    error_code: bool,
    parallel: bool,
    owner: Owner,
    worker: Arc<Worker>,
}

impl FunctionShared {
    fn execute(&self, frame: &mut Frame) {
        let base = frame.arena.as_ptr();
        let mut pointers: Vec<*mut c_void> = frame
            .offsets
            .iter()
            .map(|offset| unsafe { base.add(*offset) } as *mut c_void)
            .collect();
        if self.error_code {
            errors::clear();
        }
        unsafe {
            let code: unsafe extern "C" fn() = std::mem::transmute(self.code);
            libffi::raw::ffi_call(
                self.cif.0.as_raw_ptr(),
                Some(code),
                frame.result.as_ptr() as *mut c_void,
                pointers.as_mut_ptr(),
            );
        }
        if self.error_code {
            frame.error = errors::last();
        }
    }

    fn prepare(&self, lua: &Lua, args: MultiValue) -> Result<(Frame, Vec<(usize, mlua::Buffer)>)> {
        if args.len() > self.arguments.len() {
            return Err(runtime(format!(
                "{} takes {} argument{}, got {}",
                self.name,
                self.arguments.len(),
                if self.arguments.len() == 1 { "" } else { "s" },
                args.len()
            )));
        }
        let mut offsets = Vec::with_capacity(self.arguments.len());
        let mut size = 0;
        for ty in &self.arguments {
            size = align_up(size, ty.align());
            offsets.push(size);
            size += ty.size();
        }
        let allocation = || runtime("the memory for the call could not be allocated");
        let arena = Block::zeroed(size).ok_or_else(allocation)?;
        let mut scratch = Scratch::default();
        let values: Vec<Value> = args.into_iter().collect();
        for (index, ty) in self.arguments.iter().enumerate() {
            let value = values.get(index).cloned().unwrap_or(Value::Nil);
            unsafe { Writer::call(&mut scratch).write(lua, ty, &value, arena.as_ptr().add(offsets[index])) }
                .map_err(|error| runtime(format!("argument #{} of {}: {error}", index + 1, self.name)))?;
        }
        if let Some(hold) = self.owner.hold()? {
            scratch.holds.push(hold);
        }
        let result = Block::zeroed(self.result.size().max(16)).ok_or_else(allocation)?;
        Ok((
            Frame {
                arena,
                offsets,
                blocks: scratch.blocks,
                _holds: scratch.holds,
                result,
                error: 0,
            },
            scratch.returns,
        ))
    }
}

async fn call(lua: Lua, shared: Arc<FunctionShared>, args: MultiValue) -> Result<MultiValue> {
    let (frame, returns) = shared.prepare(&lua, args)?;
    let (reply, answer) = oneshot::channel();
    let job = Job::Call {
        function: shared.clone(),
        frame,
        reply,
    };
    if shared.parallel {
        tokio::task::spawn_blocking(move || job.run());
    } else {
        let sender = shared
            .worker
            .sender()
            .ok_or_else(|| runtime(format!("cannot call {} because its library was unloaded", shared.name)))?;
        sender
            .send(job)
            .map_err(|_| runtime(format!("cannot call {} because its library was unloaded", shared.name)))?;
    }
    let frame = answer
        .await
        .map_err(|_| runtime(format!("{} did not finish because its library was unloaded", shared.name)))?;
    for (index, buffer) in returns {
        if let Some(block) = frame.blocks.get(index) {
            buffer.write_bytes(0, &block.bytes()[..buffer.len().min(block.len())]);
        }
    }
    let value = unsafe { marshal::read(&lua, &shared.result, frame.result.as_ptr()) }?;
    let mut results = MultiValue::new();
    results.push_back(value);
    if shared.error_code {
        results.push_back(Value::Number(f64::from(frame.error)));
    }
    Ok(results)
}

fn options(options: Option<Table>) -> Result<(bool, bool)> {
    let Some(options) = options else {
        return Ok((false, false));
    };
    for pair in options.pairs::<Value, Value>() {
        let (key, _) = pair?;
        let known = matches!(&key, Value::String(name) if OPTIONS.contains(&&*name.to_str()?));
        if !known {
            return Err(runtime(format!(
                "unknown function option {}, the options are {}",
                match &key {
                    Value::String(name) => format!("'{}'", name.to_str()?),
                    other => other.type_name().to_owned(),
                },
                OPTIONS.join(" and ")
            )));
        }
    }
    Ok((
        options.get::<Option<bool>>("ErrorCode")?.unwrap_or(false),
        options.get::<Option<bool>>("Parallel")?.unwrap_or(false),
    ))
}

pub struct NativeFunction {
    base: BaseGameObject,
    shared: Arc<FunctionShared>,
}

impl NativeFunction {
    pub const CLASS_NAME: &'static str = "NativeFunction";

    pub fn create(
        lua: &Lua,
        name: String,
        pointer: Pointer,
        result: &Value,
        arguments: Option<Table>,
        settings: Option<Table>,
    ) -> Result<AnyUserData> {
        if pointer.address == 0 {
            return Err(runtime("cannot make a NativeFunction from a null Pointer"));
        }
        let result = CType::parse_result(result)?;
        let arguments = CType::parse_list(arguments)?;
        let (error_code, parallel) = options(settings)?;
        let worker = match &pointer.owner {
            Owner::Library(library) => library.worker.clone(),
            _ => NativeHost::detached(lua)?,
        };
        let cif = Cif::new(arguments.iter().map(CType::ffi).collect::<Vec<_>>(), result.ffi());
        let mut base = BaseGameObject::new(Self::CLASS_NAME);
        base.set_name(name.clone());
        lua.create_userdata(NativeFunction {
            base,
            shared: Arc::new(FunctionShared {
                name,
                cif: SharedCif(cif),
                code: pointer.address,
                arguments,
                result,
                error_code,
                parallel,
                owner: pointer.owner,
                worker,
            }),
        })
    }

    pub fn pointer(&self) -> Pointer {
        Pointer {
            address: self.shared.code,
            owner: self.shared.owner.clone(),
        }
    }
}

impl GameObject for NativeFunction {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }
}

impl UserData for NativeFunction {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("Pointer", |_, this| Ok(this.pointer()));
        fields.add_meta_field_with(MetaMethod::Call, |lua| {
            lua.create_async_function(|lua, (this, args): (AnyUserData, MultiValue)| async move {
                let shared = {
                    let this = this.borrow::<NativeFunction>()?;
                    this.base.ensure_alive()?;
                    this.shared.clone()
                };
                call(lua, shared, args).await
            })
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_async_method("Call", |lua, this: UserDataRef<Self>, args: MultiValue| {
            let shared = this.shared.clone();
            let alive = this.base.ensure_alive();
            drop(this);
            async move {
                alive?;
                call(lua, shared, args).await
            }
        });
    }
}

pub struct LibraryShared {
    pub path: String,
    pub library: Arc<libloading::Library>,
    pub(super) worker: Arc<Worker>,
}

pub struct Library {
    base: BaseGameObject,
    shared: Arc<LibraryShared>,
    exports: Exports,
    table: RefCell<Option<Table>>,
}

fn normalize(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normal.pop() {
                    normal.push(component);
                }
            }
            other => normal.push(other),
        }
    }
    normal
}

fn variants(path: &Path) -> Vec<PathBuf> {
    let mut list = vec![path.to_path_buf()];
    if path.extension().is_none()
        && let Some(name) = path.file_name().map(|name| name.to_string_lossy().into_owned())
    {
        list.push(path.with_file_name(format!("{name}.{EXTENSION}")));
        if cfg!(not(windows)) && !name.starts_with("lib") {
            list.push(path.with_file_name(format!("lib{name}.{EXTENSION}")));
        }
    }
    list
}

fn bare(path: &str) -> bool {
    !path.contains(['/', '\\']) && !path.starts_with('.')
}

fn candidates(path: &str, directories: &[PathBuf]) -> Vec<PathBuf> {
    let given = Path::new(path);
    if given.is_absolute() {
        return variants(&normalize(given));
    }
    let mut bases = Vec::new();
    if let Some(directory) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        bases.push(directory);
    }
    for directory in directories {
        let directory = std::path::absolute(directory).unwrap_or_else(|_| directory.clone());
        if !bases.contains(&directory) {
            bases.push(directory);
        }
    }
    bases
        .iter()
        .flat_map(|base| variants(&normalize(&base.join(given))))
        .collect()
}

#[cfg(windows)]
unsafe fn open_file(path: &Path) -> std::result::Result<libloading::Library, libloading::Error> {
    unsafe {
        libloading::os::windows::Library::load_with_flags(path, libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH)
            .map(Into::into)
    }
}

#[cfg(not(windows))]
unsafe fn open_file(path: &Path) -> std::result::Result<libloading::Library, libloading::Error> {
    use libloading::os::unix::{RTLD_LOCAL, RTLD_NOW};
    unsafe { libloading::os::unix::Library::open(Some(path), RTLD_NOW | RTLD_LOCAL).map(Into::into) }
}

fn open(path: &str, candidates: &[PathBuf]) -> std::result::Result<(libloading::Library, String), String> {
    for candidate in candidates {
        if candidate.is_file() {
            let shown = candidate.display().to_string();
            return unsafe { open_file(candidate) }
                .map(|library| (library, shown.clone()))
                .map_err(|error| format!("cannot load {shown}: {error}"));
        }
    }
    if bare(path) {
        let mut last = None;
        for name in variants(Path::new(path)) {
            match unsafe { libloading::Library::new(&name) } {
                Ok(library) => return Ok((library, name.display().to_string())),
                Err(error) => last = Some(error),
            }
        }
        if let Some(error) = last {
            return Err(format!("cannot load '{path}': {error}"));
        }
    }
    let looked: Vec<String> = candidates.iter().map(|candidate| candidate.display().to_string()).collect();
    Err(format!("cannot find '{path}', looked for {}", looked.join(", ")))
}

impl Library {
    pub const CLASS_NAME: &'static str = "Library";

    pub async fn load(lua: Lua, path: String) -> Result<AnyUserData> {
        if path.trim().is_empty() {
            return Err(runtime("DLL.Load needs the path of a library"));
        }
        let directories = lua
            .app_data_ref::<Arc<Engine>>()
            .map(|engine| engine.library_dirs())
            .unwrap_or_default();
        let candidates = candidates(&path, &directories);
        let (sender, receiver) = crossbeam_channel::unbounded();
        let worker = Worker::new(sender);
        let (ready, loaded) = oneshot::channel();
        let requested = path.clone();
        let name = Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        thread::Builder::new()
            .name(format!("dll {name}"))
            .spawn(move || match open(&requested, &candidates) {
                Ok((library, shown)) => {
                    let library = Arc::new(library);
                    let shared = Arc::new(LibraryShared {
                        path: shown,
                        library: library.clone(),
                        worker,
                    });
                    let registered = classes::register(&shared).map(|exports| (shared, exports));
                    let usable = registered.is_ok();
                    if ready.send(registered).is_ok() && usable {
                        serve(receiver);
                    }
                    drop(library);
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                }
            })
            .map_err(|error| runtime(format!("cannot start a thread for '{path}': {error}")))?;
        let (shared, exports) = loaded
            .await
            .map_err(|_| runtime(format!("loading '{path}' stopped unexpectedly")))?
            .map_err(runtime)?;
        let mut base = BaseGameObject::new(Self::CLASS_NAME);
        base.set_name(name);
        lua.create_userdata(Library {
            base,
            shared,
            exports,
            table: RefCell::new(None),
        })
    }

    fn exports(&self, lua: &Lua) -> Result<Table> {
        self.base.ensure_alive()?;
        if let Some(table) = self.table.borrow().as_ref() {
            return Ok(table.clone());
        }
        let table = self.exports.table(lua)?;
        *self.table.borrow_mut() = Some(table.clone());
        Ok(table)
    }

    fn symbol(&self, name: &str) -> Result<Option<usize>> {
        self.base.ensure_alive()?;
        if name.is_empty() || name.contains('\0') {
            return Err(runtime(format!("'{name}' is not a valid symbol name")));
        }
        Ok(unsafe { self.shared.library.get::<*mut c_void>(name) }
            .ok()
            .map(|symbol| *symbol as usize)
            .filter(|address| *address != 0))
    }

    fn require(&self, name: &str) -> Result<Pointer> {
        let address = self.symbol(name)?.ok_or_else(|| {
            runtime(format!("{} has no exported symbol named '{name}'", self.shared.path))
        })?;
        Ok(Pointer {
            address,
            owner: Owner::Library(self.shared.clone()),
        })
    }
}

impl GameObject for Library {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        self.shared.worker.close();
        *self.table.borrow_mut() = None;
    }
}

impl UserData for Library {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("Path", |_, this| Ok(this.shared.path.clone()));
        fields.add_field_method_get("Exports", |lua, this| this.exports(lua));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        Self::add_base_methods(methods);
        methods.add_method("HasSymbol", |_, this, name: String| Ok(this.symbol(&name)?.is_some()));
        methods.add_method("GetSymbol", |_, this, name: String| this.require(&name));
        methods.add_method(
            "GetFunction",
            |lua, this, (name, result, arguments, settings): (String, Value, Option<Table>, Option<Table>)| {
                let pointer = this.require(&name)?;
                NativeFunction::create(lua, name, pointer, &result, arguments, settings)
            },
        );
    }
}
