use std::sync::Arc;

use mlua::{Lua, MetaMethod, MultiValue, Result, UserData, UserDataFields, UserDataMethods, Value};

use crate::api::thread::ThreadHandle;
use crate::runtime::{Engine, Launch, Payload, caller_path, encode, encode_value};
use crate::script::FUNCTION_HOOK;

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

fn engine(lua: &Lua) -> Result<Arc<Engine>> {
    lua.app_data_ref::<Arc<Engine>>()
        .map(|engine| engine.clone())
        .ok_or_else(|| runtime("the luv engine is not running"))
}

#[derive(Clone)]
pub struct ParallelFunction {
    path: String,
    unit: usize,
    line: usize,
    captures: Payload,
}

impl ParallelFunction {
    pub fn place(&self) -> String {
        format!("{}:{}", self.path, self.line)
    }

    pub fn spawn(&self, lua: &Lua, label: String, launch: Launch) -> Result<u64> {
        engine(lua)?
            .spawn_thread(label, self.path.clone(), self.unit, self.captures.clone(), launch)
            .map_err(|error| runtime(format!("could not start a thread for {}: {error}", self.place())))
    }
}

impl UserData for ParallelFunction {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_meta_field(MetaMethod::Type, "ParallelFunction");
        fields.add_field_method_get("Source", |_, this| Ok(this.path.clone()));
        fields.add_field_method_get("Line", |_, this| Ok(this.line));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("ParallelFunction({})", this.place()))
        });
    }
}

pub fn expect(value: &Value, call: &str, example: &str) -> Result<ParallelFunction> {
    match value {
        Value::UserData(data) if data.is::<ParallelFunction>() => Ok(data.borrow::<ParallelFunction>()?.clone()),
        Value::Function(_) => Err(runtime(format!(
            "{call} needs the function written inside the call, like {example}, because luv can only move a function to another thread when it can see the code"
        ))),
        other => Err(runtime(format!("{call} needs a function, got a {}", other.type_name()))),
    }
}

fn hook(lua: &Lua, (unit, names, captures): (usize, String, MultiValue)) -> Result<ParallelFunction> {
    let path = caller_path(lua).ok_or_else(|| runtime("parallel functions can only be made in workspace scripts"))?;
    let line = lua.inspect_stack(1, |debug| debug.current_line()).flatten().unwrap_or(0);
    let names: Vec<&str> = names.split(',').collect();
    let captures = captures
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            encode_value(lua, value).map_err(|error| {
                runtime(format!(
                    "cannot pass `{}` into the parallel function at {path}:{line}: {error}",
                    names.get(index).copied().unwrap_or("?")
                ))
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ParallelFunction {
        path,
        unit,
        line,
        captures: captures.into(),
    })
}

pub fn spawn_once(lua: &Lua, target: &Value, args: MultiValue) -> Result<ThreadHandle> {
    let function = expect(target, "task.parallel", "task.parallel(function(...) end)")?;
    let payload = encode(lua, args).map_err(|error| {
        runtime(format!("cannot pass the arguments of task.parallel at {}: {error}", function.place()))
    })?;
    let label = format!("task.parallel at {}", function.place());
    let id = function.spawn(lua, label, Launch::Once(payload))?;
    Ok(ThreadHandle::new(id))
}

pub fn install(lua: &Lua) -> Result<()> {
    lua.globals().set(FUNCTION_HOOK, lua.create_function(hook)?)
}
