use std::io;
use std::sync::Arc;

use mlua::{Lua, Result, Table};

use crate::objects::Asset;
use crate::project::{ASSETS_DIR, is_script};
use crate::runtime::Engine;
use crate::vfs::{self, Vfs};

fn locate(vfs: &dyn Vfs, path: &str) -> io::Result<String> {
    let relative = vfs::normalize(path)
        .filter(|relative| !relative.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "asset paths must name a file inside the assets folder"))?;
    let full = vfs::join(ASSETS_DIR, &relative);
    let found = if vfs.is_file(&full) {
        full
    } else {
        let (directory, stem) = full.rsplit_once('/').unwrap_or(("", full.as_str()));
        let mut matches: Vec<String> = vfs
            .read_dir(directory)
            .unwrap_or_default()
            .into_iter()
            .filter(|name| name.rsplit_once('.').is_some_and(|(base, _)| base == stem))
            .map(|name| vfs::join(directory, &name))
            .filter(|candidate| vfs.is_file(candidate))
            .collect();
        match matches.len() {
            0 => return Err(io::Error::new(io::ErrorKind::NotFound, "no such asset")),
            1 => matches.remove(0),
            _ => {
                return Err(io::Error::other(format!(
                    "the name is ambiguous, it matches {}",
                    matches.join(", ")
                )));
            }
        }
    };
    if is_script(&found) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "scripts can only be loaded with require",
        ));
    }
    Ok(found)
}

pub(crate) async fn load(engine: Arc<Engine>, path: String) -> Result<(String, Arc<[u8]>)> {
    let requested = path.clone();
    let (found, data) = tokio::task::spawn_blocking(move || {
        let vfs = engine.vfs();
        let found = locate(vfs.as_ref(), &path)?;
        if let Some(data) = engine.assets().get(&found) {
            return Ok::<_, io::Error>((found, data));
        }
        let data: Arc<[u8]> = Arc::from(vfs.read(&found)?);
        let data = engine.assets().share(&found, data);
        Ok((found, data))
    })
    .await
    .map_err(mlua::Error::external)?
    .map_err(|error| mlua::Error::runtime(format!("cannot load asset '{requested}': {error}")))?;
    let relative = found
        .strip_prefix(ASSETS_DIR)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(&found)
        .to_owned();
    Ok((relative, data))
}

pub fn create(lua: &Lua, engine: &Arc<Engine>) -> Result<Table> {
    let asset = lua.create_table()?;

    asset.set("LoadString", {
        let engine = engine.clone();
        lua.create_async_function(move |lua, path: String| {
            let engine = engine.clone();
            async move {
                let (_, data) = load(engine, path).await?;
                lua.create_string(&*data)
            }
        })?
    })?;

    asset.set("Load", {
        let engine = engine.clone();
        lua.create_async_function(move |lua, path: String| {
            let engine = engine.clone();
            async move {
                let (path, data) = load(engine, path).await?;
                lua.create_userdata(Asset::new(path, data))
            }
        })?
    })?;

    asset.set_readonly(true);
    Ok(asset)
}
