use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use mlua::{AnyUserData, Function, Lua, Result, Table, Value};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::objects::{Child, ExitStatus};
use crate::runtime::{CloseCallbacks, Engine};

#[derive(Default)]
struct Options {
    cwd: Option<String>,
    env: Vec<(String, String)>,
    clear_env: bool,
    shell: bool,
    stdin: Option<Vec<u8>>,
    inherit: bool,
}

impl Options {
    fn from_table(table: Option<Table>) -> Result<Self> {
        let Some(table) = table else {
            return Ok(Self::default());
        };
        let inherit = match table.get::<Option<String>>("stdio")?.as_deref() {
            None | Some("capture") => false,
            Some("inherit") => true,
            Some(other) => {
                return Err(mlua::Error::runtime(format!(
                    "invalid stdio option '{other}', expected \"capture\" or \"inherit\""
                )));
            }
        };
        let env = match table.get::<Option<Table>>("env")? {
            Some(env) => env.pairs::<String, String>().collect::<Result<Vec<_>>>()?,
            None => Vec::new(),
        };
        Ok(Self {
            cwd: table.get("cwd")?,
            env,
            clear_env: table.get::<Option<bool>>("clearEnv")?.unwrap_or(false),
            shell: table.get::<Option<bool>>("shell")?.unwrap_or(false),
            stdin: table.get::<Option<mlua::LuaString>>("stdin")?.map(|stdin| stdin.as_bytes().to_vec()),
            inherit,
        })
    }

    fn command(&self, program: &str, args: &[String]) -> Command {
        let mut command = if self.shell {
            shell(program, args)
        } else {
            let mut command = Command::new(program);
            command.args(args);
            command
        };
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        if self.clear_env {
            command.env_clear();
        }
        command.envs(self.env.iter().map(|(key, value)| (key, value)));
        command
    }
}

fn command_line(program: &str, args: &[String]) -> String {
    let mut line = program.to_owned();
    for arg in args {
        line.push(' ');
        line.push_str(arg);
    }
    line
}

#[cfg(windows)]
fn shell(program: &str, args: &[String]) -> Command {
    let mut command = Command::new("cmd");
    command.raw_arg(format!("/d /s /c \"{}\"", command_line(program, args)));
    command
}

#[cfg(not(windows))]
fn shell(program: &str, args: &[String]) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(command_line(program, args));
    command
}

fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| if ch.is_control() || "<>:\"/\\|?*".contains(ch) { '_' } else { ch })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim();
    if cleaned.is_empty() { "Game".to_owned() } else { cleaned.to_owned() }
}

fn exit_code(value: Option<Value>) -> Result<i32> {
    match value {
        None | Some(Value::Nil) | Some(Value::Boolean(true)) => Ok(0),
        Some(Value::Boolean(false)) => Ok(1),
        Some(Value::Integer(code)) => Ok(code as i32),
        Some(Value::Number(code)) => Ok(code as i32),
        Some(other) => Err(mlua::Error::runtime(format!(
            "bad argument #1 to 'exit' (number or boolean expected, got {})",
            other.type_name()
        ))),
    }
}

