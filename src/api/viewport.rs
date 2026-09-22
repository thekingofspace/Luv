use std::sync::Arc;

use mlua::{AnyUserData, Lua, Result, Table, Value};
use tokio::sync::oneshot;

use crate::datatypes::UDim;
use crate::objects::Window;
use crate::runtime::Engine;
use crate::window::{Screen, WindowId};

fn runtime(message: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(message.into())
}

async fn screens(engine: &Engine, window: Option<WindowId>) -> Result<Vec<Screen>> {
    let system = engine
        .windows()
        .cloned()
        .ok_or_else(|| runtime("screens are not available because no display could be opened"))?;
    let (reply, answer) = oneshot::channel();
    system.screens(
        window,
        Box::new(move |screens| {
            let _ = reply.send(screens);
        }),
    );
    answer
        .await
        .map_err(|_| runtime("the window system stopped before it could list the screens"))
}

fn udim((x, y): (f64, f64)) -> UDim {
    UDim::new(x, y, 0.0)
}

fn describe(lua: &Lua, screen: &Screen) -> Result<Table> {
    let table = lua.create_table()?;
    table.set("Name", screen.name.clone())?;
    table.set("Position", udim(screen.position))?;
    table.set("Size", udim(screen.size))?;
    table.set(
        "PixelSize",
        udim((f64::from(screen.pixel_size.0), f64::from(screen.pixel_size.1))),
    )?;
    table.set("WorkPosition", udim(screen.work_position))?;
    table.set("WorkSize", udim(screen.work_size))?;
    table.set("Scale", screen.scale)?;
    table.set("RefreshRate", screen.refresh_rate)?;
    table.set("IsPrimary", screen.primary)?;
    Ok(table)
}

fn primary(found: &[Screen]) -> Result<&Screen> {
    found
        .iter()
        .find(|screen| screen.primary)
        .or_else(|| found.first())
        .ok_or_else(|| runtime("no screens are connected"))
}

pub fn create(lua: &Lua, engine: &Arc<Engine>) -> Result<Table> {
    let viewport = lua.create_table()?;

    viewport.set("GetScreens", {
        let engine = engine.clone();
        lua.create_async_function(move |lua, ()| {
            let engine = engine.clone();
            async move {
                let found = screens(&engine, None).await?;
                let list = found.iter().map(|screen| describe(&lua, screen)).collect::<Result<Vec<_>>>()?;
                lua.create_sequence_from(list)
            }
        })?
    })?;
    viewport.set("GetPrimaryScreen", {
        let engine = engine.clone();
        lua.create_async_function(move |lua, ()| {
            let engine = engine.clone();
            async move {
                let found = screens(&engine, None).await?;
                describe(&lua, primary(&found)?)
            }
        })?
    })?;
    viewport.set("GetScreenSize", {
        let engine = engine.clone();
        lua.create_async_function(move |_, ()| {
            let engine = engine.clone();
            async move {
                let found = screens(&engine, None).await?;
                Ok(udim(primary(&found)?.size))
            }
        })?
    })?;
    viewport.set("GetWindowScreen", {
        let engine = engine.clone();
        lua.create_async_function(move |lua, window: AnyUserData| {
            let engine = engine.clone();
            async move {
                let id = window
                    .borrow::<Window>()
                    .map_err(|_| runtime("GetWindowScreen expects a Window"))?
                    .id();
                let found = screens(&engine, Some(id)).await?;
                match found.iter().find(|screen| screen.current) {
                    Some(screen) => Ok(Value::Table(describe(&lua, screen)?)),
                    None => Ok(Value::Nil),
                }
            }
        })?
    })?;

    viewport.set_readonly(true);
    Ok(viewport)
}
