use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::io::Write;
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, RwLock};

use mlua::{
    AnyUserData, Function, LightUserData, Lua, MetaMethod, MultiValue, Result, Table, UserData, UserDataFields,
    UserDataMethods, Value,
};
use tokio::sync::{mpsc, oneshot};

use super::host::{self, RawValue, Task};
use super::library::{Job, LibraryShared};
use super::memory::{Block, Hold, Pointer};
use crate::datatypes::{Color, UDim};
use crate::runtime::Scheduler;

pub const API_VERSION: u32 = 2;
const OK: i32 = 0;
const INVALID: i32 = -4;
pub(super) const INLINE: u32 = 1;
pub(super) const PARALLEL: u32 = 2;
pub(super) const KIND_NONE: i32 = -1;
pub(super) const KIND_NIL: i32 = 0;
pub(super) const KIND_BOOLEAN: i32 = 1;
pub(super) const KIND_NUMBER: i32 = 2;
pub(super) const KIND_STRING: i32 = 3;
pub(super) const KIND_UDIM: i32 = 4;
pub(super) const KIND_COLOR: i32 = 5;
pub(super) const KIND_OBJECT: i32 = 6;
pub(super) const KIND_POINTER: i32 = 7;
pub(super) const KIND_VALUE: i32 = 8;
pub(super) const KIND_BUFFER: i32 = 9;
const SLOTS: usize = 64;
const MAX_MEMBERS: usize = 4096;
const MAX_OBJECT_SIZE: u64 = 1 << 30;
const OBJECTS: &str = "luv.native.objects";
const REGISTER: &str = "luv_register";
const OPERATORS: [&str; 17] = [
    "__add",
    "__sub",
    "__mul",
    "__div",
    "__idiv",
    "__mod",
    "__pow",
    "__unm",
    "__eq",
    "__lt",
    "__le",
    "__len",
    "__concat",
    "__call",
    "__tostring",
    "__index",
    "__newindex",
];
const METAMETHODS: [(MetaMethod, &str); 17] = [
    (MetaMethod::Index, "__index"),
    (MetaMethod::NewIndex, "__newindex"),
    (MetaMethod::ToString, "__tostring"),
    (MetaMethod::Eq, "__eq"),
    (MetaMethod::Lt, "__lt"),
    (MetaMethod::Le, "__le"),
    (MetaMethod::Add, "__add"),
    (MetaMethod::Sub, "__sub"),
    (MetaMethod::Mul, "__mul"),
    (MetaMethod::Div, "__div"),
    (MetaMethod::IDiv, "__idiv"),
    (MetaMethod::Mod, "__mod"),
    (MetaMethod::Pow, "__pow"),
    (MetaMethod::Unm, "__unm"),
    (MetaMethod::Len, "__len"),
    (MetaMethod::Concat, "__concat"),
    (MetaMethod::Call, "__call"),
];

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

pub type RawFunction = unsafe extern "C" fn(*mut Call);
type RawDestroy = unsafe extern "C" fn(*mut c_void);
type RawRegister = unsafe extern "C" fn(*const Api, *mut Registry) -> i32;

#[repr(C)]
pub struct RawMethod {
    name: *const c_char,
    function: Option<RawFunction>,
    flags: u32,
}

#[repr(C)]
pub struct RawProperty {
    name: *const c_char,
    get: Option<RawFunction>,
    set: Option<RawFunction>,
}

#[repr(C)]
pub struct RawClassInfo {
    name: *const c_char,
    size: u64,
    destroy: Option<RawDestroy>,
    methods: *const RawMethod,
    properties: *const RawProperty,
    statics: *const RawMethod,
    static_properties: *const RawProperty,
}

#[repr(C)]
pub struct RawServiceInfo {
    name: *const c_char,
    functions: *const RawMethod,
    properties: *const RawProperty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Worker,
    Inline,
    Parallel,
}

impl Mode {
    pub(super) fn from_flags(flags: u32) -> Mode {
        if flags & PARALLEL != 0 {
            Mode::Parallel
        } else if flags & INLINE != 0 {
            Mode::Inline
        } else {
            Mode::Worker
        }
    }
}

#[derive(Clone)]
pub struct Member {
    label: Arc<str>,
    function: RawFunction,
    mode: Mode,
}

thread_local! {
    static STATES: RefCell<Vec<Lua>> = const { RefCell::new(Vec::new()) };
}

pub(super) struct Standing;

impl Standing {
    pub(super) fn enter(lua: &Lua) -> Standing {
        STATES.with(|states| states.borrow_mut().push(lua.clone()));
        Standing
    }
}

impl Drop for Standing {
    fn drop(&mut self) {
        STATES.with(|states| {
            states.borrow_mut().pop();
        });
    }
}

pub(super) fn current() -> Option<Lua> {
    STATES.with(|states| states.borrow().last().cloned())
}

#[derive(Clone)]
struct Property {
    label: Arc<str>,
    get: Option<RawFunction>,
    set: Option<RawFunction>,
}

pub struct ClassShared {
    name: String,
    c_name: CString,
    size: usize,
    destroy: Option<RawDestroy>,
    methods: Vec<(String, Member)>,
    properties: HashMap<String, Property>,
    statics: Vec<(String, Member)>,
    static_properties: HashMap<String, Property>,
    operators: HashMap<&'static str, Member>,
    module: usize,
    library: RwLock<Arc<LibraryShared>>,
}

impl ClassShared {
    fn library(&self) -> Arc<LibraryShared> {
        self.library.read().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

static CLASSES: Mutex<Vec<Arc<ClassShared>>> = Mutex::new(Vec::new());
static NEXT_VALUE: AtomicU64 = AtomicU64::new(1);

fn classes() -> MutexGuard<'static, Vec<Arc<ClassShared>>> {
    CLASSES.lock().unwrap_or_else(PoisonError::into_inner)
}

fn resolve(class: *const ClassShared) -> Option<Arc<ClassShared>> {
    if class.is_null() {
        return None;
    }
    classes().iter().find(|known| Arc::as_ptr(known) == class).cloned()
}

pub struct ObjectData {
    class: Arc<ClassShared>,
    address: usize,
    block: Option<Block>,
}

impl ObjectData {
    fn create(class: Arc<ClassShared>) -> Option<Arc<ObjectData>> {
        let block = Block::zeroed(class.size)?;
        Some(Arc::new(ObjectData {
            address: block.address(),
            class,
            block: Some(block),
        }))
    }

    pub fn address(&self) -> usize {
        self.address
    }

    pub fn size(&self) -> usize {
        self.class.size
    }

