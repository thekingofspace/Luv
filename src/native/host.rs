use std::ffi::{CStr, c_char, c_void};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use mlua::{Function, Lua, MultiValue, ObjectLike, Result, Value};

use super::classes::{
    Arg, Call, Event, KIND_BOOLEAN, KIND_BUFFER, KIND_COLOR, KIND_NIL, KIND_NONE, KIND_NUMBER, KIND_OBJECT,
    KIND_POINTER, KIND_STRING, KIND_UDIM, KIND_VALUE, Mode, Out, RawFunction, RefHandle, Standing, convert, current,
    dispatch, events, finish, held, object_of, prepare, register_value,
};
use super::library::LibraryShared;
use super::memory::Pointer;
use crate::datatypes::{Color, UDim};
use crate::objects::Signal;
use crate::runtime::{Scheduler, imports};

const OK: i32 = 0;
const UNKNOWN_NAME: i32 = -1;
const OUT_OF_RANGE: i32 = -2;
const WRONG_KIND: i32 = -3;
const INVALID: i32 = -4;
const OFF_THREAD: i32 = -5;
const MAX_VALUES: usize = 64;
const MIN_PERIOD: Duration = Duration::from_millis(1);

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawValue {
    kind: i32,
    flags: u32,
    numbers: [f64; 4],
    data: *const c_void,
    length: u64,
    reference: *mut RefHandle,
}

impl RawValue {
    fn empty(kind: i32) -> RawValue {
        RawValue {
            kind,
            flags: 0,
            numbers: [0.0; 4],
            data: ptr::null(),
            length: 0,
            reference: ptr::null_mut(),
        }
    }
}

pub struct Task {
    cancelled: Arc<AtomicBool>,
}

unsafe fn text(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(pointer) }.to_str().ok().map(str::to_owned)
}

pub(super) fn field(target: &Value, name: &str) -> Result<Value> {
    match target {
        Value::Table(table) => table.get(name),
        Value::UserData(userdata) => userdata.get(name),
        other => Err(runtime(format!("cannot read {name} from a {}", other.type_name()))),
    }
}

pub(super) fn assign(target: &Value, name: &str, value: Value) -> Result<()> {
    match target {
        Value::Table(table) => table.set(name, value),
        Value::UserData(userdata) => userdata.set(name, value),
        other => Err(runtime(format!("cannot write {name} on a {}", other.type_name()))),
    }
}

fn out_of(value: &RawValue) -> Out {
    match value.kind {
        KIND_BOOLEAN => Out::Boolean(value.numbers[0] != 0.0),
        KIND_NUMBER => Out::Number(value.numbers[0]),
        KIND_UDIM => Out::UDim([value.numbers[0], value.numbers[1], value.numbers[2]]),
        KIND_COLOR => Out::Color(value.numbers),
        KIND_POINTER => Out::Pointer(value.data as usize),
        KIND_STRING | KIND_BUFFER => match bytes_of(value) {
            Some(bytes) if value.kind == KIND_BUFFER => Out::Bytes(bytes),
            Some(bytes) => Out::Text(bytes),
            None => Out::Nil,
        },
        KIND_OBJECT | KIND_VALUE => match unsafe { value.reference.as_ref() } {
            Some(handle) => Out::Ref(handle.id()),
            None => Out::Nil,
        },
        _ => Out::Nil,
    }
}

fn bytes_of(value: &RawValue) -> Option<Vec<u8>> {
    let length = usize::try_from(value.length).ok()?;
    if length == 0 {
        return Some(Vec::new());
    }
    if value.data.is_null() {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(value.data.cast::<u8>(), length) }.to_vec())
}

unsafe fn gather(values: *const RawValue, count: i32) -> std::result::Result<Vec<Out>, i32> {
    let count = usize::try_from(count).map_err(|_| OUT_OF_RANGE)?;
    if count > MAX_VALUES {
        return Err(OUT_OF_RANGE);
    }
    if count > 0 && values.is_null() {
        return Err(INVALID);
    }
    Ok((0..count)
        .map(|index| out_of(unsafe { &*values.add(index) }))
        .collect())
}

fn handle(lua: &Lua, value: Value) -> Result<*mut RefHandle> {
    let id = register_value(lua, value)?;
    Ok(RefHandle::into_raw(id, events(lua)?))
}

