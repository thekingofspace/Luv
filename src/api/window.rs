use mlua::{Lua, Result, Table};

use crate::objects::Window;

pub fn create(lua: &Lua) -> Result<Table> {
    let window = lua.create_table()?;
    window.set("new", lua.create_function(|lua, config: Option<Table>| Window::open(lua, config))?)?;
    window.set_readonly(true);
    Ok(window)
}