    pub fn class_name(&self) -> &str {
        &self.class.name
    }
}

impl Drop for ObjectData {
    fn drop(&mut self) {
        let Some(block) = self.block.take() else {
            return;
        };
        let Some(destroy) = self.class.destroy else {
            return;
        };
        let job = Job::Task(Box::new(move || {
            unsafe { destroy(block.as_ptr().cast()) };
            drop(block);
        }));
        match self.class.library().worker.sender() {
            Some(sender) => {
                if let Err(unsent) = sender.send(job) {
                    unsent.into_inner().run();
                }
            }
            None => job.run(),
        }
    }
}

pub struct Instance<const SLOT: usize>(Arc<ObjectData>);

impl<const SLOT: usize> UserData for Instance<SLOT> {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field_with(MetaMethod::Type, |lua| Ok(slot_entry(lua, SLOT)?.0.name.clone()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        for (meta, name) in METAMETHODS {
            methods.add_meta_function(meta, move |lua, args: MultiValue| metamethod(lua, SLOT, name, args));
        }
    }
}

fn create<const SLOT: usize>(lua: &Lua, object: Arc<ObjectData>) -> Result<AnyUserData> {
    lua.create_userdata(Instance::<SLOT>(object))
}

fn open<const SLOT: usize>(userdata: &AnyUserData) -> Option<Arc<ObjectData>> {
    userdata.borrow::<Instance<SLOT>>().ok().map(|instance| instance.0.clone())
}

type Create = fn(&Lua, Arc<ObjectData>) -> Result<AnyUserData>;
type Open = fn(&AnyUserData) -> Option<Arc<ObjectData>>;
type Kind = fn() -> TypeId;

macro_rules! slots {
    ($($slot:literal)*) => {
        const CREATE: [Create; SLOTS] = [$(create::<$slot>),*];
        const OPEN: [Open; SLOTS] = [$(open::<$slot>),*];
        const KINDS: [Kind; SLOTS] = [$(TypeId::of::<Instance<$slot>>),*];
    };
}

slots!(
    0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31
    32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63
);

fn slot_of(kind: TypeId) -> Option<usize> {
    static SLOT_KINDS: OnceLock<HashMap<TypeId, usize>> = OnceLock::new();
    SLOT_KINDS
        .get_or_init(|| KINDS.iter().enumerate().map(|(slot, kind)| (kind(), slot)).collect())
        .get(&kind)
        .copied()
}

pub fn object_of(userdata: &AnyUserData) -> Option<Arc<ObjectData>> {
    OPEN[slot_of(userdata.type_id()?)?](userdata)
}

fn object_value(value: &Value) -> Option<Arc<ObjectData>> {
    match value {
        Value::UserData(userdata) => object_of(userdata),
        _ => None,
    }
}

struct SlotEntry {
    class: Arc<ClassShared>,
    methods: Table,
    statics: Option<Table>,
}

#[derive(Default)]
struct ClassHost {
    slots: Vec<SlotEntry>,
    values: HashMap<u64, (Value, usize)>,
    events: Option<mpsc::UnboundedSender<Event>>,
    services: HashMap<String, Table>,
}

fn with_host<R>(lua: &Lua, action: impl FnOnce(&mut ClassHost) -> R) -> R {
    if lua.app_data_ref::<ClassHost>().is_none() {
        lua.set_app_data(ClassHost::default());
    }
    let mut host = lua
        .app_data_mut::<ClassHost>()
        .unwrap_or_else(|| unreachable!("the class host was just installed"));
    action(&mut host)
}

fn slot_entry(lua: &Lua, slot: usize) -> Result<(Arc<ClassShared>, Table)> {
    with_host(lua, |host| {
        host.slots
            .get(slot)
            .map(|entry| (entry.class.clone(), entry.methods.clone()))
    })
    .ok_or_else(|| runtime("this native object belongs to a class that is not loaded"))
}

fn slot_for(lua: &Lua, class: &Arc<ClassShared>) -> Result<usize> {
    let existing = with_host(lua, |host| {
        host.slots.iter().position(|entry| Arc::ptr_eq(&entry.class, class))
    });
    if let Some(slot) = existing {
        return Ok(slot);
    }
    if with_host(lua, |host| host.slots.len()) >= SLOTS {
        return Err(runtime(format!(
            "cannot use the native class {}, a game can use at most {SLOTS} native classes",
            class.name
        )));
    }
    let methods = lua.create_table()?;
    for (name, member) in &class.methods {
        methods.raw_set(
            name.as_str(),
            member_function(lua, Binding::Method(class.clone()), member.clone())?,
        )?;
    }
    Ok(with_host(lua, |host| {
        host.slots.push(SlotEntry {
            class: class.clone(),
            methods,
            statics: None,
        });
        host.slots.len() - 1
    }))
}

fn identities(lua: &Lua) -> Result<Table> {
    if let Some(table) = lua.named_registry_value::<Option<Table>>(OBJECTS)? {
        return Ok(table);
    }
    let table = lua.create_table()?;
    let weak = lua.create_table()?;
    weak.set("__mode", "v")?;
    table.set_metatable(Some(weak))?;
    lua.set_named_registry_value(OBJECTS, &table)?;
    Ok(table)
}

fn wrap(lua: &Lua, object: Arc<ObjectData>) -> Result<AnyUserData> {
    let table = identities(lua)?;
    let key = Value::LightUserData(LightUserData(Arc::as_ptr(&object) as *mut c_void));
    if let Some(existing) = table.raw_get::<Option<AnyUserData>>(key.clone())? {
        return Ok(existing);
    }
    let slot = slot_for(lua, &object.class)?;
    let userdata = CREATE[slot](lua, object)?;
    table.raw_set(key, &userdata)?;
    Ok(userdata)
}

pub enum Event {
    Call(u64, Vec<Out>),
    Release(u64),
    Method(u64, String, Vec<Out>),
    Assign(u64, String, Out),
}

pub(super) fn held(lua: &Lua, id: u64) -> Option<Value> {
    with_host(lua, |host| host.values.get(&id).map(|(value, _)| value.clone()))
}

pub(super) fn events(lua: &Lua) -> Result<mpsc::UnboundedSender<Event>> {
    if let Some(sender) = with_host(lua, |host| host.events.clone()) {
        return Ok(sender);
    }
    let (sender, mut receiver) = mpsc::unbounded_channel();
    with_host(lua, |host| host.events = Some(sender.clone()));
    let state = lua.clone();
    tokio::task::spawn_local(async move {
        while let Some(event) = receiver.recv().await {
            if let Err(error) = deliver(&state, event)
                && let Ok(scheduler) = Scheduler::get(&state)
            {
                scheduler.report(error);
            }
        }
    });
    Ok(sender)
}

fn deliver(lua: &Lua, event: Event) -> Result<()> {
    match event {
        Event::Release(id) => {
            adjust(lua, id, false);
            Ok(())
        }
        Event::Call(id, outputs) => {
            let Some(target) = held(lua, id) else {
                return Ok(());
            };
            let Value::Function(function) = target else {
                return Err(runtime(format!(
                    "a native library sent an event to a {} instead of a function",
                    target.type_name()
                )));
            };
            let mut args = MultiValue::new();
            for output in outputs {
                args.push_back(convert(lua, output, &[], None)?);
            }
            Scheduler::get(lua)?.spawn(lua, function, args);
            Ok(())
        }
        Event::Method(id, name, outputs) => {
            let Some(target) = held(lua, id) else {
                return Ok(());
            };
            let mut args = MultiValue::new();
            args.push_back(target.clone());
            for output in outputs {
                args.push_back(convert(lua, output, &[], None)?);
            }
            let Value::Function(function) = super::host::field(&target, &name)? else {
                return Err(runtime(format!("{name} is not a method of a {}", target.type_name())));
            };
            Scheduler::get(lua)?.spawn(lua, function, args);
            Ok(())
        }
        Event::Assign(id, name, output) => {
            let Some(target) = held(lua, id) else {
                return Ok(());
            };
            let value = convert(lua, output, &[], None)?;
            super::host::assign(&target, &name, value)
        }
    }
}

pub(super) fn register_value(lua: &Lua, value: Value) -> Result<u64> {
    events(lua)?;
    let id = NEXT_VALUE.fetch_add(1, Ordering::Relaxed);
    with_host(lua, |host| host.values.insert(id, (value, 1)));
    Ok(id)
}

pub(super) fn adjust(lua: &Lua, id: u64, retain: bool) {
    let removed = with_host(lua, |host| {
        let entry = host.values.get_mut(&id)?;
        if retain {
            entry.1 += 1;
            return None;
        }
        entry.1 = entry.1.saturating_sub(1);
        if entry.1 == 0 { host.values.remove(&id) } else { None }
    });
    drop(removed);
}

#[derive(Clone)]
pub enum Arg {
    Nil,
    Boolean(bool),
    Number(f64),
    Text(Vec<u8>),
    UDim([f64; 3]),
    Color([f64; 4]),
    Object(Arc<ObjectData>),
    Pointer(usize),
    Value(u64, &'static str),
}

impl Arg {
    fn kind(&self) -> i32 {
        match self {
            Arg::Nil => KIND_NIL,
            Arg::Boolean(_) => KIND_BOOLEAN,
            Arg::Number(_) => KIND_NUMBER,
            Arg::Text(_) => KIND_STRING,
            Arg::UDim(_) => KIND_UDIM,
            Arg::Color(_) => KIND_COLOR,
            Arg::Object(_) => KIND_OBJECT,
            Arg::Pointer(_) => KIND_POINTER,
            Arg::Value(..) => KIND_VALUE,
        }
    }