fn store(call: &mut Call, lua: &Lua, value: &Value) -> Result<RawValue> {
    Ok(match value {
        Value::Nil => RawValue::empty(KIND_NIL),
        Value::Boolean(flag) => {
            let mut raw = RawValue::empty(KIND_BOOLEAN);
            raw.numbers[0] = f64::from(*flag);
            raw
        }
        Value::Integer(number) => {
            let mut raw = RawValue::empty(KIND_NUMBER);
            raw.numbers[0] = *number as f64;
            raw
        }
        Value::Number(number) => {
            let mut raw = RawValue::empty(KIND_NUMBER);
            raw.numbers[0] = *number;
            raw
        }
        Value::String(string) => {
            let mut bytes = string.as_bytes().to_vec();
            let length = bytes.len() as u64;
            bytes.push(0);
            let mut raw = RawValue::empty(KIND_STRING);
            (raw.data, _) = call.keep(bytes);
            raw.length = length;
            raw
        }
        Value::Buffer(buffer) => {
            let mut raw = RawValue::empty(KIND_BUFFER);
            (raw.data, raw.length) = call.keep(buffer.to_vec());
            raw
        }
        Value::LightUserData(light) => {
            let mut raw = RawValue::empty(KIND_POINTER);
            raw.data = light.0;
            raw
        }
        Value::UserData(userdata) => {
            if let Ok(udim) = userdata.borrow::<UDim>() {
                let mut raw = RawValue::empty(KIND_UDIM);
                raw.numbers = [udim.x, udim.y, udim.z, 0.0];
                raw
            } else if let Ok(color) = userdata.borrow::<Color>() {
                let mut raw = RawValue::empty(KIND_COLOR);
                raw.numbers = [color.r, color.g, color.b, color.a];
                raw
            } else if let Some(object) = object_of(userdata) {
                let mut raw = RawValue::empty(KIND_OBJECT);
                raw.data = object.address() as *const c_void;
                raw.reference = handle(lua, value.clone())?;
                raw
            } else if let Ok(pointer) = Pointer::from_userdata(userdata) {
                let mut raw = RawValue::empty(KIND_POINTER);
                raw.data = pointer.address as *const c_void;
                raw
            } else {
                let mut raw = RawValue::empty(KIND_VALUE);
                raw.reference = handle(lua, value.clone())?;
                raw
            }
        }
        other => {
            let mut raw = RawValue::empty(KIND_VALUE);
            raw.reference = handle(lua, other.clone())?;
            raw
        }
    })
}

unsafe fn standing<'a>(call: *mut Call) -> std::result::Result<(&'a mut Call, Lua), i32> {
    let call = unsafe { call.as_mut() }.ok_or(INVALID)?;
    match current() {
        Some(lua) => Ok((call, lua)),
        None => {
            call.fail(format!("{} needs the game thread, register it with LUV_INLINE", call.label));
            Err(OFF_THREAD)
        }
    }
}

fn refuse<T>(call: &mut Call, error: mlua::Error, fallback: T) -> T {
    call.fail(error.to_string());
    fallback
}

fn target_of(reference: *mut RefHandle, lua: &Lua) -> Option<Value> {
    held(lua, unsafe { reference.as_ref() }?.id())
}

#[derive(Clone)]
struct Entry {
    label: Arc<str>,
    library: Option<Arc<LibraryShared>>,
    function: RawFunction,
    data: usize,
    mode: Mode,
}

impl Entry {
    fn new(call: &Call, label: Arc<str>, function: RawFunction, data: *mut c_void, flags: u32) -> Entry {
        Entry {
            label,
            library: call.library.clone(),
            function,
            data: data as usize,
            mode: Mode::from_flags(flags),
        }
    }
}

fn run(lua: &Lua, entry: &Entry, values: &[Value]) -> Result<Call> {
    let mut call = prepare(lua, &entry.label, entry.library.clone(), None, values)?;
    call.data = entry.data;
    let _standing = Standing::enter(lua);
    unsafe { (entry.function)(&mut call) };
    Ok(call)
}

