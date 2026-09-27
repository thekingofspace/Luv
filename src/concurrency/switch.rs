use std::collections::HashMap;
use std::rc::Rc;

use mlua::{Function, Lua, LuaString, MultiValue, Result, Table, Value};

use super::joined;

struct Cases {
    map: HashMap<Box<[u8]>, Function>,
    fallback: Option<Function>,
}

impl Cases {
    fn find(&self, key: &LuaString) -> Option<Function> {
        let bytes = key.as_bytes();
        self.map.get(&*bytes).cloned()
    }
}

fn build(cases: &Table, fallback: Option<Function>) -> Result<Cases> {
    let mut map = HashMap::new();
    cases.for_each(|key: Value, handler: Value| {
        let name = match key {
            Value::String(name) => name,
            other => {
                return Err(mlua::Error::runtime(format!(
                    "switch.new takes a table of strings to functions, got a {} key",
                    other.type_name()
                )));
            }
        };
        let handler = match handler {
            Value::Function(handler) => handler,
            other => {
                return Err(mlua::Error::runtime(format!(
                    "the case '{}' must be a function, got a {}",
                    name.to_string_lossy(),
                    other.type_name()
                )));
            }
        };
        map.insert(Box::from(&*name.as_bytes()), handler);
        Ok(())
    })?;
    if map.is_empty() && fallback.is_none() {
        return Err(mlua::Error::runtime("switch.new needs at least one case"));
    }
    Ok(Cases { map, fallback })
}

async fn dispatch(cases: Rc<Cases>, mut args: MultiValue) -> Result<MultiValue> {
    let key = match args.pop_front() {
        Some(Value::String(key)) => key,
        Some(other) => {
            return Err(mlua::Error::runtime(format!(
                "a switch is called with a string, got a {}",
                other.type_name()
            )));
        }
        None => return Err(mlua::Error::runtime("a switch is called with a string")),
    };
    if let Some(handler) = cases.find(&key) {
        return handler.call_async(args).await;
    }
    match &cases.fallback {
        Some(handler) => handler.call_async(joined(Value::String(key), args)).await,
        None => Err(mlua::Error::runtime(format!(
            "no case for '{}'",
            key.to_string_lossy()
        ))),
    }
}

fn new(lua: &Lua, (cases, fallback): (Table, Option<Function>)) -> Result<Function> {
    let cases = Rc::new(build(&cases, fallback)?);
    lua.create_async_function(move |_, args: MultiValue| {
        let cases = cases.clone();
        dispatch(cases, args)
    })
}

pub fn install(lua: &Lua) -> Result<()> {
    let library = lua.create_table()?;
    library.set("new", lua.create_function(new)?)?;
    lua.globals().set("switch", library)
}