    fn describe(&self) -> String {
        match self {
            Arg::Nil => "nil".to_owned(),
            Arg::Boolean(_) => "boolean".to_owned(),
            Arg::Number(_) => "number".to_owned(),
            Arg::Text(_) => "string".to_owned(),
            Arg::UDim(_) => UDim::TYPE_NAME.to_owned(),
            Arg::Color(_) => "Color".to_owned(),
            Arg::Object(object) => object.class.name.clone(),
            Arg::Pointer(_) => Pointer::TYPE_NAME.to_owned(),
            Arg::Value(_, name) => (*name).to_owned(),
        }
    }
}

#[derive(Clone)]
pub enum Out {
    Nil,
    Boolean(bool),
    Number(f64),
    Text(Vec<u8>),
    Bytes(Vec<u8>),
    UDim([f64; 3]),
    Color([f64; 4]),
    Object(Arc<ObjectData>),
    Argument(usize),
    This,
    Pointer(usize),
    Ref(u64),
}

pub struct Call {
    pub(super) label: Arc<str>,
    this: Option<Arc<ObjectData>>,
    pub(super) arguments: Vec<Arg>,
    pub(super) results: Vec<Out>,
    error: Option<String>,
    pub(super) retained: Vec<u64>,
    pub(super) events: Option<mpsc::UnboundedSender<Event>>,
    target: Option<u64>,
    _holds: Vec<Hold>,
    pub(super) data: usize,
    pub(super) scratch: Vec<Vec<u8>>,
    pub(super) library: Option<Arc<LibraryShared>>,
}

impl Call {
    pub(super) fn argument(&self, index: i32) -> Option<&Arg> {
        usize::try_from(index).ok().and_then(|index| self.arguments.get(index))
    }

    fn reject(&mut self, index: i32, expected: &str) {
        if self.error.is_some() {
            return;
        }
        let got = self.argument(index).map_or_else(|| "nothing".to_owned(), Arg::describe);
        self.error = Some(format!("argument #{} must be {expected}, got {got}", index.saturating_add(1)));
    }

    pub(super) fn fail(&mut self, message: String) {
        if self.error.is_none() {
            self.error = Some(message);
        }
    }

    pub(super) fn keep(&mut self, bytes: Vec<u8>) -> (*const c_void, u64) {
        let length = bytes.len() as u64;
        self.scratch.push(bytes);
        let stored = self
            .scratch
            .last()
            .unwrap_or_else(|| unreachable!("the bytes were just stored"));
        (stored.as_ptr().cast(), length)
    }
}

fn capture(lua: &Lua, value: &Value, registered: &mut Vec<u64>, holds: &mut Vec<Hold>) -> Result<Arg> {
    Ok(match value {
        Value::Nil => Arg::Nil,
        Value::Boolean(flag) => Arg::Boolean(*flag),
        Value::Integer(number) => Arg::Number(*number as f64),
        Value::Number(number) => Arg::Number(*number),
        Value::String(text) => {
            let mut bytes = text.as_bytes().to_vec();
            bytes.push(0);
            Arg::Text(bytes)
        }
        Value::Buffer(buffer) => {
            let mut bytes = buffer.to_vec();
            bytes.push(0);
            Arg::Text(bytes)
        }
        Value::LightUserData(light) => Arg::Pointer(light.0 as usize),
        Value::UserData(userdata) => {
            if let Ok(udim) = userdata.borrow::<UDim>() {
                Arg::UDim([udim.x, udim.y, udim.z])
            } else if let Ok(color) = userdata.borrow::<Color>() {
                Arg::Color([color.r, color.g, color.b, color.a])
            } else if let Some(object) = object_of(userdata) {
                Arg::Object(object)
            } else if let Ok(pointer) = Pointer::from_userdata(userdata) {
                holds.extend(pointer.owner.hold()?);
                Arg::Pointer(pointer.address)
            } else {
                let id = register_value(lua, value.clone())?;
                registered.push(id);
                Arg::Value(id, "userdata")
            }
        }
        other => {
            let id = register_value(lua, other.clone())?;
            registered.push(id);
            Arg::Value(id, other.type_name())
        }
    })
}

pub(super) fn prepare(
    lua: &Lua,
    label: &Arc<str>,
    library: Option<Arc<LibraryShared>>,
    this: Option<Arc<ObjectData>>,
    values: &[Value],
) -> Result<Call> {
    let mut registered = Vec::new();
    let mut holds = Vec::new();
    let mut arguments = Vec::with_capacity(values.len());
    for value in values {
        match capture(lua, value, &mut registered, &mut holds) {
            Ok(argument) => arguments.push(argument),
            Err(error) => {
                for id in registered {
                    adjust(lua, id, false);
                }
                return Err(error);
            }
        }
    }
    Ok(Call {
        label: label.clone(),
        this,
        arguments,
        results: Vec::new(),
        error: None,
        retained: Vec::new(),
        events: Some(events(lua)?),
        target: None,
        _holds: holds,
        data: 0,
        scratch: Vec::new(),
        library,
    })
}

pub(super) fn convert(lua: &Lua, output: Out, values: &[Value], this: Option<&Value>) -> Result<Value> {
    Ok(match output {
        Out::Nil => Value::Nil,
        Out::Boolean(flag) => Value::Boolean(flag),
        Out::Number(number) => Value::Number(number),
        Out::Text(bytes) => Value::String(lua.create_string(bytes)?),
        Out::Bytes(bytes) => Value::Buffer(lua.create_buffer(bytes)?),
        Out::UDim([x, y, z]) => Value::UserData(lua.create_userdata(UDim::new(x, y, z))?),
        Out::Color([r, g, b, a]) => Value::UserData(lua.create_userdata(Color::new(r, g, b, a))?),
        Out::Object(object) => Value::UserData(wrap(lua, object)?),
        Out::Argument(index) => values.get(index).cloned().unwrap_or(Value::Nil),
        Out::This => this.cloned().unwrap_or(Value::Nil),
        Out::Pointer(0) => Value::Nil,
        Out::Pointer(address) => Value::UserData(lua.create_userdata(Pointer::foreign(address))?),
        Out::Ref(id) => with_host(lua, |host| host.values.get(&id).map(|(value, _)| value.clone())).unwrap_or(Value::Nil),
    })
}

pub(super) fn finish(lua: &Lua, call: Call, values: &[Value], this: Option<&Value>) -> Result<MultiValue> {
    let Call {
        label,
        arguments,
        results,
        error,
        retained,
        ..
    } = call;
    for id in retained {
        adjust(lua, id, true);
    }
    for argument in &arguments {
        if let Arg::Value(id, _) = argument {
            adjust(lua, *id, false);
        }
    }
    if let Some(message) = error {
        return Err(runtime(format!("{label}: {message}")));
    }
    let mut output = MultiValue::new();
    for result in results {
        output.push_back(convert(lua, result, values, this)?);
    }
    Ok(output)
}

fn invoke(
    lua: &Lua,
    label: &Arc<str>,
    library: Option<Arc<LibraryShared>>,
    function: RawFunction,
    this: Option<Arc<ObjectData>>,
    values: &[Value],
    this_value: Option<&Value>,
) -> Result<MultiValue> {
    let mut call = prepare(lua, label, library, this, values)?;
    {
        let _standing = Standing::enter(lua);
        unsafe { function(&mut call) };
    }
    finish(lua, call, values, this_value)
}

fn unloaded(label: &str) -> mlua::Error {
    runtime(format!("cannot call {label} because its library was unloaded"))
}

pub(super) async fn dispatch(library: Arc<LibraryShared>, function: RawFunction, mode: Mode, call: Call) -> Result<Call> {
    let label = call.label.clone();
    let (reply, answer) = oneshot::channel();
    let task = move || {
        let mut call = call;
        unsafe { function(&mut call) };
        let _ = reply.send(call);
    };
    if mode == Mode::Parallel {
        tokio::task::spawn_blocking(task);
    } else {
        let sender = library.worker.sender().ok_or_else(|| unloaded(&label))?;
        sender
            .send(Job::Task(Box::new(task)))
            .map_err(|_| unloaded(&label))?;
    }
    answer.await.map_err(|_| unloaded(&label))
}

type Split<'a> = (Option<Arc<ObjectData>>, &'a [Value], Option<&'a Value>);

#[derive(Clone)]
enum Binding {
    Method(Arc<ClassShared>),
    Static(Arc<ClassShared>),
    Export(Arc<LibraryShared>),
}

impl Binding {
    fn library(&self) -> Arc<LibraryShared> {
        match self {
            Binding::Method(class) | Binding::Static(class) => class.library(),
            Binding::Export(library) => library.clone(),
        }
    }