async fn detached(lua: &Lua, entry: &Entry, values: &[Value]) -> Result<Call> {
    let mut call = prepare(lua, &entry.label, entry.library.clone(), None, values)?;
    call.data = entry.data;
    let function = entry.function;
    match &entry.library {
        Some(library) if entry.mode == Mode::Worker => dispatch(library.clone(), function, entry.mode, call).await,
        _ => {
            let (reply, answer) = tokio::sync::oneshot::channel();
            tokio::task::spawn_blocking(move || {
                let mut call = call;
                unsafe { function(&mut call) };
                let _ = reply.send(call);
            });
            answer
                .await
                .map_err(|_| runtime(format!("{} did not finish", entry.label)))
        }
    }
}

fn native_function(lua: &Lua, entry: Entry) -> Result<Function> {
    if entry.mode == Mode::Inline {
        return lua.create_function(move |lua, args: MultiValue| {
            let values: Vec<Value> = args.into_iter().collect();
            let call = run(lua, &entry, &values)?;
            finish(lua, call, &values, None)
        });
    }
    lua.create_async_function(move |lua, args: MultiValue| {
        let entry = entry.clone();
        async move {
            let values: Vec<Value> = args.into_iter().collect();
            let call = detached(&lua, &entry, &values).await?;
            finish(&lua, call, &values, None)
        }
    })
}

fn repeat_task(lua: &Lua, entry: Entry, period: Duration, cancelled: Arc<AtomicBool>) {
    let lua = lua.clone();
    tokio::task::spawn_local(async move {
        let scheduler = Scheduler::get(&lua).ok();
        loop {
            tokio::time::sleep(period).await;
            if cancelled.load(Ordering::Acquire) {
                break;
            }
            if entry.library.as_ref().is_some_and(|library| library.worker.sender().is_none()) {
                break;
            }
            let outcome = match entry.mode {
                Mode::Inline => run(&lua, &entry, &[]),
                _ => detached(&lua, &entry, &[]).await,
            }
            .and_then(|call| finish(&lua, call, &[], None).map(|_| ()));
            if let Err(error) = outcome
                && let Some(scheduler) = &scheduler
            {
                scheduler.report(error);
            }
        }
    });
}

pub unsafe extern "C" fn on_game_thread(call: *mut Call) -> i32 {
    if call.is_null() {
        return 0;
    }
    i32::from(current().is_some())
}

pub unsafe extern "C" fn call_data(call: *mut Call) -> *mut c_void {
    unsafe { call.as_ref() }.map_or(ptr::null_mut(), |call| call.data as *mut c_void)
}

pub unsafe extern "C" fn arg_value(call: *mut Call, index: i32, out: *mut RawValue) -> i32 {
    let Some(call) = (unsafe { call.as_mut() }) else {
        return INVALID;
    };
    if out.is_null() {
        return INVALID;
    }
    let Some(argument) = call.argument(index) else {
        unsafe { out.write(RawValue::empty(KIND_NONE)) };
        return OUT_OF_RANGE;
    };
    let mut held_id = None;
    let mut value = match argument {
        Arg::Nil => RawValue::empty(KIND_NIL),
        Arg::Boolean(flag) => {
            let mut value = RawValue::empty(KIND_BOOLEAN);
            value.numbers[0] = f64::from(*flag);
            value
        }
        Arg::Number(number) => {
            let mut value = RawValue::empty(KIND_NUMBER);
            value.numbers[0] = *number;
            value
        }
        Arg::Text(bytes) => {
            let mut value = RawValue::empty(KIND_STRING);
            value.data = bytes.as_ptr().cast();
            value.length = (bytes.len() - 1) as u64;
            value
        }
        Arg::UDim(parts) => {
            let mut value = RawValue::empty(KIND_UDIM);
            value.numbers = [parts[0], parts[1], parts[2], 0.0];
            value
        }
        Arg::Color(parts) => {
            let mut value = RawValue::empty(KIND_COLOR);
            value.numbers = *parts;
            value
        }
        Arg::Object(object) => {
            let mut value = RawValue::empty(KIND_OBJECT);
            value.data = object.address() as *const c_void;
            value
        }
        Arg::Pointer(address) => {
            let mut value = RawValue::empty(KIND_POINTER);
            value.data = *address as *const c_void;
            value
        }
        Arg::Value(id, _) => {
            held_id = Some(*id);
            RawValue::empty(KIND_VALUE)
        }
    };
    if let Some(id) = held_id {
        let Some(sender) = call.events.clone() else {
            unsafe { out.write(value) };
            return INVALID;
        };
        call.retained.push(id);
        value.reference = RefHandle::into_raw(id, sender);
    }
    unsafe { out.write(value) };
    OK
}

