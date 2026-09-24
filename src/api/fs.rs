use std::cell::RefCell;
use std::future::Future;
use std::io::{self, Cursor};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use mlua::{AnyUserData, FromLuaMulti, Function, IntoLuaMulti, Lua, MultiValue, Result, Table, Value};
use tokio::io::AsyncWriteExt;

use crate::objects::File;
use crate::objects::file::{Format, Handle, read_only};
use crate::project::is_script;
use crate::runtime::aliases::{self, Resolved};
use crate::runtime::{Engine, caller_path, module_of};
use crate::vfs::{self, Vfs};

enum Location {
    Ready(Resolved),
    Alias { module: String, alias: String, rest: String },
}

#[derive(Clone)]
struct Paths {
    vfs: Arc<dyn Vfs>,
}

impl Paths {
    fn locate(&self, lua: &Lua, path: &str) -> Result<Location> {
        let module = caller_path(lua).and_then(|script| module_of(&script));
        if let Some((alias, rest)) = aliases::split_alias(path) {
            return Ok(Location::Alias {
                module: module.unwrap_or_default(),
                alias,
                rest: rest.to_owned(),
            });
        }
        if !is_relative(path) {
            return Ok(Location::Ready(Resolved::Disk(PathBuf::from(path))));
        }
        let base = match module {
            Some(module) if module.is_empty() => None,
            Some(module) => Some(module.rsplit_once('/').map_or(String::new(), |(parent, _)| parent.to_owned())),
            None => Some(String::new()),
        };
        base.and_then(|base| vfs::normalize(&vfs::join(&base, path)))
            .map(|path| Location::Ready(Resolved::Game(path)))
            .ok_or_else(|| mlua::Error::runtime(format!("{path} points outside the game's files")))
    }

    async fn resolve(&self, location: Location) -> Result<Resolved> {
        match location {
            Location::Ready(resolved) => Ok(resolved),
            Location::Alias { module, alias, rest } => {
                let vfs = self.vfs.clone();
                blocking(move || Ok(aliases::resolve(vfs.as_ref(), &module, &alias, &rest)))
                    .await
                    .map_err(mlua::Error::external)?
                    .map_err(mlua::Error::runtime)
            }
        }
    }

    async fn read_game(&self, path: String) -> io::Result<Vec<u8>> {
        if is_script(&path) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "scripts can only be loaded with require",
            ));
        }
        let vfs = self.vfs.clone();
        blocking(move || {
            if vfs.is_file(&path) {
                vfs.read(&path)
            } else {
                Err(io::Error::new(io::ErrorKind::NotFound, "no such file"))
            }
        })
        .await
    }

    async fn game_kind(&self, path: String) -> io::Result<Option<&'static str>> {
        let vfs = self.vfs.clone();
        blocking(move || {
            Ok(if vfs.is_file(&path) {
                Some("file")
            } else if vfs.is_dir(&path) {
                Some("dir")
            } else {
                None
            })
        })
        .await
    }
}

fn is_relative(path: &str) -> bool {
    path == "." || path == ".." || ["./", ".\\", "../", "..\\"].iter().any(|prefix| path.starts_with(prefix))
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> io::Result<T> + Send + 'static) -> io::Result<T> {
    tokio::task::spawn_blocking(work).await.map_err(io::Error::other)?
}

fn failure(action: &str, path: &str, error: impl std::fmt::Display) -> mlua::Error {
    mlua::Error::runtime(format!("cannot {action} {path}: {error}"))
}

fn fail_path(lua: &Lua, path: &str, error: &io::Error) -> MultiValue {
    let message = lua
        .create_string(format!("{path}: {error}"))
        .map(Value::String)
        .unwrap_or(Value::Nil);
    MultiValue::from_vec(vec![
        Value::Nil,
        message,
        Value::Integer(error.raw_os_error().unwrap_or(0) as i64),
    ])
}

