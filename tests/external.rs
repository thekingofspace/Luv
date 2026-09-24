mod common;

use common::{run_with, workspace, write};
use mlua::Table;
use std::sync::Arc;

async fn run_mods(main: &str, mods: &[(&str, &str)]) -> common::Outcome {
    let dir = workspace(&[("src/main.luau", main)]);
    for (name, source) in mods {
        write(dir.path(), name, source);
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
local handle = ECall("extras/greeter.luau")
results = {
    class = handle.ClassName,
    name = handle.Name,
    module = handle.Module,
    absolute = handle.Path:find("extras/greeter.luau") ~= nil,
    loadedBefore = handle.IsLoaded,
}

local api = handle:Fetch()
results.loadedAfter = handle.IsLoaded
results.greeting = api.greet("world")
results.runs = api.runs

local again = handle:Fetch()
results.sameTable = again == api

local second = ECall("extras/greeter.luau")
results.shared = second:Fetch() == api
results.differentHandle = second ~= handle

results.dropped = handle:Drop()
results.droppedTwice = handle:Drop()
results.loadedAfterDrop = handle.IsLoaded

local fresh = ECall("extras/greeter.luau"):Fetch()
results.freshIsNew = fresh ~= api
results.freshRuns = fresh.runs
"#,
        &[(
            "extras/greeter.luau",
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
    assert_eq!(text("module"), "@mods/greeter");
    assert!(flag("absolute"));
    assert!(!flag("loadedBefore"));
    assert!(flag("loadedAfter"));
    assert_eq!(text("greeting"), "hello world");
    assert_eq!(results.get::<f64>("runs").unwrap(), 1.0);
    assert!(flag("sameTable"), "Fetch should return the same module twice");
    assert!(flag("shared"), "a second ECall of the same file should share the module");
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
local shared = ECall("extras/shared.luau")
local user = ECall("extras/user.luau")

local data = shared:Fetch()
local holder = user:Fetch()

data.count = 7
results = { before = holder.read() }

results.dropped = shared:Drop()
results.stillReads = holder.read()

local rebuilt = ECall("extras/shared.luau"):Fetch()
results.rebuiltCount = rebuilt.count
results.twoCopies = rebuilt ~= data
"#,
        &[
            ("extras/shared.luau", "return { count = 0 }\n"),
            (
                "extras/user.luau",
                r#"
local shared = ECall("extras/shared.luau"):Fetch()
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
    missing = tostring(select(2, pcall(ECall, "extras/nope.luau"))),
    empty = tostring(select(2, pcall(ECall, "   "))),
    broken = tostring(select(2, pcall(ECall, "extras/broken.luau"))),
}
local raises = ECall("extras/raises.luau")
results.raised = tostring(select(2, pcall(function() return raises:Fetch() end)))
"#,
        &[
            ("extras/broken.luau", "local = = =\n"),
            ("extras/raises.luau", "error(\"module said no\")\n"),
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
