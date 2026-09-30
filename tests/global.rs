mod common;

use std::sync::Arc;

use common::{main_script, run_source, run_with};
use luv::project::Project;
use luv::window::HeadlessWindows;
use mlua::Table;

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[tokio::test]
async fn global_new_sets_any_value_and_protects_luv_names() {
    let outcome = run_script(
        r#"
        global.new("greet", function(name)
            return "hello " .. name
        end)
        global.new("settings", { volume = 3 })
        SetGlobal("legacy", 7)
        local reserved, reservedProblem = pcall(global.new, "task", {})
        local invalid = pcall(global.new, "1abc", 1)
        local ownName = pcall(global.new, "global", {})
        result = {
            greeting = greet("world"),
            volume = settings.volume,
            legacy = legacy,
            reserved = reserved,
            reservedProblem = tostring(reservedProblem),
            invalid = invalid,
            ownName = ownName,
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert_eq!(result.get::<String>("greeting").unwrap(), "hello world");
    assert_eq!(result.get::<i64>("volume").unwrap(), 3);
    assert_eq!(result.get::<i64>("legacy").unwrap(), 7);
    assert!(!result.get::<bool>("reserved").unwrap());
    assert!(result.get::<String>("reservedProblem").unwrap().contains("'task' belongs to luv"));
    assert!(!result.get::<bool>("invalid").unwrap());
    assert!(!result.get::<bool>("ownName").unwrap());
}

#[tokio::test]
async fn global_new_import_is_reachable_through_import() {
    let outcome = run_script(
        r#"
        local Items = { count = 2 }
        function Items.describe()
            return "items"
        end
        global.newImport("Items", Items)
        local imported = import("Items")
        local clash = pcall(global.newImport, "Asset", {})
        local missing, problem = pcall(import, "Nothing")
        result = {
            same = imported == Items,
            described = imported.describe(),
            clash = clash,
            listed = string.find(tostring(problem), "Items", 1, true) ~= nil,
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert!(result.get::<bool>("same").unwrap());
    assert_eq!(result.get::<String>("described").unwrap(), "items");
    assert!(!result.get::<bool>("clash").unwrap());
    assert!(result.get::<bool>("listed").unwrap());
}

#[tokio::test]
async fn global_new_api_hands_every_function_the_window() {
    let dir = main_script(
        r#"
        local Window = import("Window")
        local window = Window.new({ Title = "Api" })

        local Hud = { Version = 2 }
        function Hud.Show(target, text)
            return target, text
        end
        global.newAPI("Hud", Hud)

        local api = window:GetAPI("Hud")
        local dotWindow, dotText = api.Show("dot")
        local colonWindow, colonText = api:Show("colon")
        function Hud.Later(target)
            return target.Title
        end
        local later = api.Later()
        local cannotWrite = pcall(function()
            api.Version = 3
        end)
        local builtIn = pcall(global.newAPI, "Sound", {})
        local unknown, problem = pcall(function()
            return window:GetAPI("Nothing")
        end)
        result = {
            dotIsWindow = dotWindow == window,
            dotText = dotText,
            colonIsWindow = colonWindow == window,
            colonText = colonText,
            version = api.Version,
            later = later,
            cached = window:GetAPI("Hud") == api,
            cannotWrite = cannotWrite,
            builtIn = builtIn,
            listed = string.find(tostring(problem), "Hud", 1, true) ~= nil,
        }
        window:Close()
        "#,
    );
    let project = Project::load(dir.path()).unwrap();
    let headless = Arc::new(HeadlessWindows::new());
    let outcome = run_with(Arc::new(project.source_vfs()), "src/main.luau", move |builder| {
        builder.game("Fixture", ".").windows(headless.clone())
    })
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert!(result.get::<bool>("dotIsWindow").unwrap());
    assert_eq!(result.get::<String>("dotText").unwrap(), "dot");
    assert!(result.get::<bool>("colonIsWindow").unwrap());
    assert_eq!(result.get::<String>("colonText").unwrap(), "colon");
    assert_eq!(result.get::<i64>("version").unwrap(), 2);
    assert_eq!(result.get::<String>("later").unwrap(), "Api");
    assert!(result.get::<bool>("cached").unwrap());
    assert!(!result.get::<bool>("cannotWrite").unwrap());
    assert!(!result.get::<bool>("builtIn").unwrap());
    assert!(result.get::<bool>("listed").unwrap());
}
