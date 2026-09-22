mod common;

use std::path::Path;
use std::sync::Arc;

use common::{Outcome, run_with, workspace};
use luv::builder;
use luv::plugins;
use luv::project::{ContainerInfo, Project};
use luv::runtime::AssetCache;
use luv::vfs::{EntryKind, Pak};
use mlua::Table;

const MANIFEST: &str = r#"[container]
name = "Expansion"
version = "1.2.0"
description = "More levels"
main = "src/expansion/init.luau"
"#;

const ENTRY: &str = r#"
local Asset = import("Asset")
local DLL = import("DLL")
local helper = require("@self/helper")
local shared = require("./shared")
return {
    greet = function()
        return helper.word .. " and " .. shared.word
    end,
    readAsset = function()
        return Asset.LoadString("expansion/levels.txt")
    end,
    bonus = function()
        return DLL.Load("./bonus"):GetFunction("bonus_value", "int")()
    end,
}
"#;

const BONUS: &str = r#"
#if defined(_WIN32)
__declspec(dllexport)
#endif
int bonus_value(void) {
    return 42;
}
"#;

const GAME: &str = r#"
local Container = import("Container")
local Asset = import("Asset")
results = {}
results.found = table.concat(Container.GetContainers(), ",")
results.exists = Container.Exists("Expansion")
results.existsLower = Container.Exists("expansion")
results.missing = Container.Exists("Nope")
results.loadedBefore = Container.IsLoaded("Expansion")
results.libraryBefore = Container.GetLibrary("Expansion") == nil
results.assetBefore = pcall(Asset.LoadString, "expansion/levels.txt")
results.requireBefore = pcall(function()
    return require("@Expansion")
end)
results.hiddenSource = pcall(function()
    return require("../expansion/src/expansion")
end)

local library = Container.LoadLibrary("Expansion")
results.className = library.ClassName
results.name = library.Name
results.id = library.Id
results.version = library.Version
results.description = library.Description
results.main = library.Main
results.path = library.Path
results.natives = #library.Natives
results.require = library:GetRequire()
local expansion = require(library:GetRequire())
results.greeting = expansion.greet()
results.fromContainer = expansion.readAsset()
results.merged = Asset.LoadString("expansion/levels.txt")
results.base = Asset.LoadString("base.txt")
results.bonus = expansion.bonus()
results.loadedAfter = Container.IsLoaded("Expansion")
results.library = Container.GetLibrary("expansion").Id
results.sameModule = rawequal(require("@expansion"), expansion)
results.again = Container.LoadLibrary("Expansion").Id
results.badLoad = tostring(select(2, pcall(Container.LoadLibrary, "Nope")))

local first = Asset.Load("base.txt")
local second = Asset.Load("base.txt")
first:Destroy()
results.survivor = second:ReadString()
results.destroyed = not pcall(function()
    return first:ReadString()
end)
"#;

fn expansion_workspace() -> tempfile::TempDir {
    workspace(&[
        ("src/main.luau", GAME),
        ("src/shared.luau", "return { word = \"shared\" }\n"),
        ("assets/base.txt", "base"),
        ("expansion/container.toml", MANIFEST),
        ("expansion/src/expansion/init.luau", ENTRY),
        ("expansion/src/expansion/helper.luau", "return { word = \"helper\" }\n"),
        ("expansion/assets/expansion/levels.txt", "level data"),
        ("expansion/native/bonus.c", BONUS),
    ])
}

async fn run_game(root: &Path) -> Outcome {
    let project = Project::load(root).unwrap();
    let natives = plugins::build(&project).unwrap();
    builder::build_containers(&project, &natives).await.unwrap();
    let output = project.output_dir();
    let directory = root.to_path_buf();
    run_with(Arc::new(project.source_vfs()), "src/main.luau", move |builder| {
        builder
            .game("Fixture", directory.clone())
            .library_dirs([output.clone()])
            .container_dirs([output.clone()])
    })
    .await
}