pub unsafe extern "C" fn push_value(call: *mut Call, value: *const RawValue) {
    let Some(call) = (unsafe { call.as_mut() }) else {
        return;
    };
    let output = match unsafe { value.as_ref() } {
        Some(value) => out_of(value),
        None => Out::Nil,
    };
    call.results.push(output);
}

pub unsafe extern "C" fn push_buffer(call: *mut Call, length: u64) -> *mut c_void {
    let Some(call) = (unsafe { call.as_mut() }) else {
        return ptr::null_mut();
    };
    let Ok(length) = usize::try_from(length) else {
        call.fail("a buffer cannot be that large".to_owned());
        return ptr::null_mut();
    };
    call.results.push(Out::Bytes(vec![0; length]));
    match call.results.last_mut() {
        Some(Out::Bytes(bytes)) => bytes.as_mut_ptr().cast(),
        _ => ptr::null_mut(),
    }
}

pub unsafe extern "C" fn get_import(call: *mut Call, name: *const c_char) -> *mut RefHandle {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return ptr::null_mut();
    };
    let Some(name) = (unsafe { text(name) }) else {
        return refuse(call, runtime("get_import needs a name"), ptr::null_mut());
    };
    match imports::get(&lua, &name).and_then(|value| handle(&lua, value)) {
        Ok(reference) => reference,
        Err(error) => refuse(call, error, ptr::null_mut()),
    }
}

pub unsafe extern "C" fn get_global(call: *mut Call, name: *const c_char) -> *mut RefHandle {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return ptr::null_mut();
    };
    let Some(name) = (unsafe { text(name) }) else {
        return refuse(call, runtime("get_global needs a name"), ptr::null_mut());
    };
    match lua.globals().get::<Value>(name.as_str()).and_then(|value| handle(&lua, value)) {
        Ok(reference) => reference,
        Err(error) => refuse(call, error, ptr::null_mut()),
    }
}

pub unsafe extern "C" fn set_global(call: *mut Call, name: *const c_char, value: *const RawValue) -> i32 {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return OFF_THREAD;
    };
    let Some(name) = (unsafe { text(name) }) else {
        return refuse(call, runtime("set_global needs a name"), INVALID);
    };
    let output = match unsafe { value.as_ref() } {
        Some(value) => out_of(value),
        None => Out::Nil,
    };
    match convert(&lua, output, &[], None).and_then(|value| lua.globals().set(name.as_str(), value)) {
        Ok(()) => OK,
        Err(error) => refuse(call, error, INVALID),
    }
}

pub unsafe extern "C" fn new_table(call: *mut Call) -> *mut RefHandle {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return ptr::null_mut();
    };
    match lua.create_table().and_then(|table| handle(&lua, Value::Table(table))) {
        Ok(reference) => reference,
        Err(error) => refuse(call, error, ptr::null_mut()),
    }
}

pub unsafe extern "C" fn new_signal(call: *mut Call, name: *const c_char) -> *mut RefHandle {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return ptr::null_mut();
    };
    let signal = match unsafe { text(name) } {
        Some(name) if !name.is_empty() => Signal::named(name),
        _ => Signal::new(),
    };
    match lua
        .create_userdata(signal)
        .and_then(|userdata| handle(&lua, Value::UserData(userdata)))
    {
        Ok(reference) => reference,
        Err(error) => refuse(call, error, ptr::null_mut()),
    }
}

pub unsafe extern "C" fn new_function(
    call: *mut Call,
    name: *const c_char,
    function: Option<RawFunction>,
    data: *mut c_void,
    flags: u32,
) -> *mut RefHandle {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return ptr::null_mut();
    };
    let Some(function) = function else {
        return refuse(call, runtime("new_function needs a function"), ptr::null_mut());
    };
    let label: Arc<str> = unsafe { text(name) }
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "a native function".to_owned())
        .into();
    let made = native_function(&lua, Entry::new(call, label, function, data, flags))
        .and_then(|made| handle(&lua, Value::Function(made)));
    match made {
        Ok(reference) => reference,
        Err(error) => refuse(call, error, ptr::null_mut()),
    }
}

