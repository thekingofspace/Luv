mod callback;
mod classes;
mod library;
mod marshal;
mod memory;
mod types;

pub use callback::Callback;
pub use classes::{API_VERSION, Exports, ObjectData};
pub use library::{EXTENSION, Library, NativeFunction};
pub use memory::{Hold, Owner, Pointer, raw_bytes};
pub use types::{ArrayType, CType, StructType, allocate};

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use mlua::{Lua, Result};
use tokio::sync::mpsc;

use self::callback::{Request, Stub};
use self::library::Worker;

#[derive(Default)]
pub struct NativeHost {
    requests: Option<mpsc::UnboundedSender<Request>>,
    retired: Rc<RefCell<Vec<Arc<Stub>>>>,
    next: u64,
    detached: Option<Arc<Worker>>,
}

type CallbackParts = (u64, mpsc::UnboundedSender<Request>, Rc<RefCell<Vec<Arc<Stub>>>>);

impl NativeHost {
    fn with<R>(lua: &Lua, action: impl FnOnce(&mut NativeHost) -> R) -> R {
        if lua.app_data_ref::<NativeHost>().is_none() {
            lua.set_app_data(NativeHost::default());
        }
        let mut host = lua
            .app_data_mut::<NativeHost>()
            .unwrap_or_else(|| unreachable!("the native host was just installed"));
        action(&mut host)
    }

    fn detached(lua: &Lua) -> Result<Arc<Worker>> {
        if let Some(worker) = NativeHost::with(lua, |host| host.detached.clone()) {
            return Ok(worker);
        }
        let worker = Worker::detached().map_err(|error| mlua::Error::runtime(format!("cannot start the DLL call thread: {error}")))?;
        NativeHost::with(lua, |host| host.detached = Some(worker.clone()));
        Ok(worker)
    }

    fn callbacks(lua: &Lua) -> Result<CallbackParts> {
        let existing = NativeHost::with(lua, |host| host.requests.clone());
        let requests = match existing {
            Some(requests) => requests,
            None => {
                let requests = callback::dispatcher(lua);
                NativeHost::with(lua, |host| host.requests = Some(requests.clone()));
                requests
            }
        };
        Ok(NativeHost::with(lua, |host| {
            host.next += 1;
            (host.next, requests, host.retired.clone())
        }))
    }
}