#[tokio::test]
async fn containers_stay_unloaded_until_requested_then_merge_with_the_game() {
    let dir = expansion_workspace();
    let outcome = run_game(dir.path()).await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap_or_else(|error| panic!("{key}: {error}"));
    let flag = |key: &str| results.get::<bool>(key).unwrap_or_else(|error| panic!("{key}: {error}"));
    assert_eq!(text("found"), "Expansion");
    for key in ["exists", "existsLower", "libraryBefore", "loadedAfter", "sameModule", "destroyed"] {
        assert!(flag(key), "{key}");
    }
    for key in ["missing", "loadedBefore", "assetBefore", "requireBefore", "hiddenSource"] {
        assert!(!flag(key), "{key}");
    }
    assert_eq!(text("className"), "ContainerLibrary");
    assert_eq!(text("name"), "Expansion");
    assert_eq!(text("id"), "Expansion");
    assert_eq!(text("version"), "1.2.0");
    assert_eq!(text("description"), "More levels");
    assert_eq!(text("main"), "src/expansion/init.luau");
    assert!(text("path").ends_with("Expansion.cont"));
    assert_eq!(results.get::<i64>("natives").unwrap(), 1);
    assert_eq!(text("require"), "@Expansion");
    assert_eq!(text("greeting"), "helper and shared");
    assert_eq!(text("fromContainer"), "level data");
    assert_eq!(text("merged"), "level data");
    assert_eq!(text("base"), "base");
    assert_eq!(results.get::<i64>("bonus").unwrap(), 42);
    assert_eq!(text("library"), "Expansion");
    assert_eq!(text("again"), "Expansion");
    assert!(text("badLoad").contains("no container named Nope was found next to the game"));
    assert_eq!(text("survivor"), "base");
}

#[tokio::test]
async fn containers_pack_all_their_code_and_assets_as_bytecode() {
    let dir = expansion_workspace();
    let project = Project::load(dir.path()).unwrap();
    let natives = plugins::build(&project).unwrap();
    let reports = builder::build_containers(&project, &natives).await.unwrap();
    assert_eq!(reports.len(), 1);
    assert!(reports[0].rebuilt);
    let container = Pak::open(&reports[0].path).unwrap();
    let info = ContainerInfo::from_manifest(container.manifest()).unwrap();
    assert_eq!(info.name, "Expansion");
    assert_eq!(info.main, "src/expansion/init.luau");
    assert_eq!(info.natives.len(), 1);
    assert!(info.natives[0].contains("bonus"));
    let kind = |path: &str| container.entry(path).map(|entry| entry.kind);
    assert_eq!(kind("src/expansion/init.luau"), Some(EntryKind::Bytecode));
    assert_eq!(kind("src/expansion/helper.luau"), Some(EntryKind::Bytecode));
    assert_eq!(kind("assets/expansion/levels.txt"), Some(EntryKind::Asset));
    assert_eq!(kind("container.toml"), None);
    assert!(container.entries().all(|(path, _)| !path.starts_with("native")));

    let again = builder::build_containers(&project, &natives).await.unwrap();
    assert!(!again[0].rebuilt);

    let game = builder::build(&project).await.unwrap();
    let packed = Pak::open(&game.package).unwrap();
    assert!(packed.entries().all(|(path, _)| !path.starts_with("expansion")));
    assert!(packed.entry("src/main.luau").is_some());
}

#[tokio::test]
async fn containers_cannot_reuse_paths_from_the_game() {
    let dir = workspace(&[
        ("src/main.luau", "print('game')\n"),
        ("assets/base.txt", "base"),
        (
            "clash/container.toml",
            "[container]\nname = \"Clash\"\nmain = \"src/main.luau\"\n",
        ),
        ("clash/src/main.luau", "return {}\n"),
        ("clash/assets/base.txt", "other"),
    ]);
    let project = Project::load(dir.path()).unwrap();
    let error = match builder::build_containers(&project, &[]).await {
        Ok(_) => panic!("the clashing container built"),
        Err(error) => format!("{error:#}"),
    };
    assert!(error.contains("src/main.luau is in both the game and container Clash"), "{error}");
    assert!(error.contains("assets/base.txt is in both the game and container Clash"), "{error}");
}

#[test]
fn assets_share_memory_until_the_last_user_drops_them() {
    let cache = AssetCache::default();
    let first: Arc<[u8]> = Arc::from(vec![1u8, 2, 3]);
    let shared = cache.share("assets/a.bin", first.clone());
    assert!(Arc::ptr_eq(&shared, &first));
    let duplicate = cache.share("assets/a.bin", Arc::from(vec![9u8]));
    assert!(Arc::ptr_eq(&duplicate, &first));
    assert_eq!(cache.resident(), 1);
    drop(first);
    drop(shared);
    assert!(cache.get("assets/a.bin").is_some());
    drop(duplicate);
    assert!(cache.get("assets/a.bin").is_none());
    assert_eq!(cache.resident(), 0);
}