fn contents(value: Value, function: &str) -> Result<Vec<u8>> {
    match value {
        Value::String(text) => Ok(text.as_bytes().to_vec()),
        Value::Buffer(buffer) => Ok(buffer.to_vec()),
        other => Err(mlua::Error::runtime(format!(
            "bad argument #2 to '{function}' (string or buffer expected, got {})",
            other.type_name()
        ))),
    }
}

fn seconds(time: io::Result<SystemTime>) -> Option<f64> {
    time.ok()?.duration_since(UNIX_EPOCH).ok().map(|duration| duration.as_secs_f64())
}

#[derive(Clone, Copy)]
struct Mode {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    text: bool,
}

impl Mode {
    const READ: Mode = Mode {
        read: true,
        write: false,
        append: false,
        truncate: false,
        create: false,
        text: true,
    };

    const WRITE: Mode = Mode {
        read: false,
        write: true,
        append: false,
        truncate: true,
        create: true,
        text: true,
    };

    fn parse(mode: &str) -> Option<Mode> {
        let (kind, rest) = mode.split_at_checked(1)?;
        let (plus, rest) = match rest.strip_prefix('+') {
            Some(rest) => (true, rest),
            None => (false, rest),
        };
        if !rest.chars().all(|ch| ch == 'b') {
            return None;
        }
        let text = rest.is_empty();
        Some(match kind {
            "r" => Mode { write: plus, text, ..Mode::READ },
            "w" => Mode { read: plus, text, ..Mode::WRITE },
            "a" => Mode {
                read: plus,
                write: false,
                append: true,
                truncate: false,
                create: true,
                text,
            },
            _ => return None,
        })
    }

    fn writes(&self) -> bool {
        self.write || self.append
    }

    fn options(&self) -> tokio::fs::OpenOptions {
        let mut options = tokio::fs::OpenOptions::new();
        options
            .read(self.read)
            .write(self.write)
            .append(self.append)
            .truncate(self.truncate)
            .create(self.create);
        options
    }
}

async fn open_file(lua: &Lua, paths: &Paths, resolved: Resolved, name: &str, mode: Mode) -> Result<io::Result<AnyUserData>> {
    let handle = match resolved {
        Resolved::Game(path) => {
            if mode.writes() {
                return Ok(Err(read_only()));
            }
            match paths.read_game(path).await {
                Ok(bytes) => Handle::Memory(Cursor::new(Arc::from(bytes))),
                Err(error) => return Ok(Err(error)),
            }
        }
        Resolved::Disk(path) => match mode.options().open(&path).await {
            Ok(file) => Handle::File(file),
            Err(error) => return Ok(Err(error)),
        },
    };
    Ok(Ok(lua.create_userdata(File::new(name, handle, mode.text))?))
}

async fn copy_disk(from: PathBuf, to: PathBuf) -> io::Result<()> {
    if !tokio::fs::metadata(&from).await?.is_dir() {
        tokio::fs::copy(&from, &to).await?;
        return Ok(());
    }
    let mut pending = vec![(from, to)];
    while let Some((source, target)) = pending.pop() {
        tokio::fs::create_dir_all(&target).await?;
        let mut entries = tokio::fs::read_dir(&source).await?;
        while let Some(entry) = entries.next_entry().await? {
            let destination = target.join(entry.file_name());
            if entry.file_type().await?.is_dir() {
                pending.push((entry.path(), destination));
            } else {
                tokio::fs::copy(entry.path(), destination).await?;
            }
        }
    }
    Ok(())
}

