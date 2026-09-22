use mlua::{AnyUserData, Lua, ObjectLike, Result, Table, Value};

use crate::objects::{Renderable, Shader};

fn describe(object: &Value) -> String {
    match object {
        Value::UserData(userdata) => userdata.to_string().unwrap_or_else(|_| "userdata".to_owned()),
        Value::Table(_) => "a table".to_owned(),
        other => other.type_name().to_owned(),
    }
}

pub fn update(updates: Table) -> Result<()> {
    for pair in updates.pairs::<Value, Value>() {
        let (object, properties) = pair?;
        let Value::Table(properties) = properties else {
            return Err(mlua::Error::runtime(format!(
                "the update for {} must be a table of properties, got {}",
                describe(&object),
                properties.type_name()
            )));
        };
        for property in properties.pairs::<Value, Value>() {
            let (key, value) = property?;
            let Value::String(name) = &key else {
                return Err(mlua::Error::runtime(format!(
                    "property names must be strings, got {} for {}",
                    key.type_name(),
                    describe(&object)
                )));
            };
            let applied = match &object {
                Value::UserData(userdata) => userdata.set(key.clone(), value),
                Value::Table(table) => table.set(key.clone(), value),
                other => Err(mlua::Error::runtime(format!("cannot update a {} value", other.type_name()))),
            };
            applied.map_err(|error| {
                mlua::Error::runtime(format!(
                    "cannot set {} on {}: {error}",
                    name.to_string_lossy(),
                    describe(&object)
                ))
            })?;
        }
    }
    Ok(())
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
    bulk.set("BulkUpdate", lua.create_function(|_, updates: Table| update(updates))?)?;
    bulk.set(
        "BulkWriteShaderData",
        lua.create_function(|_, (shader, updates): (AnyUserData, Table)| write_shader_data(shader, updates))?,
    )?;
    bulk.set_readonly(true);
    Ok(bulk)
}
