pub(crate) mod asset;
mod bulk;
pub mod codec;
mod container;
mod crypto;
mod dll;
mod fs;
mod net;
mod process;
mod random;
pub(crate) mod renderable;
mod shader;
mod viewport;
mod window;

pub(crate) use asset::load as load_asset;

use std::sync::Arc;

use mlua::{AnyUserData, Lua, Result, Value};

use crate::objects::Signal;
use crate::runtime::Engine;

pub fn libraries(lua: &Lua, engine: &Arc<Engine>, heartbeat: &AnyUserData) -> Result<Vec<(&'static str, Value)>> {
    let signal = lua.create_table()?;
    signal.set("new", lua.create_function(|_, ()| Ok(Signal::new()))?)?;
    signal.set_readonly(true);
    Ok(vec![
        ("Asset", Value::Table(asset::create(lua, engine)?)),
        ("Bulk", Value::Table(bulk::create(lua)?)),
        ("Container", Value::Table(container::create(lua, engine)?)),
        ("Crypto", Value::Table(crypto::create(lua)?)),
        ("DLL", Value::Table(dll::create(lua)?)),
        ("FS", Value::Table(fs::create(lua, engine)?)),
        ("Net", Value::Table(net::create(lua)?)),
        ("Process", Value::Table(process::create(lua, engine, heartbeat)?)),
        ("Random", Value::Table(random::create(lua)?)),
        ("Serde", Value::Table(codec::create(lua)?)),
        ("Shader", Value::Table(shader::create(lua)?)),
        ("Signal", Value::Table(signal)),
        ("Viewport", Value::Table(viewport::create(lua, engine)?)),
        ("Window", Value::Table(window::create(lua)?)),
    ])
}