    fn split<'a>(&self, label: &str, values: &'a [Value]) -> Result<Split<'a>> {
        let Binding::Method(class) = self else {
            return Ok((None, values, None));
        };
        let object = values
            .first()
            .and_then(object_value)
            .filter(|object| Arc::ptr_eq(&object.class, class))
            .ok_or_else(|| runtime(format!("{label} must be called on a {} with ':'", class.name)))?;
        Ok((Some(object), &values[1..], values.first()))
    }
}

fn member_function(lua: &Lua, binding: Binding, member: Member) -> Result<Function> {
    if member.mode == Mode::Inline {
        return lua.create_function(move |lua, args: MultiValue| {
            let values: Vec<Value> = args.into_iter().collect();
            let (this, rest, this_value) = binding.split(&member.label, &values)?;
            invoke(
                lua,
                &member.label,
                Some(binding.library()),
                member.function,
                this,
                rest,
                this_value,
            )
        });
    }
    lua.create_async_function(move |lua, args: MultiValue| {
        let binding = binding.clone();
        let member = member.clone();
        async move {
            let values: Vec<Value> = args.into_iter().collect();
            let (this, rest, this_value) = binding.split(&member.label, &values)?;
            let library = binding.library();
            let call = prepare(&lua, &member.label, Some(library.clone()), this, rest)?;
            let call = dispatch(library, member.function, member.mode, call).await?;
            finish(&lua, call, rest, this_value)
        }
    })
}

fn single(value: Value) -> MultiValue {
    let mut values = MultiValue::new();
    values.push_back(value);
    values
}

fn describe_key(key: &Value) -> String {
    match key {
        Value::String(text) => format!("'{}'", text.to_string_lossy()),
        other => format!("a {} key", other.type_name()),
    }
}

fn metamethod(lua: &Lua, slot: usize, name: &'static str, args: MultiValue) -> Result<MultiValue> {
    let (class, methods) = slot_entry(lua, slot)?;
    let mut values: Vec<Value> = args.into_iter().collect();
    match name {
        "__index" => return index(lua, &class, &methods, &values),
        "__newindex" => return new_index(lua, &class, &values),
        "__unm" | "__len" | "__tostring" => values.truncate(1),
        _ => {}
    }
    if let Some(member) = class.operators.get(name) {
        return invoke(lua, &member.label, Some(class.library()), member.function, None, &values, None);
    }
    match name {
        "__tostring" => Ok(single(Value::String(lua.create_string(&class.name)?))),
        "__eq" => {
            let same = match (values.first(), values.get(1)) {
                (Some(Value::UserData(left)), Some(Value::UserData(right))) => left.to_pointer() == right.to_pointer(),
                _ => false,
            };
            Ok(single(Value::Boolean(same)))
        }
        "__call" => Err(runtime(format!("attempt to call a {} value", class.name))),
        _ => Err(runtime(format!("{} does not support the {name} operator", class.name))),
    }
}

fn index(lua: &Lua, class: &Arc<ClassShared>, methods: &Table, values: &[Value]) -> Result<MultiValue> {
    let object = values.first().cloned().unwrap_or(Value::Nil);
    let key = values.get(1).cloned().unwrap_or(Value::Nil);
    if let Value::String(text) = &key {
        let name = text.to_str()?;
        let method: Value = methods.raw_get(&*name)?;
        if !method.is_nil() {
            return Ok(single(method));
        }
        if let Some(property) = class.properties.get(&*name) {
            let getter = property
                .get
                .ok_or_else(|| runtime(format!("{} cannot be read", property.label)))?;
            let this = object_value(&object);
            return invoke(lua, &property.label, Some(class.library()), getter, this, &[], Some(&object));
        }
    }
    if let Some(member) = class.operators.get("__index") {
        return invoke(lua, &member.label, Some(class.library()), member.function, None, values, None);
    }
    Err(runtime(format!("{} is not a valid member of {}", describe_key(&key), class.name)))
}

fn new_index(lua: &Lua, class: &Arc<ClassShared>, values: &[Value]) -> Result<MultiValue> {
    let object = values.first().cloned().unwrap_or(Value::Nil);
    let key = values.get(1).cloned().unwrap_or(Value::Nil);
    if let Value::String(text) = &key
        && let Some(property) = class.properties.get(&*text.to_str()?)
    {
        let setter = property
            .set
            .ok_or_else(|| runtime(format!("{} is read only", property.label)))?;
        let value = values.get(2).cloned().unwrap_or(Value::Nil);
        invoke(lua, &property.label, Some(class.library()), setter, object_value(&object), &[value], Some(&object))?;
        return Ok(MultiValue::new());
    }
    if let Some(member) = class.operators.get("__newindex") {
        invoke(lua, &member.label, Some(class.library()), member.function, None, values, None)?;
        return Ok(MultiValue::new());
    }
    Err(runtime(format!("{} is not a valid member of {}", describe_key(&key), class.name)))
}

fn statics_table(lua: &Lua, class: &Arc<ClassShared>) -> Result<Table> {
    let table = lua.create_table()?;
    for (name, member) in &class.statics {
        table.raw_set(
            name.as_str(),
            member_function(lua, Binding::Static(class.clone()), member.clone())?,
        )?;
    }
    let meta = lua.create_table()?;
    let reader = class.clone();
    meta.raw_set(
        "__index",
        lua.create_function(move |lua, (_, key): (Value, Value)| {
            let property = match &key {
                Value::String(text) => reader.static_properties.get(&*text.to_str()?).cloned(),
                _ => None,
            };
            let Some(property) = property else {
                return Err(runtime(format!("{} is not a valid member of {}", describe_key(&key), reader.name)));
            };
            let getter = property
                .get
                .ok_or_else(|| runtime(format!("{} cannot be read", property.label)))?;
            invoke(lua, &property.label, Some(reader.library()), getter, None, &[], None)
        })?,
    )?;
    let writer = class.clone();
    meta.raw_set(
        "__newindex",
        lua.create_function(move |lua, (_, key, value): (Value, Value, Value)| {
            let property = match &key {
                Value::String(text) => writer.static_properties.get(&*text.to_str()?).cloned(),
                _ => None,
            };
            let Some(property) = property else {
                return Err(runtime(format!("{} is not a valid member of {}", describe_key(&key), writer.name)));
            };
            let setter = property
                .set
                .ok_or_else(|| runtime(format!("{} is read only", property.label)))?;
            invoke(lua, &property.label, Some(writer.library()), setter, None, &[value], None)?;
            Ok(())
        })?,
    )?;
    let name = class.name.clone();
    meta.raw_set("__tostring", lua.create_function(move |_, _: Value| Ok(name.clone()))?)?;
    meta.raw_set("__metatable", class.name.as_str())?;
    table.set_metatable(Some(meta))?;
    table.set_readonly(true);
    Ok(table)
}

fn class_table(lua: &Lua, class: &Arc<ClassShared>) -> Result<Table> {
    let slot = slot_for(lua, class)?;
    if let Some(table) = with_host(lua, |host| host.slots.get(slot).and_then(|entry| entry.statics.clone())) {
        return Ok(table);
    }
    let table = statics_table(lua, class)?;
    with_host(lua, |host| {
        if let Some(entry) = host.slots.get_mut(slot) {
            entry.statics = Some(table.clone());
        }
    });
    Ok(table)
}

fn service_table(lua: &Lua, service: &Arc<ClassShared>) -> Result<Table> {
    if let Some(table) = with_host(lua, |host| host.services.get(&service.name).cloned()) {
        return Ok(table);
    }
    let table = statics_table(lua, service)?;
    with_host(lua, |host| host.services.insert(service.name.clone(), table.clone()));
    Ok(table)
}

pub struct Exports {
    library: Arc<LibraryShared>,
    classes: Vec<Arc<ClassShared>>,
    functions: Vec<(String, Member)>,
    services: Vec<Arc<ClassShared>>,
}

impl Exports {
    pub fn table(&self, lua: &Lua) -> Result<Table> {
        let table = lua.create_table()?;
        for class in &self.classes {
            table.raw_set(class.name.as_str(), class_table(lua, class)?)?;
        }
        for (name, member) in &self.functions {
            table.raw_set(
                name.as_str(),
                member_function(lua, Binding::Export(self.library.clone()), member.clone())?,
            )?;
        }
        table.set_readonly(true);
        Ok(table)
    }

