use mlua::{Lua, Result, Table, Value};

const IMPORTS: &str = "luv.imports";
const NAMES: &str = "luv.imports.names";

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
    lua.globals()
        .set("import", lua.create_function(|lua, name: String| get(lua, &name))?)
}

pub fn get(lua: &Lua, name: &str) -> Result<Value> {
    let imports: Table = lua.named_registry_value(IMPORTS)?;
    match imports.raw_get::<Value>(name)? {
        Value::Nil => {
            let mut available = imports
                .pairs::<String, Value>()
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
