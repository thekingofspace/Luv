use std::sync::Arc;

use mlua::{AnyUserData, Function, Lua, Result, Table, Value};
use wgpu::naga::ShaderStage;

use crate::graphics::PRELUDE;
use crate::graphics::combine::{self, Combined, Part};
use crate::graphics::shader::{self, Language, ShaderSource, parse_stage};
use crate::objects::file::Format;
use crate::objects::{Asset, File, GameObject, Shader, ShaderCombo};
use crate::runtime::Scheduler;

enum Body {
    Bytes(Vec<u8>),
    File(AnyUserData),
    Combined(Arc<Combined>),
}

struct Request {
    name: String,
    language: Language,
    stage: Option<ShaderStage>,
    body: Body,
}

fn stage(name: &str) -> Result<ShaderStage> {
    parse_stage(name).ok_or_else(|| {
        mlua::Error::runtime(format!(
            "unknown shader stage '{name}', expected \"vertex\", \"fragment\" or \"compute\""
        ))
    })
}

fn language(name: &str) -> Result<Language> {
    Language::parse(name).ok_or_else(|| {
        mlua::Error::runtime(format!(
            "unknown shader language '{name}', expected \"wgsl\", \"glsl\" or \"spirv\""
        ))
    })
}

fn request(value: Value) -> Result<Request> {
    let from_path = |name: &str, fallback: Language| {
        Language::from_path(name).map_or((fallback, None), |(language, stage)| (language, stage))
    };
    match value {
        Value::String(code) => Ok(Request {
            name: "shader".to_owned(),
            language: Language::Wgsl,
            stage: None,
            body: Body::Bytes(code.as_bytes().to_vec()),
        }),
        Value::Buffer(buffer) => Ok(Request {
            name: "shader".to_owned(),
            language: Language::Spirv,
            stage: None,
            body: Body::Bytes(buffer.to_vec()),
        }),
        Value::UserData(userdata) if userdata.is::<Asset>() => {
            let asset = userdata.borrow::<Asset>()?;
            let (language, stage) = from_path(asset.path(), Language::Wgsl);
            Ok(Request {
                name: asset.path().to_owned(),
                language,
                stage,
                body: Body::Bytes(asset.data()?.to_vec()),
            })
        }
        Value::UserData(userdata) if userdata.is::<File>() => {
            let name = userdata.borrow::<File>()?.base().name().to_owned();
            let (language, stage) = from_path(&name, Language::Wgsl);
            Ok(Request {
                name,
                language,
                stage,
                body: Body::File(userdata),
            })
        }
        Value::UserData(userdata) if userdata.is::<ShaderCombo>() => {
            let combined = userdata.borrow::<ShaderCombo>()?.combined()?;
            Ok(Request {
                name: combined.name.clone(),
                language: combined.language,
                stage: combined.stage,
                body: Body::Combined(combined),
            })
        }
        Value::Table(options) => {
            let source: Value = options.get("Source")?;
            if source.is_nil() {
                return Err(mlua::Error::runtime("shader options need a Source"));
            }
            let mut request = request(source)?;
            if let Some(name) = options.get::<Option<String>>("Name")? {
                request.name = name;
            }
            if let Some(name) = options.get::<Option<String>>("Language")? {
                request.language = language(&name)?;
            }
            if let Some(name) = options.get::<Option<String>>("Stage")? {
                request.stage = Some(stage(&name)?);
            }
            Ok(request)
        }
        other => Err(mlua::Error::runtime(format!(
            "bad shader source (expected string, buffer, Asset, File, ShaderCombo or options table, got {})",
            other.type_name()
        ))),
    }
}

fn requests(value: Value) -> Result<(Vec<Request>, bool)> {
    match value {
        Value::Table(table) if table.raw_get::<Value>("Source")?.is_nil() => {
            let sources = table.sequence_values::<Value>().collect::<Result<Vec<_>>>()?;
            Ok((sources.into_iter().map(request).collect::<Result<_>>()?, false))
        }
        other => Ok((vec![request(other)?], true)),
    }
}

async fn read(lua: &Lua, body: Body) -> std::result::Result<Vec<u8>, String> {
    match body {
        Body::Bytes(bytes) => Ok(bytes),
        Body::Combined(combined) => Ok(combined.code.as_bytes().to_vec()),
        Body::File(file) => {
            let shared = File::shared(&file).map_err(|error| error.to_string())?;
            let values = shared
                .read_formats(lua, &[Format::All])
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
            match values.into_iter().next() {
                Some(Value::String(text)) => Ok(text.as_bytes().to_vec()),
                _ => Ok(Vec::new()),
            }
        }
    }
}