    pub fn services(&self, lua: &Lua) -> Result<Vec<(String, Table)>> {
        self.services
            .iter()
            .map(|service| Ok((service.name.clone(), service_table(lua, service)?)))
            .collect()
    }

    pub fn service_names(&self) -> Vec<String> {
        self.services.iter().map(|service| service.name.clone()).collect()
    }
}

pub struct Registry {
    module: usize,
    library: Arc<LibraryShared>,
    classes: Vec<Arc<ClassShared>>,
    functions: Vec<(String, Member)>,
    services: Vec<Arc<ClassShared>>,
    errors: Vec<String>,
}

pub fn register(library: &Arc<LibraryShared>) -> std::result::Result<Exports, String> {
    let empty = |library: &Arc<LibraryShared>| Exports {
        library: library.clone(),
        classes: Vec::new(),
        functions: Vec::new(),
        services: Vec::new(),
    };
    let Ok(symbol) = (unsafe { library.library.get::<RawRegister>(REGISTER) }) else {
        return Ok(empty(library));
    };
    let entry: RawRegister = *symbol;
    let mut registry = Registry {
        module: entry as usize,
        library: library.clone(),
        classes: Vec::new(),
        functions: Vec::new(),
        services: Vec::new(),
        errors: Vec::new(),
    };
    let status = unsafe { entry(&API, &mut registry) };
    if status != OK || !registry.errors.is_empty() {
        let mut message = format!("{REGISTER} in {} failed", library.path);
        if status != OK {
            message.push_str(&format!(" with code {status}"));
        }
        if !registry.errors.is_empty() {
            message.push_str(": ");
            message.push_str(&registry.errors.join("; "));
        }
        return Err(message);
    }
    Ok(Exports {
        library: library.clone(),
        classes: registry.classes,
        functions: registry.functions,
        services: registry.services,
    })
}

unsafe fn text(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(pointer) }.to_str().ok().map(str::to_owned)
}

