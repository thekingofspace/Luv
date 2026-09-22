#![allow(dead_code)]

pub mod font;
pub mod native;

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use luv::builder;
use luv::project::{GameInfo, Project};
use luv::runtime::{Engine, EngineBuilder, Runtime};
use luv::vfs::{Pak, Vfs};
use mlua::FromLua;
use tempfile::TempDir;

pub fn write(root: &Path, path: &str, contents: &str) {
    let target = root.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, contents).unwrap();
}

pub fn workspace(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "build.toml", "[game]\nname = \"Fixture\"\n");
    for (path, contents) in files {
        write(dir.path(), path, contents);
    }
    dir
}

pub fn main_script(source: &str) -> TempDir {
    workspace(&[("src/main.luau", source)])
}

pub struct Outcome {
    pub runtime: Runtime,
    pub errors: Vec<String>,
}

impl Outcome {
    pub fn global<T: FromLua>(&self, name: &str) -> T {
        self.runtime.lua().globals().get(name).unwrap()
    }

    pub fn assert_clean(&self) {
        assert!(self.errors.is_empty(), "unexpected errors: {:#?}", self.errors);
    }
}

pub async fn run(vfs: Arc<dyn Vfs>, entry: &str) -> Outcome {
    run_with(vfs, entry, |builder| builder).await
}

pub async fn run_with(
    vfs: Arc<dyn Vfs>,
    entry: &str,
    customize: impl FnOnce(EngineBuilder) -> EngineBuilder,
) -> Outcome {
    let errors = Arc::new(Mutex::new(Vec::new()));
    let sink = errors.clone();
    let builder = Engine::builder(vfs).reporter(move |message| sink.lock().unwrap().push(message.to_owned()));
    let engine = customize(builder)
        .setup(|lua| {
            let sleep = lua.create_async_function(|_, ms: u64| async move {
                tokio::time::sleep(Duration::from_millis(ms)).await;
                Ok(())
            })?;
            lua.globals().set("sleep", sleep)?;
            let thread_name = lua.create_function(|_, ()| {
                Ok(std::thread::current().name().unwrap_or("unnamed").to_owned())
            })?;
            lua.globals().set("threadName", thread_name)
        })
        .build();
    let runtime = Runtime::new(engine).unwrap();
    tokio::time::timeout(Duration::from_secs(30), runtime.run(entry))
        .await
        .expect("the game did not finish");
    let errors = errors.lock().unwrap().clone();
    Outcome { runtime, errors }
}

pub async fn run_source(root: &Path) -> Outcome {
    let project = Project::load(root).unwrap();
    run(Arc::new(project.source_vfs()), &project.entry().unwrap()).await
}

pub async fn build(root: &Path) -> Pak {
    let project = Project::load(root).unwrap();
    let report = builder::build(&project).await.unwrap();
    Pak::open(&report.package).unwrap()
}

pub async fn run_package(root: &Path) -> Outcome {
    let pak = build(root).await;
    let game = GameInfo::from_manifest(pak.manifest()).unwrap();
    run(Arc::new(pak), &game.main).await
}

pub async fn run_both(root: &Path) -> [Outcome; 2] {
    [run_source(root).await, run_package(root).await]
}

pub async fn run_both_with(
    root: &Path,
    customize: impl Fn(EngineBuilder) -> EngineBuilder,
) -> [Outcome; 2] {
    let project = Project::load(root).unwrap();
    let source = run_with(Arc::new(project.source_vfs()), &project.entry().unwrap(), &customize).await;
    let pak = build(root).await;
    let game = GameInfo::from_manifest(pak.manifest()).unwrap();
    let package = run_with(Arc::new(pak), &game.main, &customize).await;
    [source, package]
}

pub fn global_string(name: &'static str, value: String) -> impl Fn(EngineBuilder) -> EngineBuilder {
    move |builder| {
        let value = value.clone();
        builder.setup(move |lua| lua.globals().set(name, value.as_str()))
    }
}
