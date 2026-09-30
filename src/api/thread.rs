use std::sync::Arc;
use std::time::Duration;

use mlua::{
    AnyUserData, Lua, MetaMethod, MultiValue, Result, Table, UserData, UserDataFields, UserDataMethods, Value,
};

use crate::runtime::{CurrentThread, Engine, Message, ThreadEntry, decode_value, encode, encode_value};

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

fn engine(lua: &Lua) -> Result<Arc<Engine>> {
    lua.app_data_ref::<Arc<Engine>>()
        .map(|engine| engine.clone())
        .ok_or_else(|| runtime("the luv engine is not running"))
}

pub fn current(lua: &Lua) -> Result<u64> {
    lua.app_data_ref::<CurrentThread>()
        .map(|thread| thread.0)
        .ok_or_else(|| runtime("this code is not running on a luv thread yet"))
}

fn duration(seconds: Option<f64>) -> Result<Option<Duration>> {
    match seconds {
        None => Ok(None),
        Some(seconds) if seconds.is_finite() && seconds >= 0.0 => Ok(Some(Duration::from_secs_f64(seconds))),
        Some(_) => Err(runtime("the timeout must be 0 seconds or more")),
    }
}

async fn wait_until(engine: &Engine, timeout: Option<Duration>, found: impl Fn(&[ThreadEntry]) -> bool) -> bool {
    let deadline = timeout.map(|timeout| tokio::time::Instant::now() + timeout);
    loop {
        let notified = engine.threads().changed().notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if found(&engine.threads().list()) {
            return true;
        }
        match deadline {
            Some(deadline) => {
                if tokio::time::timeout_at(deadline, notified).await.is_err() {
                    return found(&engine.threads().list());
                }
            }
            None => notified.await,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ThreadHandle {
    id: u64,
}

impl ThreadHandle {
    pub fn new(id: u64) -> Self {
        Self { id }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    fn entry(&self, lua: &Lua) -> Result<Option<ThreadEntry>> {
        Ok(engine(lua)?.threads().get(self.id))
    }
}

impl UserData for ThreadHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "Thread");
        fields.add_field_method_get("ClassName", |_, _| Ok("Thread"));
        fields.add_field_method_get("Id", |_, this| Ok(this.id));
        fields.add_field_method_get("Name", |lua, this| Ok(this.entry(lua)?.map(|entry| entry.name)));
        fields.add_field_method_get("State", |lua, this| Ok(this.entry(lua)?.and_then(|entry| entry.state)));
        fields.add_field_method_get("Data", |lua, this| match this.entry(lua)?.and_then(|entry| entry.data) {
            Some(data) => decode_value(lua, &data),
            None => Ok(Value::Nil),
        });
        fields.add_field_method_get("IsMain", |lua, this| Ok(this.entry(lua)?.is_some_and(|entry| entry.main)));
        fields.add_field_method_get("IsReady", |lua, this| Ok(this.entry(lua)?.is_some_and(|entry| entry.ready)));
        fields.add_field_method_get("IsAlive", |lua, this| Ok(this.entry(lua)?.is_some()));
        fields.add_field_method_get("IsCurrent", |lua, this| Ok(current(lua)? == this.id));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |lua, this, ()| {
            Ok(match this.entry(lua)? {
                Some(entry) => format!("Thread({})", entry.name),
                None => format!("Thread({}, ended)", this.id),
            })
        });
        methods.add_meta_function(MetaMethod::Eq, |_, (left, right): (AnyUserData, AnyUserData)| {
            Ok(match (left.borrow::<ThreadHandle>(), right.borrow::<ThreadHandle>()) {
                (Ok(left), Ok(right)) => left.id == right.id,
                _ => false,
            })
        });
        methods.add_method("Send", |lua, this, (topic, args): (String, MultiValue)| {
            let payload = encode(lua, args).map_err(|error| runtime(format!("cannot send '{topic}': {error}")))?;
            Ok(engine(lua)?.bus().send_to(
                this.id,
                Message::Topic {
                    topic: topic.as_str().into(),
                    payload,
                },
            ))
        });
        methods.add_method("MarkReady", |lua, this, ()| {
            if current(lua)? != this.id {
                return Err(runtime("only a thread can mark itself ready, call it on Thread.Running()"));
            }
            let mut first = false;
            engine(lua)?.threads().update(this.id, |entry| {
                first = !entry.ready;
                entry.ready = true;
            });
            if first {
                crate::runtime::boot_ready(lua)?;
            }
            Ok(first)
        });
        methods.add_async_method("WaitReady", |lua, this, timeout: Option<f64>| async move {
            let timeout = duration(timeout)?;
            let engine = engine(&lua)?;
            let id = this.id;
            let ready = wait_until(&engine, timeout, |entries| {
                entries.iter().find(|entry| entry.id == id).is_none_or(|entry| entry.ready)
            })
            .await;
            Ok(ready && engine.threads().get(id).is_some_and(|entry| entry.ready))
        });
    }
}

fn handles(lua: &Lua, entries: impl IntoIterator<Item = ThreadEntry>) -> Result<Table> {
    lua.create_sequence_from(entries.into_iter().map(|entry| ThreadHandle::new(entry.id)))
}

pub fn create(lua: &Lua) -> Result<Table> {
    let library = lua.create_table()?;
    library.set(
        "Running",
        lua.create_function(|lua, ()| Ok(ThreadHandle::new(current(lua)?)))?,
    )?;
    library.set(
        "Get",
        lua.create_function(|lua, state: Option<String>| {
            let mut entries = engine(lua)?.threads().list();
            entries.sort_by_key(|entry| (!entry.main, entry.id));
            handles(
                lua,
                entries
                    .into_iter()
                    .filter(|entry| state.is_none() || entry.state == state),
            )
        })?,
    )?;
    library.set(
        "Set",
        lua.create_function(|lua, (state, data): (Option<String>, Value)| {
            let data = match data {
                Value::Nil => None,
                value => Some(encode_value(lua, value).map_err(|error| runtime(format!("Thread.Set cannot keep that Data: {error}")))?),
            };
            let id = current(lua)?;
            engine(lua)?.threads().update(id, |entry| {
                entry.state = state;
                entry.data = data;
            });
            Ok(())
        })?,
    )?;
    library.set(
        "WaitFor",
        lua.create_async_function(|lua, (state, timeout): (String, Option<f64>)| async move {
            let timeout = duration(timeout)?;
            let engine = engine(&lua)?;
            wait_until(&engine, timeout, |entries| {
                entries.iter().any(|entry| entry.state.as_deref() == Some(state.as_str()))
            })
            .await;
            let found = engine
                .threads()
                .list()
                .into_iter()
                .filter(|entry| entry.state.as_deref() == Some(state.as_str()))
                .min_by_key(|entry| entry.id);
            Ok(found.map(|entry| ThreadHandle::new(entry.id)))
        })?,
    )?;
    library.set_readonly(true);
    Ok(library)
}