fn identifier(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

unsafe fn method_list(list: *const RawMethod) -> std::result::Result<Vec<(String, Option<RawFunction>, u32)>, String> {
    let mut found = Vec::new();
    if list.is_null() {
        return Ok(found);
    }
    for index in 0..MAX_MEMBERS {
        let entry = unsafe { &*list.add(index) };
        let Some(name) = (unsafe { text(entry.name) }) else {
            if entry.name.is_null() {
                return Ok(found);
            }
            return Err("a member name is not valid UTF-8".to_owned());
        };
        found.push((name, entry.function, entry.flags));
    }
    Err(format!("a member list has more than {MAX_MEMBERS} entries, is it missing its {{0}} terminator?"))
}

type PropertyEntry = (String, Option<RawFunction>, Option<RawFunction>);

unsafe fn property_list(list: *const RawProperty) -> std::result::Result<Vec<PropertyEntry>, String> {
    let mut found = Vec::new();
    if list.is_null() {
        return Ok(found);
    }
    for index in 0..MAX_MEMBERS {
        let entry = unsafe { &*list.add(index) };
        let Some(name) = (unsafe { text(entry.name) }) else {
            if entry.name.is_null() {
                return Ok(found);
            }
            return Err("a property name is not valid UTF-8".to_owned());
        };
        found.push((name, entry.get, entry.set));
    }
    Err(format!("a property list has more than {MAX_MEMBERS} entries, is it missing its {{0}} terminator?"))
}

fn build_properties(class: &str, entries: Vec<PropertyEntry>, separator: &str) -> std::result::Result<HashMap<String, Property>, String> {
    let mut properties = HashMap::new();
    for (name, get, set) in entries {
        if !identifier(&name) {
            return Err(format!("{class}: '{name}' is not a valid property name"));
        }
        if get.is_none() && set.is_none() {
            return Err(format!("{class}{separator}{name} needs a getter or a setter"));
        }
        let property = Property {
            label: format!("{class}{separator}{name}").into(),
            get,
            set,
        };
        if properties.insert(name.clone(), property).is_some() {
            return Err(format!("{class}{separator}{name} is defined twice"));
        }
    }
    Ok(properties)
}

unsafe fn build_class(registry: &mut Registry, info: &RawClassInfo) -> std::result::Result<Arc<ClassShared>, String> {
    let name = unsafe { text(info.name) }
        .filter(|name| identifier(name))
        .ok_or("a class needs a name made of letters, digits and underscores")?;
    let existing = classes()
        .iter()
        .find(|known| known.module == registry.module && known.name == name)
        .cloned();
    if let Some(existing) = existing {
        return Ok(adopt(registry, existing));
    }
    if info.size > MAX_OBJECT_SIZE {
        return Err(format!("{name} objects cannot be larger than {MAX_OBJECT_SIZE} bytes"));
    }
    let mut methods: Vec<(String, Member)> = Vec::new();
    let mut operators = HashMap::new();
    for (method, function, flags) in unsafe { method_list(info.methods) }.map_err(|error| format!("{name}: {error}"))? {
        let function = function.ok_or_else(|| format!("{name}.{method} has no function"))?;
        if let Some(operator) = OPERATORS.iter().find(|operator| **operator == method) {
            let member = Member {
                label: format!("{name}.{method}").into(),
                function,
                mode: Mode::Inline,
            };
            if operators.insert(*operator, member).is_some() {
                return Err(format!("{name}.{method} is defined twice"));
            }
            continue;
        }
        if !identifier(&method) || method.starts_with("__") {
            return Err(format!(
                "{name}: '{method}' is not a valid method name, operators must be one of {}",
                OPERATORS.join(", ")
            ));
        }
        if methods.iter().any(|(known, _)| *known == method) {
            return Err(format!("{name}:{method} is defined twice"));
        }
        let label = format!("{name}:{method}").into();
        methods.push((method, Member {
            label,
            function,
            mode: Mode::from_flags(flags),
        }));
    }
    let properties = build_properties(
        &name,
        unsafe { property_list(info.properties) }.map_err(|error| format!("{name}: {error}"))?,
        ".",
    )?;
    if let Some((clash, _)) = methods.iter().find(|(method, _)| properties.contains_key(method)) {
        return Err(format!("{name}.{clash} is both a method and a property"));
    }
    let mut statics: Vec<(String, Member)> = Vec::new();
    for (function_name, function, flags) in unsafe { method_list(info.statics) }.map_err(|error| format!("{name}: {error}"))? {
        let function = function.ok_or_else(|| format!("{name}.{function_name} has no function"))?;
        if !identifier(&function_name) || function_name.starts_with("__") {
            return Err(format!("{name}: '{function_name}' is not a valid function name"));
        }
        if statics.iter().any(|(known, _)| *known == function_name) {
            return Err(format!("{name}.{function_name} is defined twice"));
        }
        let label = format!("{name}.{function_name}").into();
        statics.push((function_name, Member {
            label,
            function,
            mode: Mode::from_flags(flags),
        }));
    }
    let static_properties = build_properties(
        &name,
        unsafe { property_list(info.static_properties) }.map_err(|error| format!("{name}: {error}"))?,
        ".",
    )?;
    if let Some((clash, _)) = statics.iter().find(|(function, _)| static_properties.contains_key(function)) {
        return Err(format!("{name}.{clash} is both a function and a property"));
    }
    let class = Arc::new(ClassShared {
        c_name: CString::new(name.clone()).map_err(|_| format!("{name} is not a valid class name"))?,
        name,
        size: info.size as usize,
        destroy: info.destroy,
        methods,
        properties,
        statics,
        static_properties,
        operators,
        module: registry.module,
        library: RwLock::new(registry.library.clone()),
    });
    let mut known = classes();
    let raced = known
        .iter()
        .find(|known| known.module == class.module && known.name == class.name)
        .cloned();
    if let Some(existing) = raced {
        drop(known);
        return Ok(adopt(registry, existing));
    }
    known.push(class.clone());
    drop(known);
    registry.classes.push(class.clone());
    Ok(class)
}

unsafe fn build_service(registry: &mut Registry, info: &RawServiceInfo) -> std::result::Result<Arc<ClassShared>, String> {
    let name = unsafe { text(info.name) }
        .filter(|name| identifier(name))
        .ok_or("a service needs a name made of letters, digits and underscores")?;
    if registry.services.iter().any(|known| known.name == name) {
        return Err(format!("the service {name} is defined twice"));
    }
    let mut statics: Vec<(String, Member)> = Vec::new();
    for (function_name, function, flags) in unsafe { method_list(info.functions) }.map_err(|error| format!("{name}: {error}"))? {
        let function = function.ok_or_else(|| format!("{name}.{function_name} has no function"))?;
        if !identifier(&function_name) || function_name.starts_with("__") {
            return Err(format!("{name}: '{function_name}' is not a valid function name"));
        }
        if statics.iter().any(|(known, _)| *known == function_name) {
            return Err(format!("{name}.{function_name} is defined twice"));
        }
        let label = format!("{name}.{function_name}").into();
        statics.push((function_name, Member {
            label,
            function,
            mode: Mode::from_flags(flags),
        }));
    }
    let static_properties = build_properties(
        &name,
        unsafe { property_list(info.properties) }.map_err(|error| format!("{name}: {error}"))?,
        ".",
    )?;
    if let Some((clash, _)) = statics.iter().find(|(function, _)| static_properties.contains_key(function)) {
        return Err(format!("{name}.{clash} is both a function and a property"));
    }
    let service = Arc::new(ClassShared {
        c_name: CString::new(name.clone()).map_err(|_| format!("{name} is not a valid service name"))?,
        name,
        size: 0,
        destroy: None,
        methods: Vec::new(),
        properties: HashMap::new(),
        statics,
        static_properties,
        operators: HashMap::new(),
        module: registry.module,
        library: RwLock::new(registry.library.clone()),
    });
    registry.services.push(service.clone());
    Ok(service)
}

fn adopt(registry: &mut Registry, existing: Arc<ClassShared>) -> Arc<ClassShared> {
    *existing.library.write().unwrap_or_else(PoisonError::into_inner) = registry.library.clone();
    if !registry.classes.iter().any(|known| Arc::ptr_eq(known, &existing)) {
        registry.classes.push(existing.clone());
    }
    existing
}

pub struct RefHandle {
    id: u64,
    events: mpsc::UnboundedSender<Event>,
}

impl RefHandle {
    pub(super) fn into_raw(id: u64, events: mpsc::UnboundedSender<Event>) -> *mut RefHandle {
        Box::into_raw(Box::new(RefHandle { id, events }))
    }

    pub(super) fn id(&self) -> u64 {
        self.id
    }

    pub(super) fn send(&self, event: Event) -> bool {
        self.events.send(event).is_ok()
    }
}

#[repr(C)]
pub struct Api {
    version: u32,
    struct_size: u32,
    define_class: unsafe extern "C" fn(*mut Registry, *const RawClassInfo) -> *const ClassShared,
    define_function: unsafe extern "C" fn(*mut Registry, *const RawMethod) -> i32,
    find_class: unsafe extern "C" fn(*const c_char) -> *const ClassShared,
    class_name: unsafe extern "C" fn(*const ClassShared) -> *const c_char,
    arg_count: unsafe extern "C" fn(*mut Call) -> i32,
    arg_kind: unsafe extern "C" fn(*mut Call, i32) -> i32,
    arg_class: unsafe extern "C" fn(*mut Call, i32) -> *const ClassShared,
    check_boolean: unsafe extern "C" fn(*mut Call, i32) -> i32,
    opt_boolean: unsafe extern "C" fn(*mut Call, i32, i32) -> i32,
    check_number: unsafe extern "C" fn(*mut Call, i32) -> f64,
    opt_number: unsafe extern "C" fn(*mut Call, i32, f64) -> f64,
    check_string: unsafe extern "C" fn(*mut Call, i32, *mut u64) -> *const c_char,
    opt_string: unsafe extern "C" fn(*mut Call, i32, *const c_char, *mut u64) -> *const c_char,
    check_udim: unsafe extern "C" fn(*mut Call, i32, *mut f64) -> i32,
    check_color: unsafe extern "C" fn(*mut Call, i32, *mut f64) -> i32,
    check_object: unsafe extern "C" fn(*mut Call, i32, *const ClassShared) -> *mut c_void,
    to_object: unsafe extern "C" fn(*mut Call, i32, *const ClassShared) -> *mut c_void,
    check_pointer: unsafe extern "C" fn(*mut Call, i32) -> *mut c_void,
    self_data: unsafe extern "C" fn(*mut Call) -> *mut c_void,
    push_nil: unsafe extern "C" fn(*mut Call),
    push_boolean: unsafe extern "C" fn(*mut Call, i32),
    push_number: unsafe extern "C" fn(*mut Call, f64),
    push_string: unsafe extern "C" fn(*mut Call, *const c_char),
    push_bytes: unsafe extern "C" fn(*mut Call, *const c_void, u64),
    push_udim: unsafe extern "C" fn(*mut Call, f64, f64, f64),
    push_color: unsafe extern "C" fn(*mut Call, f64, f64, f64, f64),
    push_object: unsafe extern "C" fn(*mut Call, *const ClassShared) -> *mut c_void,
    push_argument: unsafe extern "C" fn(*mut Call, i32),
    push_self: unsafe extern "C" fn(*mut Call),
    push_pointer: unsafe extern "C" fn(*mut Call, *mut c_void),
    push_ref: unsafe extern "C" fn(*mut Call, *mut RefHandle),
    fail: unsafe extern "C" fn(*mut Call, *const c_char),
    retain: unsafe extern "C" fn(*mut Call, i32) -> *mut RefHandle,
    release: unsafe extern "C" fn(*mut RefHandle),
    begin_event: unsafe extern "C" fn(*mut RefHandle) -> *mut Call,
    send_event: unsafe extern "C" fn(*mut Call) -> i32,
    print: unsafe extern "C" fn(*const c_char),
    warn: unsafe extern "C" fn(*const c_char),
    define_service: unsafe extern "C" fn(*mut Registry, *const RawServiceInfo) -> *const ClassShared,
    on_game_thread: unsafe extern "C" fn(*mut Call) -> i32,
    call_data: unsafe extern "C" fn(*mut Call) -> *mut c_void,
    arg_value: unsafe extern "C" fn(*mut Call, i32, *mut RawValue) -> i32,
    push_value: unsafe extern "C" fn(*mut Call, *const RawValue),
    push_buffer: unsafe extern "C" fn(*mut Call, u64) -> *mut c_void,
    get_import: unsafe extern "C" fn(*mut Call, *const c_char) -> *mut RefHandle,
    get_global: unsafe extern "C" fn(*mut Call, *const c_char) -> *mut RefHandle,
    set_global: unsafe extern "C" fn(*mut Call, *const c_char, *const RawValue) -> i32,
    get_api: unsafe extern "C" fn(*mut Call, *mut RefHandle, *const c_char) -> *mut RefHandle,
    new_table: unsafe extern "C" fn(*mut Call) -> *mut RefHandle,
    new_signal: unsafe extern "C" fn(*mut Call, *const c_char) -> *mut RefHandle,
    new_function: unsafe extern "C" fn(*mut Call, *const c_char, Option<RawFunction>, *mut c_void, u32) -> *mut RefHandle,
    read_member: unsafe extern "C" fn(*mut Call, *mut RefHandle, *const c_char, *mut RawValue) -> i32,
    write_member: unsafe extern "C" fn(*mut Call, *mut RefHandle, *const c_char, *const RawValue) -> i32,
    call_member: unsafe extern "C" fn(*mut Call, *mut RefHandle, *const c_char, *const RawValue, i32, *mut RawValue, i32) -> i32,
    construct: unsafe extern "C" fn(*mut Call, *mut RefHandle, *const c_char, *const RawValue, i32) -> *mut RefHandle,
    connect: unsafe extern "C" fn(*mut Call, *mut RefHandle, *const c_char, Option<RawFunction>, *mut c_void, u32) -> i32,
    post_call: unsafe extern "C" fn(*mut RefHandle, *const c_char, *const RawValue, i32) -> i32,
    post_write: unsafe extern "C" fn(*mut RefHandle, *const c_char, *const RawValue) -> i32,
    schedule: unsafe extern "C" fn(*mut Call, *const c_char, Option<RawFunction>, *mut c_void, f64, u32) -> *mut Task,
    cancel: unsafe extern "C" fn(*mut Task),
}

pub static API: Api = Api {
    version: API_VERSION,
    struct_size: size_of::<Api>() as u32,
    define_class,
    define_function,
    find_class,
    class_name,
    arg_count,
    arg_kind,
    arg_class,
    check_boolean,
    opt_boolean,
    check_number,
    opt_number,
    check_string,
    opt_string,
    check_udim,
    check_color,
    check_object,
    to_object,
    check_pointer,
    self_data,
    push_nil,
    push_boolean,
    push_number,
    push_string,
    push_bytes,
    push_udim,
    push_color,
    push_object,
    push_argument,
    push_self,
    push_pointer,
    push_ref,
    fail,
    retain,
    release,
    begin_event,
    send_event,
    print,
    warn,
    define_service,
    on_game_thread: host::on_game_thread,
    call_data: host::call_data,
    arg_value: host::arg_value,
    push_value: host::push_value,
    push_buffer: host::push_buffer,
    get_import: host::get_import,
    get_global: host::get_global,
    set_global: host::set_global,
    get_api: host::get_api,
    new_table: host::new_table,
    new_signal: host::new_signal,
    new_function: host::new_function,
    read_member: host::read_member,
    write_member: host::write_member,
    call_member: host::call_member,
    construct: host::construct,
    connect: host::connect,
    post_call: host::post_call,
    post_write: host::post_write,
    schedule: host::schedule,
    cancel: host::cancel,
};

unsafe extern "C" fn define_service(registry: *mut Registry, info: *const RawServiceInfo) -> *const ClassShared {
    let Some(registry) = (unsafe { registry.as_mut() }) else {
        return ptr::null();
    };
    let Some(info) = (unsafe { info.as_ref() }) else {
        registry.errors.push("define_service needs a service description".to_owned());
        return ptr::null();
    };
    match unsafe { build_service(registry, info) } {
        Ok(service) => Arc::as_ptr(&service),
        Err(error) => {
            registry.errors.push(error);
            ptr::null()
        }
    }
}

unsafe extern "C" fn define_class(registry: *mut Registry, info: *const RawClassInfo) -> *const ClassShared {
    let Some(registry) = (unsafe { registry.as_mut() }) else {
        return ptr::null();
    };
    let Some(info) = (unsafe { info.as_ref() }) else {
        registry.errors.push("define_class needs a class description".to_owned());
        return ptr::null();
    };
    match unsafe { build_class(registry, info) } {
        Ok(class) => Arc::as_ptr(&class),
        Err(error) => {
            registry.errors.push(error);
            ptr::null()
        }
    }
}

unsafe extern "C" fn define_function(registry: *mut Registry, method: *const RawMethod) -> i32 {
    let Some(registry) = (unsafe { registry.as_mut() }) else {
        return INVALID;
    };
    let Some(method) = (unsafe { method.as_ref() }) else {
        registry.errors.push("define_function needs a function description".to_owned());
        return INVALID;
    };
    let Some(name) = (unsafe { text(method.name) }).filter(|name| identifier(name)) else {
        registry.errors.push("an exported function needs a name made of letters, digits and underscores".to_owned());
        return INVALID;
    };
    let Some(function) = method.function else {
        registry.errors.push(format!("the exported function {name} has no function"));
        return INVALID;
    };
    let taken = registry.functions.iter().any(|(known, _)| *known == name)
        || registry.classes.iter().any(|class| class.name == name);
    if taken {
        registry.errors.push(format!("{name} is exported twice"));
        return INVALID;
    }
    registry.functions.push((name.clone(), Member {
        label: name.into(),
        function,
        mode: Mode::from_flags(method.flags),
    }));
    OK
}

unsafe extern "C" fn find_class(name: *const c_char) -> *const ClassShared {
    let Some(name) = (unsafe { text(name) }) else {
        return ptr::null();
    };
    classes()
        .iter()
        .find(|class| class.name == name)
        .map_or(ptr::null(), Arc::as_ptr)
}

unsafe extern "C" fn class_name(class: *const ClassShared) -> *const c_char {
    resolve(class).map_or(ptr::null(), |class| class.c_name.as_ptr())
}

unsafe fn call_mut<'a>(call: *mut Call) -> Option<&'a mut Call> {
    unsafe { call.as_mut() }
}

unsafe extern "C" fn arg_count(call: *mut Call) -> i32 {
    unsafe { call_mut(call) }.map_or(0, |call| i32::try_from(call.arguments.len()).unwrap_or(i32::MAX))
}

unsafe extern "C" fn arg_kind(call: *mut Call, index: i32) -> i32 {
    unsafe { call_mut(call) }
        .and_then(|call| call.argument(index).map(Arg::kind))
        .unwrap_or(KIND_NONE)
}

unsafe extern "C" fn arg_class(call: *mut Call, index: i32) -> *const ClassShared {
    match unsafe { call_mut(call) }.and_then(|call| call.argument(index)) {
        Some(Arg::Object(object)) => Arc::as_ptr(&object.class),
        _ => ptr::null(),
    }
}

unsafe extern "C" fn check_boolean(call: *mut Call, index: i32) -> i32 {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return 0;
    };
    match call.argument(index) {
        Some(Arg::Boolean(flag)) => i32::from(*flag),
        _ => {
            call.reject(index, "a boolean");
            0
        }
    }
}

