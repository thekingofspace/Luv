use mlua::{AnyUserData, Function, Lua, ObjectLike, Result, Table, Value};

use crate::objects::{Renderable, Shader};

const APPLY: &str = r#"
local blame = ...
local type = type
return function(updates)
    local object, field
    local ok, problem = pcall(function()
        for target, properties in updates do
            object, field = target, nil
            if type(properties) ~= "table" then
                error(nil, 0)
            end
            for name, value in properties do
                field = name
                if type(name) ~= "string" then
                    error(nil, 0)
                end
                target[name] = value
            end
        end
    end)
    if not ok then
        blame(updates, object, field, problem)
    end
end
"#;

fn describe(object: &Value) -> String {
    match object {
        Value::UserData(userdata) => userdata.to_string().unwrap_or_else(|_| "userdata".to_owned()),
        Value::Table(_) => "a table".to_owned(),
        other => other.type_name().to_owned(),
    }
}

fn reason(problem: &Value) -> String {
    match problem {
        Value::String(text) => text.to_string_lossy(),
        Value::Error(error) => error.to_string(),
        other => other.type_name().to_owned(),
    }
}

fn explain(updates: &Table, object: &Value, field: &Value, problem: &Value) -> mlua::Error {
    let properties = updates.get::<Value>(object.clone()).unwrap_or(Value::Nil);
    if !matches!(properties, Value::Table(_)) {
        return mlua::Error::runtime(format!(
            "the update for {} must be a table of properties, got {}",
            describe(object),
            properties.type_name()
        ));
    }
    match field {
        Value::String(name) => mlua::Error::runtime(format!(
            "cannot set {} on {}: {}",
            name.to_string_lossy(),
            describe(object),
            reason(problem)
        )),
        Value::Nil => mlua::Error::runtime(format!("cannot update {}", describe(object))),
        other => mlua::Error::runtime(format!(
            "property names must be strings, got {} for {}",
            other.type_name(),
            describe(object)
        )),
    }
}

pub fn write_shader_data(shader: AnyUserData, updates: Table) -> Result<()> {
    if !shader.is::<Shader>() {
        return Err(mlua::Error::runtime("BulkWriteShaderData expects a Shader as its first argument"));
    }
    for pair in updates.pairs::<Value, Value>() {
        let (object, data) = pair?;
        let renderable = match &object {
            Value::UserData(userdata) => userdata.borrow::<Renderable>().ok(),
            _ => None,
        };
        let Some(renderable) = renderable else {
            return Err(mlua::Error::runtime(format!(
                "the keys of BulkWriteShaderData must be renderables, got {}",
                describe(&object)
            )));
        };
        renderable.write_values(&shader, data).map_err(|error| {
            mlua::Error::runtime(format!("cannot write shader data for {}: {error}", describe(&object)))
        })?;
    }
    Ok(())
}

pub fn create(lua: &Lua) -> Result<Table> {
    let bulk = lua.create_table()?;
    let blame = lua.create_function(
        |_, (updates, object, field, problem): (Table, Value, Value, Value)| -> Result<()> {
            Err(explain(&updates, &object, &field, &problem))
        },
    )?;
    let apply: Function = lua.load(APPLY).set_name("=luv.bulk").call(blame)?;
    bulk.set("BulkUpdate", apply)?;
    bulk.set(
        "BulkWriteShaderData",
        lua.create_function(|_, (shader, updates): (AnyUserData, Table)| write_shader_data(shader, updates))?,
    )?;
    bulk.set_readonly(true);
    Ok(bulk)
}
