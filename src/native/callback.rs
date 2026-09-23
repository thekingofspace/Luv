use std::cell::RefCell;
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, ThreadId};

use libffi::low::ffi_cif;
use libffi::middle::Cif;
use libffi::raw::ffi_closure;
use mlua::{AnyUserData, Function, Lua, MetaMethod, MultiValue, Result, Table, UserData, UserDataFields, UserDataMethods, Value};
use tokio::sync::mpsc;

use super::NativeHost;
use super::library;
use super::marshal::{self, Writer};
use super::memory::{Owner, Pointer};
use super::types::CType;
use crate::objects::{BaseGameObject, GameObject};
use crate::runtime::Scheduler;

const REGISTRY: &str = "luv.callbacks";

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

pub enum Incoming {
    Raw(Vec<u8>),
    Text(Option<Vec<u8>>),
    Wide(Option<Vec<u32>>),
}

pub struct Request {
    id: u64,
    values: Vec<Incoming>,
    reply: crossbeam_channel::Sender<Option<Vec<u8>>>,
}

pub struct CallbackShared {
    id: u64,
    arguments: Vec<CType>,
    result: CType,
    requests: mpsc::UnboundedSender<Request>,
    home: ThreadId,
    alive: AtomicBool,
}

impl CallbackShared {
    unsafe fn capture(ty: &CType, source: *const u8) -> Incoming {
        unsafe {
            match ty {
                CType::String => match ptr::read_unaligned(source as *const usize) {
                    0 => Incoming::Text(None),
                    address => Incoming::Text(Some(CStr::from_ptr(address as *const c_char).to_bytes().to_vec())),
                },
                CType::WString => match ptr::read_unaligned(source as *const usize) {
                    0 => Incoming::Wide(None),
                    address => Incoming::Wide(Some(marshal::read_wide_string(address))),
                },
                other => Incoming::Raw(std::slice::from_raw_parts(source, other.size()).to_vec()),
            }
        }
    }

    fn invoke(&self, args: *mut *mut c_void) -> Option<Vec<u8>> {
        if !self.alive.load(Ordering::Acquire) || thread::current().id() == self.home {
            return None;
        }
        let values = self
            .arguments
            .iter()
            .enumerate()
            .map(|(index, ty)| unsafe { Self::capture(ty, *args.add(index) as *const u8) })
            .collect();
        let (reply, answer) = crossbeam_channel::bounded(1);
        self.requests
            .send(Request {
                id: self.id,
                values,
                reply,
            })
            .ok()?;
        match library::serving() {
            Some(jobs) => loop {
                crossbeam_channel::select! {
                    recv(answer) -> outcome => return outcome.ok().flatten(),
                    recv(jobs) -> job => match job {
                        Ok(job) => job.run(),
                        Err(_) => return answer.recv().ok().flatten(),
                    },
                }
            },
            None => answer.recv().ok().flatten(),
        }
    }

    unsafe fn store(&self, result: *mut u8, bytes: Option<Vec<u8>>) {
        let bytes = bytes.unwrap_or_default();
        let mut value = [0u8; 8];
        let copied = bytes.len().min(8);
        value[..copied].copy_from_slice(&bytes[..copied]);
        unsafe {
            match &self.result {
                CType::Void => {}
                CType::I8 => ptr::write_unaligned(result as *mut i64, i64::from(value[0] as i8)),
                CType::I16 => ptr::write_unaligned(result as *mut i64, i64::from(i16::from_le_bytes([value[0], value[1]]))),
                CType::I32 => ptr::write_unaligned(
                    result as *mut i64,
                    i64::from(i32::from_le_bytes([value[0], value[1], value[2], value[3]])),
                ),
                CType::Bool | CType::U8 => ptr::write_unaligned(result as *mut u64, u64::from(value[0])),
                CType::U16 => ptr::write_unaligned(result as *mut u64, u64::from(u16::from_le_bytes([value[0], value[1]]))),
                CType::U32 => ptr::write_unaligned(
                    result as *mut u64,
                    u64::from(u32::from_le_bytes([value[0], value[1], value[2], value[3]])),
                ),
                other => {
                    let size = other.size();
                    ptr::write_bytes(result, 0, size);
                    ptr::copy_nonoverlapping(bytes.as_ptr(), result, bytes.len().min(size));
                }
            }
        }
    }
}

