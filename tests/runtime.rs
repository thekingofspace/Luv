mod common;

use std::fs;

use common::{build, main_script, run_both, run_package, run_source, workspace, write};
use luv::builder;
use luv::project::{self, GameInfo, Project};
use luv::vfs::EntryKind;
use mlua::Table;
use tempfile::TempDir;

fn fixture() -> TempDir {
    workspace(&[
        ("build.toml", "[game]\nname = \"Fixture\"\nmain = \"./src/main.luau\"\n"),
        (".luaurc", r#"{ "aliases": { "lib": "./lib" } }"#),
        (".vscode/settings.json", "{}"),
        ("types.d.luau", "declare function sleep(ms: number): ()\n"),
        ("assets/logo.txt", "luv"),
        ("lib/answer.luau", "return 42\n"),
        ("src/util.luau", "return { name = \"util\" }\n"),
        (
            "src/shared/init.luau",
            "local helper = require(\"@self/helper\")\nreturn { name = \"shared:\" .. helper }\n",
        ),
        ("src/shared/helper.luau", "return \"helper\"\n"),
        ("src/shared/deep.luau", "return require(\"../util\").name .. \"+deep\"\n"),
        ("src/waits.luau", "sleep(5)\nreturn \"waited\"\n"),
        (
            "src/main.luau",
            r#"
local util = require("./util")
local shared = require("./shared")
result = {
    util = util.name,
    shared = shared.name,
    deep = require("./shared/deep"),
    aliased = require("@lib/answer"),
    waited = require("./waits"),
    cached = require("./util") == util,
}
"#,
        ),
    ])
}

#[tokio::test]
async fn runs_the_workspace_from_source_and_from_a_package() {
    let dir = fixture();
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let result: Table = outcome.global("result");
        assert_eq!(result.get::<String>("util").unwrap(), "util");
        assert_eq!(result.get::<String>("shared").unwrap(), "shared:helper");
        assert_eq!(result.get::<String>("deep").unwrap(), "util+deep");
        assert_eq!(result.get::<i64>("aliased").unwrap(), 42);
        assert_eq!(result.get::<String>("waited").unwrap(), "waited");
        assert!(result.get::<bool>("cached").unwrap());
    }
}

#[tokio::test]
async fn package_manifest_records_the_game() {
    let dir = fixture();
    let pak = build(dir.path()).await;
    let game = GameInfo::from_manifest(pak.manifest()).unwrap();
    assert_eq!(game.name, "Fixture");
    assert_eq!(game.main, "src/main.luau");
}

#[tokio::test]
async fn packages_workspace_files_with_their_paths() {
    let dir = fixture();
    let pak = build(dir.path()).await;

    let mut paths: Vec<_> = pak.entries().map(|(path, entry)| (path.to_owned(), entry.kind)).collect();
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        paths,
        [
            (".luaurc", EntryKind::Asset),
            ("assets/logo.txt", EntryKind::Asset),
            ("lib/answer.luau", EntryKind::Bytecode),
            ("src/main.luau", EntryKind::Bytecode),
            ("src/shared/deep.luau", EntryKind::Bytecode),
            ("src/shared/helper.luau", EntryKind::Bytecode),
            ("src/shared/init.luau", EntryKind::Bytecode),
            ("src/util.luau", EntryKind::Bytecode),
            ("src/waits.luau", EntryKind::Bytecode),
        ]
        .map(|(path, kind)| (path.to_owned(), kind))
    );
    assert_eq!(pak.read("assets/logo.txt").unwrap(), b"luv");
    assert_ne!(pak.read("src/util.luau").unwrap(), b"return { name = \"util\" }\n");
}

#[tokio::test]
async fn packages_use_the_luvit_extension() {
    let dir = fixture();
    let project = Project::load(dir.path()).unwrap();
    let report = builder::build(&project).await.unwrap();
    assert_eq!(report.package, dir.path().join("build").join("Fixture.luvit"));
}

#[tokio::test]
async fn rebuilding_does_not_package_previous_builds() {
    let dir = fixture();
    build(dir.path()).await;
    let pak = build(dir.path()).await;
    assert!(pak.entries().all(|(path, _)| !path.starts_with("build/")));
}