unsafe extern "C" fn opt_boolean(call: *mut Call, index: i32, fallback: i32) -> i32 {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return fallback;
    };
    match call.argument(index) {
        None | Some(Arg::Nil) => fallback,
        Some(Arg::Boolean(flag)) => i32::from(*flag),
        _ => {
            call.reject(index, "a boolean or nil");
            fallback
        }
    }
}

unsafe extern "C" fn check_number(call: *mut Call, index: i32) -> f64 {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return 0.0;
    };
    match call.argument(index) {
        Some(Arg::Number(number)) => *number,
        _ => {
            call.reject(index, "a number");
            0.0
        }
    }
}

unsafe extern "C" fn opt_number(call: *mut Call, index: i32, fallback: f64) -> f64 {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return fallback;
    };
    match call.argument(index) {
        None | Some(Arg::Nil) => fallback,
        Some(Arg::Number(number)) => *number,
        _ => {
            call.reject(index, "a number or nil");
            fallback
        }
    }
}

unsafe fn store_length(length: *mut u64, value: usize) {
    if !length.is_null() {
        unsafe { *length = value as u64 };
    }
}

unsafe extern "C" fn check_string(call: *mut Call, index: i32, length: *mut u64) -> *const c_char {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return ptr::null();
    };
    match call.argument(index) {
        Some(Arg::Text(bytes)) => {
            unsafe { store_length(length, bytes.len() - 1) };
            bytes.as_ptr().cast()
        }
        _ => {
            unsafe { store_length(length, 0) };
            call.reject(index, "a string");
            ptr::null()
        }
    }
}

unsafe extern "C" fn opt_string(call: *mut Call, index: i32, fallback: *const c_char, length: *mut u64) -> *const c_char {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return fallback;
    };
    let fallback_length = || if fallback.is_null() { 0 } else { unsafe { CStr::from_ptr(fallback) }.to_bytes().len() };
    match call.argument(index) {
        None | Some(Arg::Nil) => {
            unsafe { store_length(length, fallback_length()) };
            fallback
        }
        Some(Arg::Text(bytes)) => {
            unsafe { store_length(length, bytes.len() - 1) };
            bytes.as_ptr().cast()
        }
        _ => {
            call.reject(index, "a string or nil");
            unsafe { store_length(length, fallback_length()) };
            fallback
        }
    }
}