unsafe extern "C" fn trampoline(_cif: *mut ffi_cif, result: *mut c_void, args: *mut *mut c_void, userdata: *mut c_void) {
    let shared = unsafe { &*(userdata as *const CallbackShared) };
    let bytes = panic::catch_unwind(AssertUnwindSafe(|| shared.invoke(args))).ok().flatten();
    unsafe { shared.store(result as *mut u8, bytes) };
}

pub struct Stub {
    closure: NonNull<ffi_closure>,
    code: usize,
    _cif: Box<Cif>,
    shared: Arc<CallbackShared>,
}

unsafe impl Send for Stub {}
unsafe impl Sync for Stub {}

impl Drop for Stub {
    fn drop(&mut self) {
        unsafe { libffi::low::closure_free(self.closure.as_ptr()) };
    }
}

pub struct Callback {
    base: BaseGameObject,
    stub: Option<Arc<Stub>>,
    retired: Rc<RefCell<Vec<Arc<Stub>>>>,
}

fn registry(lua: &Lua) -> Result<Table> {
    if let Some(table) = lua.named_registry_value::<Option<Table>>(REGISTRY)? {
        return Ok(table);
    }
    let table = lua.create_table()?;
    let weak = lua.create_table()?;
    weak.set("__mode", "v")?;
    table.set_metatable(Some(weak))?;
    lua.set_named_registry_value(REGISTRY, &table)?;
    Ok(table)
}

fn arguments(lua: &Lua, types: &[CType], values: Vec<Incoming>) -> Result<MultiValue> {
    let mut args = MultiValue::new();
    for (ty, value) in types.iter().zip(values) {
        args.push_back(match value {
            Incoming::Raw(bytes) => unsafe { marshal::read(lua, ty, bytes.as_ptr()) }?,
            Incoming::Text(Some(bytes)) => Value::String(lua.create_string(bytes)?),
            Incoming::Wide(Some(units)) => Value::String(lua.create_string(marshal::decode_wide(&units))?),
            Incoming::Text(None) | Incoming::Wide(None) => Value::Nil,
        });
    }
    Ok(args)
}

fn encode(lua: &Lua, ty: &CType, values: MultiValue) -> Result<Vec<u8>> {
    let value = values.into_iter().next().unwrap_or(Value::Nil);
    let mut bytes = vec![0u8; ty.size().max(8)];
    if *ty == CType::Void {
        return Ok(bytes);
    }
    if matches!(ty, CType::String | CType::WString) && matches!(value, Value::String(_)) {
        return Err(runtime(
            "a callback cannot return a Luau string as a C string, return a Pointer from DLL.String instead",
        ));
    }
    unsafe { Writer::memory().write(lua, ty, &value, bytes.as_mut_ptr()) }?;
    Ok(bytes)
}

fn answer(lua: &Lua, request: Request) -> Result<()> {
    let Some(userdata) = registry(lua)?.raw_get::<Option<AnyUserData>>(request.id)? else {
        return Ok(());
    };
    let Ok(function) = userdata.user_value::<Function>() else {
        return Ok(());
    };
    let (types, result) = {
        let callback = userdata.borrow::<Callback>()?;
        let Some(stub) = &callback.stub else {
            return Ok(());
        };
        (stub.shared.arguments.clone(), stub.shared.result.clone())
    };
    let args = arguments(lua, &types, request.values)?;
    let scheduler = Scheduler::get(lua)?;
    let reply = request.reply;
    let (state, reporter) = (lua.clone(), scheduler.clone());
    scheduler.spawn_returning(lua, function, args, move |values| {
        let bytes = values.and_then(|values| match encode(&state, &result, values) {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                reporter.report(error);
                None
            }
        });
        let _ = reply.send(bytes);
    });
    Ok(())
}

