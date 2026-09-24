use mlua::{Lua, Result, Table, Value};

const IMPORTS: &str = "luv.imports";
const NAMES: &str = "luv.imports.names";
const EXTRA: &str = "luv.imports.extra";

pub fn install(lua: &Lua, entries: impl IntoIterator<Item = (&'static str, Value)>) -> Result<()> {
    let imports = lua.create_table()?;
    let names = lua.create_table()?;
    for (name, value) in entries {
        imports.raw_set(name, value.clone())?;
        names.raw_set(value, name)?;
    }
    imports.set_readonly(true);
    lua.set_named_registry_value(IMPORTS, imports)?;
    lua.set_named_registry_value(NAMES, names)?;
    lua.set_named_registry_value(EXTRA, lua.create_table()?)?;
    lua.globals()
        .set("import", lua.create_function(|lua, name: String| get(lua, &name))?)
}

fn extra(lua: &Lua) -> Result<Table> {
    if let Some(table) = lua.named_registry_value::<Option<Table>>(EXTRA)? {
        return Ok(table);
    }
    let table = lua.create_table()?;
    lua.set_named_registry_value(EXTRA, &table)?;
    Ok(table)
}

pub fn provide(lua: &Lua, name: &str, value: Value) -> Result<()> {
    let imports: Table = lua.named_registry_value(IMPORTS)?;
    if !imports.raw_get::<Value>(name)?.is_nil() {
        return Err(mlua::Error::runtime(format!(
            "'{name}' cannot be a service because luv already imports a library with that name"
        )));
    }
    if let Some(names) = lua.named_registry_value::<Option<Table>>(NAMES)? {
        names.raw_set(value.clone(), name)?;
    }
    extra(lua)?.raw_set(name, value)
}

pub fn get(lua: &Lua, name: &str) -> Result<Value> {
    let imports: Table = lua.named_registry_value(IMPORTS)?;
    let found = match imports.raw_get::<Value>(name)? {
        Value::Nil => extra(lua)?.raw_get::<Value>(name)?,
        value => value,
    };
    match found {
        Value::Nil => {
            let mut available = imports
                .pairs::<String, Value>()
                .chain(extra(lua)?.pairs::<String, Value>())
                .filter_map(|pair| pair.ok().map(|(name, _)| name))
                .collect::<Vec<_>>();
            available.sort();
            Err(mlua::Error::runtime(format!(
                "'{name}' cannot be imported, the available imports are {}",
                available.join(", ")
            )))
        }
        value => Ok(value),
    }
}

pub fn name_of(lua: &Lua, value: &Value) -> Result<Option<String>> {
    match lua.named_registry_value::<Option<Table>>(NAMES)? {
        Some(names) => names.raw_get(value.clone()),
        None => Ok(None),
    }
}