async fn copy_game(paths: &Paths, from: String, to: PathBuf) -> io::Result<()> {
    match paths.game_kind(from.clone()).await? {
        Some("file") => {
            let bytes = paths.read_game(from).await?;
            tokio::fs::write(&to, bytes).await
        }
        Some(_) => {
            let vfs = paths.vfs.clone();
            let (directories, files) = blocking(move || {
                let mut directories = vec![String::new()];
                let mut files = Vec::new();
                let mut pending = vec![(from, String::new())];
                while let Some((directory, relative)) = pending.pop() {
                    for name in vfs.read_dir(&directory)? {
                        let child = vfs::join(&directory, &name);
                        let child_relative = vfs::join(&relative, &name);
                        if vfs.is_dir(&child) {
                            directories.push(child_relative.clone());
                            pending.push((child, child_relative));
                        } else if !is_script(&child) {
                            files.push((child_relative, vfs.read(&child)?));
                        }
                    }
                }
                Ok((directories, files))
            })
            .await?;
            for directory in directories {
                tokio::fs::create_dir_all(to.join(directory)).await?;
            }
            for (relative, bytes) in files {
                tokio::fs::write(to.join(relative), bytes).await?;
            }
            Ok(())
        }
        None => Err(io::Error::new(io::ErrorKind::NotFound, "no such file or directory")),
    }
}

fn with_path<A, R, F, Fut>(lua: &Lua, paths: &Paths, action: F) -> Result<Function>
where
    A: FromLuaMulti + 'static,
    R: IntoLuaMulti + 'static,
    F: Fn(Lua, Paths, Resolved, String, A) -> Fut + Clone + 'static,
    Fut: Future<Output = Result<R>> + 'static,
{
    let paths = paths.clone();
    lua.create_async_function(move |lua, (path, extra): (String, A)| {
        let paths = paths.clone();
        let action = action.clone();
        let location = paths.locate(&lua, &path);
        async move {
            let resolved = paths.resolve(location?).await?;
            action(lua, paths, resolved, path, extra).await
        }
    })
}

struct Defaults {
    input: AnyUserData,
    output: AnyUserData,
}

enum Choice {
    Keep,
    Open(String, Result<Location>),
    Use(AnyUserData),
    Invalid(String),
}

fn choose(lua: &Lua, paths: &Paths, value: Value, function: &str) -> Choice {
    match value {
        Value::Nil => Choice::Keep,
        Value::String(path) => match path.to_str() {
            Ok(path) => {
                let path = path.to_string();
                let location = paths.locate(lua, &path);
                Choice::Open(path, location)
            }
            Err(error) => Choice::Invalid(error.to_string()),
        },
        Value::UserData(file) if file.is::<File>() => Choice::Use(file),
        other => Choice::Invalid(format!(
            "bad argument #1 to '{function}' (file or path expected, got {})",
            other.type_name()
        )),
    }
}

fn default_stream(lua: &Lua, paths: &Paths, defaults: &Rc<RefCell<Defaults>>, output: bool) -> Result<Function> {
    let paths = paths.clone();
    let defaults = defaults.clone();
    let function = if output { "output" } else { "input" };
    lua.create_async_function(move |lua, value: Value| {
        let paths = paths.clone();
        let defaults = defaults.clone();
        let choice = choose(&lua, &paths, value, function);
        async move {
            let chosen = match choice {
                Choice::Keep => None,
                Choice::Use(file) => Some(file),
                Choice::Invalid(message) => return Err(mlua::Error::runtime(message)),
                Choice::Open(path, location) => {
                    let resolved = paths.resolve(location?).await?;
                    let mode = if output { Mode::WRITE } else { Mode::READ };
                    let file = open_file(&lua, &paths, resolved, &path, mode)
                        .await?
                        .map_err(|error| mlua::Error::runtime(format!("{path}: {error}")))?;
                    Some(file)
                }
            };
            let mut defaults = defaults.borrow_mut();
            if let Some(file) = chosen {
                if output {
                    defaults.output = file;
                } else {
                    defaults.input = file;
                }
            }
            Ok(if output { defaults.output.clone() } else { defaults.input.clone() })
        }
    })
}

fn temporary(lua: &Lua, engine: &Arc<Engine>, directory: bool) -> Result<Function> {
    let engine = engine.clone();
    lua.create_async_function(move |_, ()| {
        let engine = engine.clone();
        async move {
            let path = blocking(move || engine.temp_entry(directory))
                .await
                .map_err(|error| failure("create", if directory { "a temporary folder" } else { "a temporary file" }, error))?;
            Ok(path.to_string_lossy().into_owned())
        }
    })
}