#[tokio::test]
async fn reports_every_compile_error_with_its_path() {
    let dir = fixture();
    write(dir.path(), "src/broken.luau", "local x = \n");
    write(dir.path(), "src/also_broken.luau", "return }\n");
    let project = Project::load(dir.path()).unwrap();

    let message = format!("{:#}", builder::build(&project).await.err().unwrap());
    assert!(message.contains("src/broken.luau:2:"), "{message}");
    assert!(message.contains("src/also_broken.luau:1:"), "{message}");
    assert!(!dir.path().join("build").exists());
}

#[tokio::test]
async fn runtime_errors_point_at_the_script() {
    let dir = main_script("local x = 1\nerror(\"boom\")\n");
    for outcome in run_both(dir.path()).await {
        assert_eq!(outcome.errors.len(), 1, "{:#?}", outcome.errors);
        assert!(outcome.errors[0].contains("src/main.luau:2: boom"), "{}", outcome.errors[0]);
    }
}

#[tokio::test]
async fn missing_scripts_are_reported() {
    let dir = main_script("return nil\n");
    let project = Project::load(dir.path()).unwrap();
    let outcome = common::run(
        std::sync::Arc::new(luv::vfs::DirVfs::new(&project.root)),
        "src/missing.luau",
    )
    .await;
    assert_eq!(outcome.errors.len(), 1, "{:#?}", outcome.errors);
}

#[tokio::test]
async fn init_creates_a_workspace_that_builds_and_runs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("My Game");

    let report = project::init(&root, None).unwrap();
    assert_eq!(
        report.created,
        [
            "types.d.luau",
            "native/luv.h",
            "native/luv.rs",
            ".vscode/settings.json",
            "build.toml",
            "src/main.luau",
            ".gitignore"
        ]
    );
    assert!(root.join("assets").is_dir());
    assert!(!report.existing);

    let settings = fs::read_to_string(root.join(".vscode/settings.json")).unwrap();
    assert!(settings.contains("\"luv\": \"./types.d.luau\""));
    assert!(settings.contains("\"luau-lsp.require.mode\": \"relativeToFile\""));

    let types = fs::read_to_string(root.join("types.d.luau")).unwrap();
    assert!(types.contains("declare import: <K>(name: keyof<Imports> & K) -> index<Imports, K>"));
    assert!(types.contains("declare function EnterParallel(): ()"));
    let header = fs::read_to_string(root.join("native/luv.h")).unwrap();
    assert!(header.contains("struct LuvRenderContext"));
    let bindings = fs::read_to_string(root.join("native/luv.rs")).unwrap();
    assert!(bindings.contains("pub struct LuvRenderContext"));

    let project = Project::load(&root).unwrap();
    assert_eq!(project.manifest.game.name, "My Game");
    assert_eq!(project.manifest.game.icon.as_deref(), Some("assets/icon.png"));
    assert_eq!(project.entry().unwrap(), "src/main.luau");

    let report = builder::build(&project).await.unwrap();
    assert_eq!(report.package, root.join("build").join("My-Game.luvit"));
    assert_eq!((report.scripts, report.assets), (1, 0));

    run_package(&root).await.assert_clean();
    run_source(&root).await.assert_clean();
}

#[test]
fn init_refreshes_outdated_types_in_existing_workspaces() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    project::init(root, None).unwrap();

    let again = project::init(root, None).unwrap();
    assert!(again.existing);
    assert!(again.created.is_empty() && again.updated.is_empty());
    assert_eq!(again.skipped, ["types.d.luau", "native/luv.h", "native/luv.rs", ".vscode/settings.json"]);

    let current = fs::read_to_string(root.join("types.d.luau")).unwrap();
    fs::write(root.join("types.d.luau"), "export type Old = number\n").unwrap();
    fs::write(root.join("src/main.luau"), "print(\"mine\")\n").unwrap();
    fs::remove_file(root.join(".vscode/settings.json")).unwrap();
    fs::remove_file(root.join(".gitignore")).unwrap();

    let refreshed = project::init(root, Some("Ignored".to_owned())).unwrap();
    assert_eq!(refreshed.updated, ["types.d.luau"]);
    assert_eq!(refreshed.created, [".vscode/settings.json"]);
    assert_eq!(fs::read_to_string(root.join("types.d.luau")).unwrap(), current);
    assert_eq!(fs::read_to_string(root.join("src/main.luau")).unwrap(), "print(\"mine\")\n");
    assert!(!root.join(".gitignore").exists());
    assert!(!fs::read_to_string(root.join("build.toml")).unwrap().contains("Ignored"));
}