pub unsafe extern "C" fn read_member(
    call: *mut Call,
    target: *mut RefHandle,
    name: *const c_char,
    out: *mut RawValue,
) -> i32 {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return OFF_THREAD;
    };
    if out.is_null() {
        return INVALID;
    }
    unsafe { out.write(RawValue::empty(KIND_NIL)) };
    let (Some(target), Some(name)) = (target_of(target, &lua), unsafe { text(name) }) else {
        return refuse(call, runtime("read_member needs a value and a name"), INVALID);
    };
    match field(&target, &name).and_then(|value| store(call, &lua, &value)) {
        Ok(value) => {
            unsafe { out.write(value) };
            OK
        }
        Err(error) => refuse(call, error, UNKNOWN_NAME),
    }
}

pub unsafe extern "C" fn write_member(
    call: *mut Call,
    target: *mut RefHandle,
    name: *const c_char,
    value: *const RawValue,
) -> i32 {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return OFF_THREAD;
    };
    let (Some(target), Some(name)) = (target_of(target, &lua), unsafe { text(name) }) else {
        return refuse(call, runtime("write_member needs a value and a name"), INVALID);
    };
    let output = match unsafe { value.as_ref() } {
        Some(value) => out_of(value),
        None => Out::Nil,
    };
    match convert(&lua, output, &[], None).and_then(|value| assign(&target, &name, value)) {
        Ok(()) => OK,
        Err(error) => refuse(call, error, UNKNOWN_NAME),
    }
}

fn apply(lua: &Lua, target: &Value, name: &str, args: Vec<Out>, method: bool) -> Result<MultiValue> {
    let Value::Function(function) = field(target, name)? else {
        return Err(runtime(format!("{name} is not a function of a {}", target.type_name())));
    };
    let mut values = MultiValue::new();
    if method {
        values.push_back(target.clone());
    }
    for output in args {
        values.push_back(convert(lua, output, &[], None)?);
    }
    function.call::<MultiValue>(values)
}

pub unsafe extern "C" fn call_member(
    call: *mut Call,
    target: *mut RefHandle,
    name: *const c_char,
    args: *const RawValue,
    count: i32,
    results: *mut RawValue,
    limit: i32,
) -> i32 {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return OFF_THREAD;
    };
    let (Some(target), Some(name)) = (target_of(target, &lua), unsafe { text(name) }) else {
        return refuse(call, runtime("call_member needs a value and a name"), INVALID);
    };
    let args = match unsafe { gather(args, count) } {
        Ok(args) => args,
        Err(code) => return refuse(call, runtime("call_member was given too many arguments"), code),
    };
    let returned = match apply(&lua, &target, &name, args, true) {
        Ok(returned) => returned,
        Err(error) => return refuse(call, error, WRONG_KIND),
    };
    let limit = usize::try_from(limit).unwrap_or(0).min(MAX_VALUES);
    if limit == 0 || results.is_null() {
        return OK;
    }
    let mut written = 0;
    for value in returned.into_iter().take(limit) {
        match store(call, &lua, &value) {
            Ok(value) => unsafe { results.add(written).write(value) },
            Err(error) => return refuse(call, error, WRONG_KIND),
        }
        written += 1;
    }
    i32::try_from(written).unwrap_or(0)
}

pub unsafe extern "C" fn construct(
    call: *mut Call,
    api: *mut RefHandle,
    name: *const c_char,
    args: *const RawValue,
    count: i32,
) -> *mut RefHandle {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return ptr::null_mut();
    };
    let (Some(target), Some(name)) = (target_of(api, &lua), unsafe { text(name) }) else {
        return refuse(call, runtime("construct needs an API and a name"), ptr::null_mut());
    };
    let args = match unsafe { gather(args, count) } {
        Ok(args) => args,
        Err(_) => return refuse(call, runtime("construct was given too many arguments"), ptr::null_mut()),
    };
    let made = apply(&lua, &target, &name, args, false)
        .and_then(|returned| handle(&lua, returned.into_iter().next().unwrap_or(Value::Nil)));
    match made {
        Ok(reference) => reference,
        Err(error) => refuse(call, error, ptr::null_mut()),
    }
}