pub fn create(lua: &Lua, engine: &Arc<Engine>, heartbeat: &AnyUserData) -> Result<Table> {
    let process = lua.create_table()?;
    process.set("Heartbeat", heartbeat.clone())?;

    let args = lua.create_sequence_from(engine.args().iter().cloned())?;
    args.set_readonly(true);
    process.set("args", args)?;

    let env = lua.create_table()?;
    for (key, value) in std::env::vars_os() {
        env.set(key.to_string_lossy().as_ref(), value.to_string_lossy().as_ref())?;
    }
    env.set_readonly(true);
    process.set("env", env)?;

    let dirs = lua.create_table()?;
    let folders = [
        ("home", dirs::home_dir()),
        ("appData", dirs::data_dir()),
        ("localAppData", dirs::data_local_dir()),
        ("config", dirs::config_dir()),
        ("cache", dirs::cache_dir()),
        ("temp", Some(std::env::temp_dir())),
        ("documents", dirs::document_dir()),
        ("desktop", dirs::desktop_dir()),
        ("downloads", dirs::download_dir()),
        ("pictures", dirs::picture_dir()),
        ("music", dirs::audio_dir()),
        ("videos", dirs::video_dir()),
        ("game", engine.game_dir().map(Path::to_path_buf)),
        ("save", dirs::data_dir().map(|directory| directory.join(folder_name(engine.game_name())))),
    ];
    for (key, folder) in folders {
        if let Some(folder) = folder {
            dirs.set(key, folder.to_string_lossy().as_ref())?;
        }
    }
    dirs.set_readonly(true);
    process.set("dirs", dirs)?;
    process.set(
        "executable",
        std::env::current_exe().ok().map(|path| path.to_string_lossy().into_owned()),
    )?;
    process.set("gameName", engine.game_name())?;
    process.set(
        "BindToClose",
        lua.create_function(|lua, callback: Function| {
            let callbacks = lua
                .app_data_ref::<CloseCallbacks>()
                .ok_or_else(|| mlua::Error::runtime("the luv engine is not running"))?;
            callbacks.push(callback);
            Ok(())
        })?,
    )?;

    process.set("os", std::env::consts::OS)?;
    process.set("arch", std::env::consts::ARCH)?;
    process.set("pid", std::process::id())?;

    process.set(
        "cwd",
        lua.create_function(|_, ()| {
            std::env::current_dir()
                .map(|directory| directory.to_string_lossy().into_owned())
                .map_err(|error| mlua::Error::runtime(format!("cannot read the working directory: {error}")))
        })?,
    )?;

    process.set("exit", {
        let engine = engine.clone();
        lua.create_async_function(move |_, code: Option<Value>| {
            let engine = engine.clone();
            async move {
                engine.request_exit(exit_code(code)?);
                std::future::pending::<()>().await;
                Ok(())
            }
        })?
    })?;

    process.set(
        "spawn",
        lua.create_async_function(
            |lua, (program, args, options): (String, Option<Vec<String>>, Option<Table>)| {
                let prepared = Options::from_table(options).map(|options| {
                    let mut command = options.command(&program, &args.unwrap_or_default());
                    command
                        .stdin(if options.stdin.is_some() {
                            Stdio::piped()
                        } else if options.inherit {
                            Stdio::inherit()
                        } else {
                            Stdio::null()
                        })
                        .stdout(if options.inherit { Stdio::inherit() } else { Stdio::piped() })
                        .stderr(if options.inherit { Stdio::inherit() } else { Stdio::piped() });
                    (command, options.stdin)
                });
                async move {
                    let (mut command, stdin) = prepared?;
                    let mut child = command
                        .spawn()
                        .map_err(|error| mlua::Error::runtime(format!("cannot start '{program}': {error}")))?;
                    let writer = match (child.stdin.take(), stdin) {
                        (Some(mut pipe), Some(data)) => Some(tokio::spawn(async move {
                            let _ = pipe.write_all(&data).await;
                        })),
                        _ => None,
                    };
                    let output = child
                        .wait_with_output()
                        .await
                        .map_err(|error| mlua::Error::runtime(format!("'{program}' failed: {error}")))?;
                    if let Some(writer) = writer {
                        let _ = writer.await;
                    }
                    let result = ExitStatus::from_std(Ok(output.status)).to_table(&lua)?;
                    result.set("stdout", lua.create_string(output.stdout)?)?;
                    result.set("stderr", lua.create_string(output.stderr)?)?;
                    Ok(result)
                }
            },
        )?,
    )?;

    process.set(
        "start",
        lua.create_function(
            |lua, (program, args, options): (String, Option<Vec<String>>, Option<Table>)| {
                let options = Options::from_table(options)?;
                let mut command = options.command(&program, &args.unwrap_or_default());
                let stdio = || if options.inherit { Stdio::inherit() } else { Stdio::piped() };
                command
                    .stdin(if options.inherit && options.stdin.is_none() {
                        Stdio::inherit()
                    } else {
                        Stdio::piped()
                    })
                    .stdout(stdio())
                    .stderr(stdio());
                let mut child = command
                    .spawn()
                    .map_err(|error| mlua::Error::runtime(format!("cannot start '{program}': {error}")))?;
                if let Some(data) = options.stdin
                    && let Some(mut pipe) = child.stdin.take()
                {
                    tokio::spawn(async move {
                        let _ = pipe.write_all(&data).await;
                    });
                }
                Child::spawn(lua, &program, child)
            },
        )?,
    )?;

    process.set_readonly(true);
    Ok(process)
}