unsafe extern "C" fn check_udim(call: *mut Call, index: i32, out: *mut f64) -> i32 {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return 0;
    };
    match call.argument(index) {
        Some(Arg::UDim(values)) if !out.is_null() => {
            unsafe { ptr::copy_nonoverlapping(values.as_ptr(), out, 3) };
            1
        }
        _ => {
            call.reject(index, "a UDim");
            0
        }
    }
}

unsafe extern "C" fn check_color(call: *mut Call, index: i32, out: *mut f64) -> i32 {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return 0;
    };
    match call.argument(index) {
        Some(Arg::Color(values)) if !out.is_null() => {
            unsafe { ptr::copy_nonoverlapping(values.as_ptr(), out, 4) };
            1
        }
        _ => {
            call.reject(index, "a Color");
            0
        }
    }
}

fn matching(call: &Call, index: i32, class: *const ClassShared) -> Option<usize> {
    match call.argument(index) {
        Some(Arg::Object(object)) if class.is_null() || Arc::as_ptr(&object.class) == class => Some(object.address),
        _ => None,
    }
}

unsafe extern "C" fn check_object(call: *mut Call, index: i32, class: *const ClassShared) -> *mut c_void {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return ptr::null_mut();
    };
    if let Some(address) = matching(call, index, class) {
        return address as *mut c_void;
    }
    let expected = resolve(class).map_or_else(|| "a native object".to_owned(), |class| format!("a {}", class.name));
    call.reject(index, &expected);
    ptr::null_mut()
}

unsafe extern "C" fn to_object(call: *mut Call, index: i32, class: *const ClassShared) -> *mut c_void {
    unsafe { call_mut(call) }
        .and_then(|call| matching(call, index, class))
        .map_or(ptr::null_mut(), |address| address as *mut c_void)
}

unsafe extern "C" fn check_pointer(call: *mut Call, index: i32) -> *mut c_void {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return ptr::null_mut();
    };
    match call.argument(index) {
        Some(Arg::Nil) => ptr::null_mut(),
        Some(Arg::Pointer(address)) => *address as *mut c_void,
        Some(Arg::Object(object)) => object.address as *mut c_void,
        Some(Arg::Text(bytes)) => bytes.as_ptr() as *mut c_void,
        _ => {
            call.reject(index, "a Pointer, string or nil");
            ptr::null_mut()
        }
    }
}

unsafe extern "C" fn self_data(call: *mut Call) -> *mut c_void {
    unsafe { call_mut(call) }
        .and_then(|call| call.this.as_ref().map(|object| object.address as *mut c_void))
        .unwrap_or(ptr::null_mut())
}

unsafe fn push(call: *mut Call, output: Out) {
    if let Some(call) = unsafe { call_mut(call) } {
        call.results.push(output);
    }
}

unsafe extern "C" fn push_nil(call: *mut Call) {
    unsafe { push(call, Out::Nil) }
}

unsafe extern "C" fn push_boolean(call: *mut Call, value: i32) {
    unsafe { push(call, Out::Boolean(value != 0)) }
}

unsafe extern "C" fn push_number(call: *mut Call, value: f64) {
    unsafe { push(call, Out::Number(value)) }
}

unsafe extern "C" fn push_string(call: *mut Call, value: *const c_char) {
    let output = if value.is_null() {
        Out::Nil
    } else {
        Out::Text(unsafe { CStr::from_ptr(value) }.to_bytes().to_vec())
    };
    unsafe { push(call, output) }
}

unsafe extern "C" fn push_bytes(call: *mut Call, data: *const c_void, length: u64) {
    let output = match usize::try_from(length) {
        Ok(0) => Out::Text(Vec::new()),
        Ok(length) if !data.is_null() => Out::Text(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), length) }.to_vec()),
        _ => Out::Nil,
    };
    unsafe { push(call, output) }
}

unsafe extern "C" fn push_udim(call: *mut Call, x: f64, y: f64, z: f64) {
    unsafe { push(call, Out::UDim([x, y, z])) }
}

unsafe extern "C" fn push_color(call: *mut Call, r: f64, g: f64, b: f64, a: f64) {
    unsafe { push(call, Out::Color([r, g, b, a])) }
}

unsafe extern "C" fn push_object(call: *mut Call, class: *const ClassShared) -> *mut c_void {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return ptr::null_mut();
    };
    let Some(class) = resolve(class) else {
        call.fail("push_object was given a class that was never defined".to_owned());
        return ptr::null_mut();
    };
    let Some(object) = ObjectData::create(class.clone()) else {
        call.fail(format!("the memory for a new {} could not be allocated", class.name));
        return ptr::null_mut();
    };
    let address = object.address;
    call.results.push(Out::Object(object));
    address as *mut c_void
}

unsafe extern "C" fn push_argument(call: *mut Call, index: i32) {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return;
    };
    let output = match usize::try_from(index) {
        Ok(index) if index < call.arguments.len() => Out::Argument(index),
        _ => Out::Nil,
    };
    call.results.push(output);
}

unsafe extern "C" fn push_self(call: *mut Call) {
    unsafe { push(call, Out::This) }
}

unsafe extern "C" fn push_pointer(call: *mut Call, address: *mut c_void) {
    unsafe { push(call, Out::Pointer(address as usize)) }
}

unsafe extern "C" fn push_ref(call: *mut Call, handle: *mut RefHandle) {
    let output = unsafe { handle.as_ref() }.map_or(Out::Nil, |handle| Out::Ref(handle.id));
    unsafe { push(call, output) }
}

unsafe extern "C" fn fail(call: *mut Call, message: *const c_char) {
    let message = unsafe { text(message) }.unwrap_or_else(|| "the native call failed".to_owned());
    if let Some(call) = unsafe { call_mut(call) } {
        call.fail(message);
    }
}

unsafe extern "C" fn retain(call: *mut Call, index: i32) -> *mut RefHandle {
    let Some(call) = (unsafe { call_mut(call) }) else {
        return ptr::null_mut();
    };
    let Some(Arg::Value(id, _)) = call.argument(index) else {
        return ptr::null_mut();
    };
    let id = *id;
    let Some(events) = call.events.clone() else {
        return ptr::null_mut();
    };
    call.retained.push(id);
    Box::into_raw(Box::new(RefHandle { id, events }))
}

unsafe extern "C" fn release(handle: *mut RefHandle) {
    if handle.is_null() {
        return;
    }
    let handle = unsafe { Box::from_raw(handle) };
    let _ = handle.events.send(Event::Release(handle.id));
}

unsafe extern "C" fn begin_event(handle: *mut RefHandle) -> *mut Call {
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return ptr::null_mut();
    };
    Box::into_raw(Box::new(Call {
        label: Arc::from("event"),
        this: None,
        arguments: Vec::new(),
        results: Vec::new(),
        error: None,
        retained: Vec::new(),
        events: Some(handle.events.clone()),
        target: Some(handle.id),
        _holds: Vec::new(),
        data: 0,
        scratch: Vec::new(),
        library: None,
    }))
}

unsafe extern "C" fn send_event(call: *mut Call) -> i32 {
    if call.is_null() {
        return INVALID;
    }
    let call = unsafe { Box::from_raw(call) };
    let (Some(target), Some(events), None) = (call.target, call.events, call.error) else {
        return INVALID;
    };
    match events.send(Event::Call(target, call.results)) {
        Ok(()) => OK,
        Err(_) => INVALID,
    }
}

unsafe extern "C" fn print(message: *const c_char) {
    if let Some(message) = unsafe { text(message) } {
        let _ = writeln!(std::io::stdout().lock(), "{message}");
    }
}

unsafe extern "C" fn warn(message: *const c_char) {
    if let Some(message) = unsafe { text(message) } {
        let _ = writeln!(std::io::stderr().lock(), "warning: {message}");
    }
}