pub fn create(lua: &Lua, engine: &Arc<Engine>) -> Result<Table> {
    let paths = Paths { vfs: engine.vfs().clone() };
    let fs = lua.create_table()?;

    let stdin = lua.create_userdata(File::new("stdin", Handle::Stdin(tokio::io::stdin()), true))?;
    let stdout = lua.create_userdata(File::new("stdout", Handle::Stdout(tokio::io::stdout()), true))?;
    let stderr = lua.create_userdata(File::new("stderr", Handle::Stderr(tokio::io::stderr()), true))?;
    let defaults = Rc::new(RefCell::new(Defaults {
        input: stdin.clone(),
        output: stdout.clone(),
    }));
    fs.set("stdin", stdin)?;
    fs.set("stdout", stdout)?;
    fs.set("stderr", stderr)?;

    fs.set("open", {
        let paths = paths.clone();
        lua.create_async_function(move |lua, (path, mode): (String, Option<String>)| {
            let paths = paths.clone();
            let location = paths.locate(&lua, &path);
            async move {
                let mode_text = mode.unwrap_or_else(|| "r".to_owned());
                let mode = Mode::parse(&mode_text).ok_or_else(|| {
                    mlua::Error::runtime(format!("bad argument #2 to 'open' (invalid mode '{mode_text}')"))
                })?;
                let resolved = paths.resolve(location?).await?;
                Ok(match open_file(&lua, &paths, resolved, &path, mode).await? {
                    Ok(file) => MultiValue::from_vec(vec![Value::UserData(file)]),
                    Err(error) => fail_path(&lua, &path, &error),
                })
            }
        })?
    })?;

    fs.set("lines", {
        let paths = paths.clone();
        let defaults = defaults.clone();
        lua.create_async_function(move |lua, arguments: MultiValue| {
            let paths = paths.clone();
            let defaults = defaults.clone();
            let mut arguments = arguments.into_iter();
            let first = arguments.next().unwrap_or(Value::Nil);
            let formats: MultiValue = arguments.collect();
            let choice = choose(&lua, &paths, first, "lines");
            async move {
                let formats = Format::parse(&formats, 2)?;
                match choice {
                    Choice::Keep => {
                        let input = defaults.borrow().input.clone();
                        File::lines(&lua, File::shared(&input)?, formats, false)
                    }
                    Choice::Use(file) => File::lines(&lua, File::shared(&file)?, formats, false),
                    Choice::Invalid(message) => Err(mlua::Error::runtime(message)),
                    Choice::Open(path, location) => {
                        let resolved = paths.resolve(location?).await?;
                        let file = open_file(&lua, &paths, resolved, &path, Mode::READ)
                            .await?
                            .map_err(|error| mlua::Error::runtime(format!("{path}: {error}")))?;
                        File::lines(&lua, File::shared(&file)?, formats, true)
                    }
                }
            }
        })?
    })?;

    fs.set("input", default_stream(lua, &paths, &defaults, false)?)?;
    fs.set("output", default_stream(lua, &paths, &defaults, true)?)?;

    fs.set("read", {
        let defaults = defaults.clone();
        lua.create_async_function(move |lua, formats: MultiValue| {
            let input = defaults.borrow().input.clone();
            async move { File::read(&lua, &input, formats).await }
        })?
    })?;

    fs.set("write", {
        let defaults = defaults.clone();
        lua.create_async_function(move |lua, values: MultiValue| {
            let output = defaults.borrow().output.clone();
            async move { File::write(&lua, &output, values).await }
        })?
    })?;

    fs.set("close", {
        let defaults = defaults.clone();
        lua.create_async_function(move |lua, file: Option<AnyUserData>| {
            let file = file.unwrap_or_else(|| defaults.borrow().output.clone());
            async move { File::close(&lua, &file).await }
        })?
    })?;

    fs.set(
        "type",
        lua.create_function(|_, value: Value| {
            Ok(match value {
                Value::UserData(file) => file
                    .borrow::<File>()
                    .ok()
                    .map(|file| if file.is_closed() { "closed file" } else { "file" }),
                _ => None,
            })
        })?,
    )?;

    fs.set(
        "readFile",
        with_path(lua, &paths, |lua, paths, resolved, path, ()| async move {
            let bytes = match resolved {
                Resolved::Game(game) => paths.read_game(game).await,
                Resolved::Disk(disk) => tokio::fs::read(disk).await,
            }
            .map_err(|error| failure("read", &path, error))?;
            lua.create_string(bytes)
        })?,
    )?;

    fs.set(
        "writeFile",
        with_path(lua, &paths, |_, _, resolved, path, data: Value| async move {
            let data = contents(data, "writeFile")?;
            match resolved {
                Resolved::Game(_) => Err(failure("write", &path, read_only())),
                Resolved::Disk(disk) => tokio::fs::write(disk, data).await.map_err(|error| failure("write", &path, error)),
            }
        })?,
    )?;

    fs.set(
        "appendFile",
        with_path(lua, &paths, |_, _, resolved, path, data: Value| async move {
            let data = contents(data, "appendFile")?;
            match resolved {
                Resolved::Game(_) => Err(failure("append to", &path, read_only())),
                Resolved::Disk(disk) => async {
                    let mut file = tokio::fs::OpenOptions::new().append(true).create(true).open(disk).await?;
                    file.write_all(&data).await?;
                    file.flush().await
                }
                .await
                .map_err(|error| failure("append to", &path, error)),
            }
        })?,
    )?;

    fs.set(
        "readDir",
        with_path(lua, &paths, |lua, paths, resolved, path, ()| async move {
            let names = match resolved {
                Resolved::Game(game) => {
                    let vfs = paths.vfs.clone();
                    blocking(move || vfs.read_dir(&game)).await
                }
                Resolved::Disk(disk) => async {
                    let mut names = Vec::new();
                    let mut entries = tokio::fs::read_dir(disk).await?;
                    while let Some(entry) = entries.next_entry().await? {
                        names.push(entry.file_name().to_string_lossy().into_owned());
                    }
                    names.sort();
                    Ok(names)
                }
                .await,
            }
            .map_err(|error| failure("read directory", &path, error))?;
            lua.create_sequence_from(names)
        })?,
    )?;

    fs.set(
        "makeDir",
        with_path(lua, &paths, |_, _, resolved, path, ()| async move {
            match resolved {
                Resolved::Game(_) => Err(failure("create directory", &path, read_only())),
                Resolved::Disk(disk) => tokio::fs::create_dir_all(disk)
                    .await
                    .map_err(|error| failure("create directory", &path, error)),
            }
        })?,
    )?;

    fs.set(
        "remove",
        with_path(lua, &paths, |_, _, resolved, path, ()| async move {
            match resolved {
                Resolved::Game(_) => Err(failure("remove", &path, read_only())),
                Resolved::Disk(disk) => async {
                    if tokio::fs::symlink_metadata(&disk).await?.is_dir() {
                        tokio::fs::remove_dir(&disk).await
                    } else {
                        tokio::fs::remove_file(&disk).await
                    }
                }
                .await
                .map_err(|error| failure("remove", &path, error)),
            }
        })?,
    )?;

    fs.set(
        "removeDir",
        with_path(lua, &paths, |_, _, resolved, path, ()| async move {
            match resolved {
                Resolved::Game(_) => Err(failure("remove", &path, read_only())),
                Resolved::Disk(disk) => tokio::fs::remove_dir_all(disk)
                    .await
                    .map_err(|error| failure("remove", &path, error)),
            }
        })?,
    )?;

    fs.set("rename", {
        let paths = paths.clone();
        lua.create_async_function(move |lua, (from, to): (String, String)| {
            let paths = paths.clone();
            let source = paths.locate(&lua, &from);
            let destination = paths.locate(&lua, &to);
            async move {
                match (paths.resolve(source?).await?, paths.resolve(destination?).await?) {
                    (Resolved::Disk(source), Resolved::Disk(destination)) => tokio::fs::rename(source, destination)
                        .await
                        .map_err(|error| failure("rename", &from, error)),
                    _ => Err(failure("rename", &from, read_only())),
                }
            }
        })?
    })?;

    fs.set("copy", {
        let paths = paths.clone();
        lua.create_async_function(move |lua, (from, to): (String, String)| {
            let paths = paths.clone();
            let source = paths.locate(&lua, &from);
            let destination = paths.locate(&lua, &to);
            async move {
                let Resolved::Disk(destination) = paths.resolve(destination?).await? else {
                    return Err(failure("copy to", &to, read_only()));
                };
                match paths.resolve(source?).await? {
                    Resolved::Game(source) => copy_game(&paths, source, destination).await,
                    Resolved::Disk(source) => copy_disk(source, destination).await,
                }
                .map_err(|error| failure("copy", &from, error))
            }
        })?
    })?;

    fs.set(
        "exists",
        with_path(lua, &paths, |_, paths, resolved, _, ()| async move {
            Ok(match resolved {
                Resolved::Game(game) => paths.game_kind(game).await.ok().flatten().is_some(),
                Resolved::Disk(disk) => tokio::fs::try_exists(disk).await.unwrap_or(false),
            })
        })?,
    )?;

    fs.set(
        "isFile",
        with_path(lua, &paths, |_, paths, resolved, _, ()| async move {
            Ok(match resolved {
                Resolved::Game(game) => paths.game_kind(game).await.ok().flatten() == Some("file"),
                Resolved::Disk(disk) => tokio::fs::metadata(disk).await.is_ok_and(|metadata| metadata.is_file()),
            })
        })?,
    )?;

    fs.set(
        "isDir",
        with_path(lua, &paths, |_, paths, resolved, _, ()| async move {
            Ok(match resolved {
                Resolved::Game(game) => paths.game_kind(game).await.ok().flatten() == Some("dir"),
                Resolved::Disk(disk) => tokio::fs::metadata(disk).await.is_ok_and(|metadata| metadata.is_dir()),
            })
        })?,
    )?;

    fs.set(
        "metadata",
        with_path(lua, &paths, |lua, paths, resolved, path, ()| async move {
            let table = lua.create_table()?;
            match resolved {
                Resolved::Game(game) => {
                    let vfs = paths.vfs.clone();
                    let (kind, size) = blocking(move || {
                        Ok(if vfs.is_file(&game) {
                            ("file", vfs.file_size(&game).unwrap_or(0))
                        } else if vfs.is_dir(&game) {
                            ("dir", 0)
                        } else {
                            return Err(io::Error::new(io::ErrorKind::NotFound, "no such file or directory"));
                        })
                    })
                    .await
                    .map_err(|error| failure("read metadata of", &path, error))?;
                    table.set("kind", kind)?;
                    table.set("size", size)?;
                    table.set("readonly", true)?;
                }
                Resolved::Disk(disk) => {
                    let metadata = tokio::fs::symlink_metadata(disk)
                        .await
                        .map_err(|error| failure("read metadata of", &path, error))?;
                    let kind = if metadata.file_type().is_symlink() {
                        "symlink"
                    } else if metadata.is_dir() {
                        "dir"
                    } else {
                        "file"
                    };
                    table.set("kind", kind)?;
                    table.set("size", metadata.len())?;
                    table.set("readonly", metadata.permissions().readonly())?;
                    table.set("modified", seconds(metadata.modified()))?;
                    table.set("created", seconds(metadata.created()))?;
                    table.set("accessed", seconds(metadata.accessed()))?;
                }
            }
            Ok(table)
        })?,
    )?;

    fs.set("tmpname", temporary(lua, engine, false)?)?;
    fs.set("tmpdir", temporary(lua, engine, true)?)?;

    fs.set_readonly(true);
    Ok(fs)
}