#[test]
fn init_escapes_the_game_name() {
    let dir = tempfile::tempdir().unwrap();
    project::init(dir.path(), Some("Say \"Hi\" \\ bye".to_owned())).unwrap();
    let project = Project::load(dir.path()).unwrap();
    assert_eq!(project.manifest.game.name, "Say \"Hi\" \\ bye");
}

#[test]
fn discovers_the_workspace_from_a_subdirectory() {
    let dir = fixture();
    let project = Project::discover(&dir.path().join("src").join("shared")).unwrap();
    assert_eq!(project.root, dir.path());
}

#[test]
fn init_fuses_plugin_type_files_into_the_game_types() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    project::init(root, None).unwrap();

    write(
        root,
        "native/physics.d.luau",
        r#"export type Body = {
	Mass: number,
	Push: (self: Body, force: UDim) -> (),
}

export type Physics_API = {
	Gravity: number,
	AddBody: (mass: number) -> Body,
}

export type Imports = {
	Physics: Physics_API,
}

export type WindowAPIs = {
	Physics: Physics_API,
}
"#,
    );
    write(
        root,
        "packs/tools/container.toml",
        "[container]\nname = \"Tools\"\nmain = \"./init.luau\"\n",
    );
    write(root, "packs/tools/init.luau", "return {}\n");
    write(
        root,
        "packs/tools/native/tools.d.luau",
        "export type Tools_API = {\n\tOpen: () -> boolean,\n}\n\nexport type Imports = {\n\tTools: Tools_API,\n}\n",
    );

    let report = project::init(root, None).unwrap();
    assert_eq!(report.updated, ["types.d.luau"]);

    let types = fs::read_to_string(root.join("types.d.luau")).unwrap();
    assert!(types.contains("\t-- from native/physics.d.luau\n\tPhysics: Physics_API,\n"));
    assert!(types.contains("\t-- from packs/tools/native/tools.d.luau\n\tTools: Tools_API,\n"));
    assert!(types.contains("-- luv plugin types from native/physics.d.luau"));
    assert!(types.contains("-- end of packs/tools/native/tools.d.luau"));
    assert!(types.contains("export type Physics_API = {"));
    assert!(types.contains("export type Tools_API = {"));
    assert!(types.contains("declare import: <K>(name: keyof<Imports> & K) -> index<Imports, K>"));
    assert_eq!(types.matches("export type Imports = {").count(), 1);
    assert_eq!(types.matches("export type WindowAPIs = {").count(), 1);
    assert!(types.contains("\tWindow: Window_API,\n\t-- from native/physics.d.luau"));
    assert!(types.contains("\tSound: Sound_API,\n\t-- from native/physics.d.luau"));

    let project = Project::load(root).unwrap();
    assert!(!luv::typegen::sync(&project).unwrap().changed);

    fs::remove_file(root.join("native/physics.d.luau")).unwrap();
    let report = luv::typegen::sync(&project).unwrap();
    assert!(report.changed);
    assert_eq!(report.sources, ["packs/tools/native/tools.d.luau"]);
    let types = fs::read_to_string(root.join("types.d.luau")).unwrap();
    assert!(!types.contains("Physics_API"));
    assert!(types.contains("Tools_API"));
}

#[test]
fn a_workspace_without_plugin_types_gets_the_plain_engine_types() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    project::init(root, None).unwrap();
    let types = fs::read_to_string(root.join("types.d.luau")).unwrap();
    assert_eq!(types, project::TYPES_TEMPLATE);
}