pub fn dispatcher(lua: &Lua) -> mpsc::UnboundedSender<Request> {
    let (sender, mut receiver) = mpsc::unbounded_channel::<Request>();
    let lua = lua.clone();
    tokio::task::spawn_local(async move {
        while let Some(request) = receiver.recv().await {
            if let Err(error) = answer(&lua, request)
                && let Ok(scheduler) = Scheduler::get(&lua)
            {
                scheduler.report(error);
            }
        }
    });
    sender
}

impl Callback {
    pub const CLASS_NAME: &'static str = "Callback";

    pub fn create(lua: &Lua, result: &Value, types: Option<Table>, handler: Function) -> Result<AnyUserData> {
        let result = CType::parse_result(result)?;
        let arguments = CType::parse_list(types)?;
        let (id, requests, retired) = NativeHost::callbacks(lua)?;
        let shared = Arc::new(CallbackShared {
            id,
            arguments: arguments.clone(),
            result: result.clone(),
            requests,
            home: thread::current().id(),
            alive: AtomicBool::new(true),
        });
        let cif = Box::new(Cif::new(arguments.iter().map(CType::ffi).collect::<Vec<_>>(), result.ffi()));
        let (closure, code) =
            libffi::low::try_closure_alloc().ok_or_else(|| runtime("the callback could not be allocated"))?;
        let closure = NonNull::new(closure).ok_or_else(|| runtime("the callback could not be allocated"))?;
        let status = unsafe {
            libffi::raw::ffi_prep_closure_loc(
                closure.as_ptr(),
                cif.as_raw_ptr(),
                Some(trampoline),
                Arc::as_ptr(&shared) as *mut c_void,
                code.as_mut_ptr(),
            )
        };
        if status != libffi::raw::ffi_status_FFI_OK {
            unsafe { libffi::low::closure_free(closure.as_ptr()) };
            return Err(runtime("the callback could not be prepared for this signature"));
        }
        let stub = Arc::new(Stub {
            closure,
            code: code.as_ptr() as usize,
            _cif: cif,
            shared,
        });
        let userdata = lua.create_userdata(Callback {
            base: BaseGameObject::new(Self::CLASS_NAME),
            stub: Some(stub),
            retired,
        })?;
        userdata.set_user_value(handler)?;
        registry(lua)?.raw_set(id, &userdata)?;
        Ok(userdata)
    }

    pub fn pointer(&self) -> Result<Pointer> {
        let stub = self
            .stub
            .as_ref()
            .ok_or_else(|| runtime(format!("{} '{}' has been destroyed", Self::CLASS_NAME, self.base.name())))?;
        Ok(Pointer {
            address: stub.code,
            owner: Owner::Callback(stub.clone()),
        })
    }
}

impl GameObject for Callback {
    fn base(&self) -> &BaseGameObject {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseGameObject {
        &mut self.base
    }

    fn on_destroy(&mut self) {
        if let Some(stub) = self.stub.take() {
            stub.shared.alive.store(false, Ordering::Release);
            self.retired.borrow_mut().push(stub);
        }
    }
}

impl Drop for Callback {
    fn drop(&mut self) {
        if let Some(stub) = self.stub.take() {
            stub.shared.alive.store(false, Ordering::Release);
            self.retired.borrow_mut().push(stub);
        }
    }
}

impl UserData for Callback {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        Self::add_base_fields(fields);
        fields.add_field_method_get("Pointer", |_, this| this.pointer());
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("Destroy", |_, userdata: AnyUserData| {
            userdata.borrow_mut::<Callback>()?.destroy();
            userdata.set_user_value(Value::Nil)
        });
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| Ok(this.base.name().to_owned()));
    }
}