pub unsafe extern "C" fn get_api(call: *mut Call, window: *mut RefHandle, name: *const c_char) -> *mut RefHandle {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return ptr::null_mut();
    };
    let (Some(target), Some(name)) = (target_of(window, &lua), unsafe { text(name) }) else {
        return refuse(call, runtime("get_api needs a window and a name"), ptr::null_mut());
    };
    let found = apply(&lua, &target, "GetAPI", vec![Out::Text(name.into_bytes())], true)
        .and_then(|returned| handle(&lua, returned.into_iter().next().unwrap_or(Value::Nil)));
    match found {
        Ok(reference) => reference,
        Err(error) => refuse(call, error, ptr::null_mut()),
    }
}

pub unsafe extern "C" fn connect(
    call: *mut Call,
    signal: *mut RefHandle,
    id: *const c_char,
    function: Option<RawFunction>,
    data: *mut c_void,
    flags: u32,
) -> i32 {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return OFF_THREAD;
    };
    let (Some(target), Some(id), Some(function)) = (target_of(signal, &lua), unsafe { text(id) }, function) else {
        return refuse(call, runtime("connect needs a signal, a name and a function"), INVALID);
    };
    let made = native_function(&lua, Entry::new(call, id.as_str().into(), function, data, flags));
    let made = match made {
        Ok(made) => made,
        Err(error) => return refuse(call, error, INVALID),
    };
    let mut values = MultiValue::new();
    values.push_back(target.clone());
    values.push_back(Value::String(match lua.create_string(&id) {
        Ok(text) => text,
        Err(error) => return refuse(call, error, INVALID),
    }));
    values.push_back(Value::Function(made));
    let bound = match field(&target, "BindHandler") {
        Ok(Value::Function(bind)) => bind.call::<()>(values),
        Ok(_) | Err(_) => Err(runtime("connect needs a Signal")),
    };
    match bound {
        Ok(()) => OK,
        Err(error) => refuse(call, error, WRONG_KIND),
    }
}

pub unsafe extern "C" fn post_call(
    target: *mut RefHandle,
    name: *const c_char,
    args: *const RawValue,
    count: i32,
) -> i32 {
    let (Some(target), Some(name)) = (unsafe { target.as_ref() }, unsafe { text(name) }) else {
        return INVALID;
    };
    let args = match unsafe { gather(args, count) } {
        Ok(args) => args,
        Err(code) => return code,
    };
    match target.send(Event::Method(target.id(), name, args)) {
        true => OK,
        false => INVALID,
    }
}

pub unsafe extern "C" fn post_write(target: *mut RefHandle, name: *const c_char, value: *const RawValue) -> i32 {
    let (Some(target), Some(name)) = (unsafe { target.as_ref() }, unsafe { text(name) }) else {
        return INVALID;
    };
    let output = match unsafe { value.as_ref() } {
        Some(value) => out_of(value),
        None => Out::Nil,
    };
    match target.send(Event::Assign(target.id(), name, output)) {
        true => OK,
        false => INVALID,
    }
}

pub unsafe extern "C" fn schedule(
    call: *mut Call,
    name: *const c_char,
    function: Option<RawFunction>,
    data: *mut c_void,
    seconds: f64,
    flags: u32,
) -> *mut Task {
    let Ok((call, lua)) = (unsafe { standing(call) }) else {
        return ptr::null_mut();
    };
    let Some(function) = function else {
        return refuse(call, runtime("schedule needs a function"), ptr::null_mut());
    };
    if !seconds.is_finite() || seconds < 0.0 {
        return refuse(call, runtime("schedule needs a gap of at least 0 seconds"), ptr::null_mut());
    }
    let label: Arc<str> = unsafe { text(name) }
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "a native task".to_owned())
        .into();
    let cancelled = Arc::new(AtomicBool::new(false));
    repeat_task(
        &lua,
        Entry::new(call, label, function, data, flags),
        Duration::from_secs_f64(seconds).max(MIN_PERIOD),
        cancelled.clone(),
    );
    Box::into_raw(Box::new(Task { cancelled }))
}

pub unsafe extern "C" fn cancel(task: *mut Task) {
    if task.is_null() {
        return;
    }
    let task = unsafe { Box::from_raw(task) };
    task.cancelled.store(true, Ordering::Release);
}

