mod common;

use common::{run_with, workspace, write};
use mlua::Table;
use std::sync::Arc;

async fn run_mods(main: &str, mods: &[(&str, &str)]) -> common::Outcome {
    run_mods_with(main, mods, &[]).await
}

async fn run_mods_with(main: &str, mods: &[(&str, &str)], files: &[(&str, Vec<u8>)]) -> common::Outcome {
    let dir = workspace(&[("src/main.luau", main)]);
    for (name, source) in mods {
        write(dir.path(), name, source);
    }
    for (name, bytes) in files {
        let target = dir.path().join(name);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, bytes).unwrap();
    }
    let project = luv::project::Project::load(dir.path()).unwrap();
    let root = dir.path().to_path_buf();
    run_with(Arc::new(project.source_vfs()), "src/main.luau", move |builder| {
        builder.game("Mods", root.clone())
    })
    .await
}

#[tokio::test]
async fn ecall_compiles_an_external_file_and_caches_it() {
    let outcome = run_mods(
        r#"
local counted = 0
local handle = ecall("extras/greeter")
results = {
    class = handle.ClassName,
    name = handle.Name,
    folder = handle.Folder,
    source = handle.Source:find("extras/greeter") ~= nil,
    loadedBefore = handle.IsLoaded,
}

local api = handle:Fetch()
results.loadedAfter = handle.IsLoaded
results.greeting = api.greet("world")
results.runs = api.runs

local again = handle:Fetch()
results.sameTable = again == api

local second = ecall("extras/greeter")
results.shared = second:Fetch() == api
results.differentHandle = second ~= handle

results.dropped = handle:Drop()
results.droppedTwice = handle:Drop()
results.loadedAfterDrop = handle.IsLoaded

local fresh = ecall("extras/greeter"):Fetch()
results.freshIsNew = fresh ~= api
results.freshRuns = fresh.runs
"#,
        &[(
            "extras/greeter/init.luau",
            r#"
local runs = 1
return {
    runs = runs,
    greet = function(who: string): string
        return `hello {who}`
    end,
}
"#,
        )],
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap();
    let flag = |key: &str| results.get::<bool>(key).unwrap();
    assert_eq!(text("class"), "ExternalModule");
    assert_eq!(text("name"), "greeter");
    assert_eq!(text("folder"), "mods/greeter");
    assert!(flag("source"));
    assert!(!flag("loadedBefore"));
    assert!(flag("loadedAfter"));
    assert_eq!(text("greeting"), "hello world");
    assert_eq!(results.get::<f64>("runs").unwrap(), 1.0);
    assert!(flag("sameTable"), "Fetch should return the same module twice");
    assert!(flag("shared"), "a second ecall of the same file should share the module");
    assert!(flag("differentHandle"));
    assert!(flag("dropped"));
    assert!(!flag("droppedTwice"), "dropping twice should report false");
    assert!(!flag("loadedAfterDrop"));
    assert!(flag("freshIsNew"), "after a drop the file should run again");
}

#[tokio::test]
async fn a_dropped_module_stays_alive_for_whoever_holds_it() {
    let outcome = run_mods(
        r#"
local shared = ecall("extras/shared")
local user = ecall("extras/user")

local data = shared:Fetch()
local holder = user:Fetch()

data.count = 7
results = { before = holder.read() }

results.dropped = shared:Drop()
results.stillReads = holder.read()

local rebuilt = ecall("extras/shared"):Fetch()
results.rebuiltCount = rebuilt.count
results.twoCopies = rebuilt ~= data
"#,
        &[
            ("extras/shared/init.luau", "return { count = 0 }\n"),
            (
                "extras/user/init.luau",
                r#"
local shared = ecall("extras/shared"):Fetch()
return { read = function(): number return shared.count end }
"#,
            ),
        ],
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(results.get::<f64>("before").unwrap(), 7.0);
    assert!(results.get::<bool>("dropped").unwrap());
    assert_eq!(
        results.get::<f64>("stillReads").unwrap(),
        7.0,
        "a holder keeps the dropped module"
    );
    assert_eq!(results.get::<f64>("rebuiltCount").unwrap(), 0.0);
    assert!(results.get::<bool>("twoCopies").unwrap(), "fetching after a drop makes a second copy");
}

#[tokio::test]
async fn ecall_reports_bad_paths_and_broken_files() {
    let outcome = run_mods(
        r#"
results = {
    missing = tostring(select(2, pcall(ecall, "extras/nope"))),
    empty = tostring(select(2, pcall(ecall, "   "))),
    broken = tostring(select(2, pcall(ecall, "extras/broken"))),
}
local raises = ecall("extras/raises")
results.raised = tostring(select(2, pcall(function() return raises:Fetch() end)))
"#,
        &[
            ("extras/broken/init.luau", "local = = =\n"),
            ("extras/raises/init.luau", "error(\"module said no\")\n"),
        ],
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap();
    assert!(text("missing").contains("cannot read"), "{}", text("missing"));
    assert!(text("empty").contains("needs the path"), "{}", text("empty"));
    assert!(text("broken").to_lowercase().contains("expected"), "{}", text("broken"));
    assert!(text("raised").contains("module said no"), "{}", text("raised"));
}

#[tokio::test]
async fn a_whole_folder_is_compiled_and_mounted() {
    let png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10, 1, 2, 3, 4];
    let outcome = run_mods_with(
        r#"
local pack = ecall("extras/pack")
results = {
    folder = pack.Folder,
    entry = pack.Entry,
    files = pack.Files,
    listed = table.concat(pack:GetFiles(), ","),
}
local mod = pack:Fetch()
results.given = mod.given
results.fromRequire = mod.fromRequire
results.fromDeepRequire = mod.fromDeepRequire
results.readBytes = mod.readBytes
results.assetName = mod.asset.Name
results.assetSize = mod.asset.Size
results.dataAsset = mod.data.Size
"#,
        &[
            (
                "extras/pack/init.luau",
                r#"
local folder = ...
local Asset = import("Asset")
local FS = import("FS")

local helper = require("@self/helper")
local deep = require("@self/parts/deep")

return {
    given = folder,
    fromRequire = helper.name,
    fromDeepRequire = deep.name,
    readBytes = #FS.readFile("@self/icon.png"),
    asset = Asset.Load("icon.png"),
    data = Asset.Load("notes.txt"),
}
"#,
            ),
            ("extras/pack/helper.luau", "return { name = \"helper\" }
"),
            ("extras/pack/parts/deep.luau", "return { name = \"deep\" }
"),
        ],
        &[
            ("extras/pack/icon.png", png.clone()),
            ("extras/pack/assets/notes.txt", b"mod notes".to_vec()),
        ],
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap();
    assert_eq!(text("folder"), "mods/pack");
    assert_eq!(text("entry"), "mods/pack/init.luau");
    assert_eq!(results.get::<usize>("files").unwrap(), 5, "every file in the folder is mounted");
    assert_eq!(text("given"), "mods/pack", "the mod gets its mounted folder");
    assert_eq!(text("fromRequire"), "helper", "require works inside a mod");
    assert_eq!(text("fromDeepRequire"), "deep", "require reaches a subfolder");
    assert_eq!(results.get::<usize>("readBytes").unwrap(), png.len(), "FS reads the mod's own files");
    assert_eq!(text("assetName"), "icon.png");
    assert_eq!(results.get::<usize>("assetSize").unwrap(), png.len());
    assert_eq!(
        results.get::<usize>("dataAsset").unwrap(),
        9,
        "Asset.Load finds the mod's own assets folder"
    );
    assert!(text("listed").contains("mods/pack/parts/deep.luau"));
}

#[tokio::test]
async fn a_module_loads_the_assets_sitting_next_to_it() {
    let png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10, 1, 2, 3, 4];

    let outcome = run_mods_with(
        r#"
local pack = ecall("extras/pack")
results = {
    folderIsMounted = pack.Folder == "mods/pack",
    entryInsideFolder = pack.Entry == "mods/pack/init.luau",
}
local mod = pack:Fetch()
results.gotFolder = mod.folder == pack.Folder
results.iconClass = mod.icon.ClassName
results.iconName = mod.icon.Name
results.iconSize = mod.icon.Size
results.iconExtension = mod.icon.Extension
"#,
        &[(
            "extras/pack/init.luau",
            r#"
local folder = ...
local Asset = import("Asset")
local FS = import("FS")

return {
    folder = folder,
    icon = Asset.FromBytes("icon.png", FS.readFile("@self/icon.png")),
}
"#,
        )],
        &[("extras/pack/icon.png", png.clone())],
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!(results.get::<bool>("folderIsMounted").unwrap());
    assert!(results.get::<bool>("entryInsideFolder").unwrap());
    assert!(results.get::<bool>("gotFolder").unwrap(), "a module gets its folder as the first vararg");
    assert_eq!(results.get::<String>("iconClass").unwrap(), "Asset");
    assert_eq!(results.get::<String>("iconName").unwrap(), "icon.png");
    assert_eq!(results.get::<String>("iconExtension").unwrap(), "png");
    assert_eq!(results.get::<usize>("iconSize").unwrap(), png.len());
}

#[tokio::test]
async fn setglobal_exposes_values_to_a_mod() {
    let outcome = run_mods(
        r#"
SetGlobal("hostVersion", 3)
SetGlobal("hostGreet", function(who: string): string
    return `host says hi to {who}`
end)
SetGlobal("hostTools", { add = function(a: number, b: number): number return a + b end })

local mod = ecall("extras/user"):Fetch()
results = {
    sawVersion = mod.version,
    sawGreeting = mod.greeting,
    sawSum = mod.sum,
    direct = hostVersion,
    badName = tostring(select(2, pcall(SetGlobal, "not a name", 1))),
    empty = tostring(select(2, pcall(SetGlobal, "  ", 1))),
    reserved = tostring(select(2, pcall(SetGlobal, "import", 1))),
    stillImports = type(import),
}
"#,
        &[(
            "extras/user/init.luau",
            r#"
return {
    version = hostVersion,
    greeting = hostGreet("the mod"),
    sum = hostTools.add(2, 3),
}
"#,
        )],
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap();
    assert_eq!(results.get::<f64>("sawVersion").unwrap(), 3.0, "a mod sees a global the host set");
    assert_eq!(text("sawGreeting"), "host says hi to the mod");
    assert_eq!(results.get::<f64>("sawSum").unwrap(), 5.0);
    assert_eq!(results.get::<f64>("direct").unwrap(), 3.0);
    assert!(text("badName").contains("is not a valid global name"), "{}", text("badName"));
    assert!(text("empty").contains("needs a name"), "{}", text("empty"));
    assert!(text("reserved").contains("belongs to luv"), "{}", text("reserved"));
    assert_eq!(text("stillImports"), "function");
}