async fn build(lua: Lua, request: Request) -> Result<AnyUserData> {
    let Request {
        name,
        language,
        stage,
        body,
    } = request;
    let segments = match &body {
        Body::Combined(combined) => Some(Arc::from(combined.segments.clone())),
        _ => None,
    };
    let result = match read(&lua, body).await {
        Ok(code) => {
            let source = ShaderSource {
                name: name.clone(),
                language,
                stage,
                code,
                segments,
            };
            tokio::task::spawn_blocking(move || shader::compile(&source))
                .await
                .map_err(mlua::Error::external)?
        }
        Err(error) => Err(format!("{name}: {error}")),
    };
    lua.create_userdata(Shader::new(name, language, result))
}

fn merge(current: &mut Option<Language>, next: Language, name: &str) -> Result<()> {
    match current {
        Some(existing) if *existing != next => Err(mlua::Error::runtime(format!(
            "'{name}' is {} but the combo is {}, every part of a combo must use the same language",
            next.name(),
            existing.name()
        ))),
        _ => {
            *current = Some(next);
            Ok(())
        }
    }
}

async fn combo(lua: Lua, parts: Table, options: Option<Table>) -> Result<AnyUserData> {
    let mut name = "combo".to_owned();
    let mut target: Option<Language> = None;
    let mut entry: Option<ShaderStage> = None;
    if let Some(options) = &options {
        if let Some(value) = options.get::<Option<String>>("Name")? {
            name = value;
        }
        if let Some(value) = options.get::<Option<String>>("Language")? {
            target = Some(language(&value)?);
        }
        if let Some(value) = options.get::<Option<String>>("Stage")? {
            entry = Some(stage(&value)?);
        }
    }

    let mut collected = Vec::new();
    for (index, value) in parts.sequence_values::<Value>().enumerate() {
        match value? {
            Value::String(code) => {
                let code = code
                    .to_str()
                    .map_err(|_| mlua::Error::runtime(format!("part {} is not valid UTF-8 text", index + 1)))?
                    .to_string();
                let label = if code == PRELUDE {
                    "prelude".to_owned()
                } else {
                    format!("part {}", index + 1)
                };
                collected.push(Part { name: label, code });
            }
            Value::UserData(userdata) if userdata.is::<ShaderCombo>() => {
                let combined = userdata.borrow::<ShaderCombo>()?.combined()?;
                merge(&mut target, combined.language, &combined.name)?;
                entry = entry.or(combined.stage);
                collected.extend(combined.parts.iter().cloned());
            }
            other => {
                let request = request(other)?;
                if request.language == Language::Spirv {
                    return Err(mlua::Error::runtime(format!(
                        "'{}' is SPIR-V, which cannot be combined, combine WGSL or GLSL sources instead",
                        request.name
                    )));
                }
                merge(&mut target, request.language, &request.name)?;
                entry = entry.or(request.stage);
                let label = request.name.clone();
                let code = read(&lua, request.body).await.map_err(mlua::Error::runtime)?;
                let code = String::from_utf8(code)
                    .map_err(|_| mlua::Error::runtime(format!("'{label}' is not valid UTF-8 text")))?;
                collected.push(Part { name: label, code });
            }
        }
    }
    let combined = combine::combine(name, target.unwrap_or(Language::Wgsl), entry, collected)
        .map_err(mlua::Error::runtime)?;
    lua.create_userdata(ShaderCombo::new(combined))
}

pub fn create(lua: &Lua) -> Result<Table> {
    let library = lua.create_table()?;

    library.set(
        "Compile",
        lua.create_async_function(|lua, (input, callback): (Value, Option<Function>)| {
            let parsed = requests(input);
            async move {
                let (requests, single) = parsed?;
                if let Some(callback) = callback {
                    let scheduler = Scheduler::get(&lua)?;
                    for (index, request) in requests.into_iter().enumerate() {
                        let lua = lua.clone();
                        let callback = callback.clone();
                        let spawner = scheduler.clone();
                        scheduler.spawn_task(async move {
                            match build(lua.clone(), request).await {
                                Ok(shader) => spawner.spawn(&lua, callback, (shader, index + 1)),
                                Err(error) => spawner.report(error),
                            }
                        });
                    }
                    return Ok(Value::Nil);
                }

                let tasks: Vec<_> = requests
                    .into_iter()
                    .map(|request| tokio::task::spawn_local(build(lua.clone(), request)))
                    .collect();
                let mut shaders = Vec::with_capacity(tasks.len());
                for task in tasks {
                    shaders.push(task.await.map_err(mlua::Error::external)??);
                }
                if single {
                    Ok(Value::UserData(shaders.remove(0)))
                } else {
                    Ok(Value::Table(lua.create_sequence_from(shaders)?))
                }
            }
        })?,
    )?;

    library.set(
        "Combine",
        lua.create_async_function(|lua, (parts, options): (Table, Option<Table>)| combo(lua, parts, options))?,
    )?;

    library.set("Prelude", PRELUDE)?;
    library.set_readonly(true);
    Ok(library)
}
