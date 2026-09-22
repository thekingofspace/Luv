mod common;

use common::{main_script, run_both, run_source};
use mlua::Table;

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[tokio::test]
async fn enums_are_global_values() {
    let outcome = run_script(
        r#"
        local item = enum.WindowType.FullScreen
        local names = {}
        for _, entry in enum.WindowType:GetEnumItems() do
            table.insert(names, entry.Name)
        end
        results = {
            name = item.Name,
            value = item.Value,
            enumType = item.EnumType,
            text = tostring(item),
            typeName = typeof(item),
            names = table.concat(names, ","),
            same = item == enum.WindowType.FullScreen,
            different = item ~= enum.WindowType.Windowed,
            frozen = not pcall(function() enum.WindowType.Extra = 1 end),
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(results.get::<String>("name").unwrap(), "FullScreen");
    assert_eq!(results.get::<i64>("value").unwrap(), 3);
    assert_eq!(results.get::<String>("enumType").unwrap(), "WindowType");
    assert_eq!(results.get::<String>("text").unwrap(), "enum.WindowType.FullScreen");
    assert_eq!(results.get::<String>("typeName").unwrap(), "EnumItem");
    assert_eq!(
        results.get::<String>("names").unwrap(),
        "Windowed,Borderless,Maximized,FullScreen,ExclusiveFullScreen"
    );
    for key in ["same", "different", "frozen"] {
        assert!(results.get::<bool>(key).unwrap(), "{key}");
    }
}

#[tokio::test]
async fn enum_items_cross_threads_as_the_same_item() {
    let dir = main_script(
        r#"
local Messenger = import("Messenger")
Messenger:Subscribe("Mode", function(mode, lookup)
    received = {
        same = rawequal(mode, enum.WindowType.Borderless),
        keyed = lookup[enum.WindowType.Borderless] == "found",
    }
end)
local choice = enum.WindowType.Borderless
EnterParallel()
local lookup = { [choice] = "found" }
Messenger:Fire("Mode", choice, lookup)
ExitParallel()
"#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let received: Table = outcome.global("received");
        assert!(received.get::<bool>("same").unwrap());
        assert!(received.get::<bool>("keyed").unwrap());
    }
}

#[tokio::test]
async fn bind_to_close_runs_when_the_game_finishes() {
    let outcome = run_script(
        r#"
        local Process = import("Process")
        log = {}
        Process.BindToClose(function()
            sleep(10)
            table.insert(log, "first")
        end)
        Process.BindToClose(function()
            table.insert(log, "second")
        end)
        table.insert(log, "main")
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["main", "second", "first"]);
}

#[tokio::test]
async fn bind_to_close_runs_before_exit() {
    let outcome = run_script(
        r#"
        local Process = import("Process")
        Process.BindToClose(function()
            sleep(10)
            saved = true
        end)
        Process.exit(4)
        reached = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("saved"));
    assert_eq!(outcome.global::<Option<bool>>("reached"), None);
    assert_eq!(outcome.runtime.engine().exit_code(), Some(4));
}

#[tokio::test]
async fn bind_to_close_reaches_parallel_blocks() {
    let dir = main_script(
        r#"
local Process = import("Process")
local Messenger = import("Messenger")
Process.BindToClose(function()
    goodbye = Messenger:Wait("Goodbye")
end)

EnterParallel()
local Process = import("Process")
Process.BindToClose(function()
    Messenger:Fire("Goodbye", threadName())
end)
ExitParallel()
"#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        assert_eq!(outcome.global::<String>("goodbye"), "parallel block #1 of src/main.luau");
    }
}

#[tokio::test]
async fn close_callbacks_start_before_later_messages_arrive() {
    let dir = main_script(
        r#"
local Process = import("Process")
local Messenger = import("Messenger")
Process.BindToClose(function()
    goodbye = Messenger:Wait("Goodbye")
end)

EnterParallel()
local Process = import("Process")
coroutine.wrap(function()
    Process.exit(0)
end)()
Messenger:Fire("Goodbye", "sent while closing")
ExitParallel()
"#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        assert_eq!(outcome.global::<String>("goodbye"), "sent while closing");
        assert_eq!(outcome.runtime.engine().exit_code(), Some(0));
    }
}

#[tokio::test]
async fn bind_to_close_hears_every_parallel_goodbye() {
    let dir = main_script(
        r#"
local Process = import("Process")
local Messenger = import("Messenger")
goodbyes = {}
Process.BindToClose(function()
    for _ = 1, 8 do
        table.insert(goodbyes, (Messenger:Wait("Goodbye")))
    end
    table.sort(goodbyes)
end)

for index = 1, 8 do
    EnterParallel()
    local Process = import("Process")
    Process.BindToClose(function()
        Messenger:Fire("Goodbye", index)
    end)
    ExitParallel()
end
"#,
    );
    for _ in 0..3 {
        for outcome in run_both(dir.path()).await {
            outcome.assert_clean();
            assert_eq!(outcome.global::<Vec<i64>>("goodbyes"), [1, 2, 3, 4, 5, 6, 7, 8]);
        }
    }
}

#[tokio::test]
async fn close_callback_errors_are_reported() {
    let outcome = run_script(
        r#"
        local Process = import("Process")
        Process.BindToClose(function()
            error("could not save")
        end)
        "#,
    )
    .await;
    assert_eq!(outcome.errors.len(), 1, "{:#?}", outcome.errors);
    assert!(outcome.errors[0].contains("could not save"), "{}", outcome.errors[0]);
}
