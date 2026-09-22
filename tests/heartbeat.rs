mod common;

use std::time::{Duration, Instant};

use common::{main_script, run_both, run_source};
use mlua::Table;

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[tokio::test]
async fn heartbeat_fires_with_the_delta_time() {
    let started = Instant::now();
    let outcome = run_script(
        r#"
        local Process = import("Process")
        assert(Process.Heartbeat.ClassName == "Signal")
        assert(Process.Heartbeat.Name == "Heartbeat")
        deltas = {}
        Process.Heartbeat:BindHandler("update", function(dt)
            table.insert(deltas, dt)
            if #deltas == 10 then
                Process.Heartbeat:UnBind("update")
            end
        end)
        "#,
    )
    .await;
    outcome.assert_clean();
    let deltas: Vec<f64> = outcome.global("deltas");
    assert_eq!(deltas.len(), 10);
    assert!(deltas.iter().all(|dt| *dt > 0.0 && *dt < 1.0), "{deltas:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn waiting_on_heartbeat_yields_one_tick() {
    let outcome = run_script(
        r#"
        local Process = import("Process")
        log = {}
        coroutine.wrap(function()
            local dt = Process.Heartbeat:Wait()
            table.insert(log, type(dt) == "number" and dt > 0 and "ticked" or "bad")
        end)()
        table.insert(log, "main")
        local first = Process.Heartbeat:Wait()
        local second = Process.Heartbeat:Wait()
        waited = first > 0 and second > 0
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["main", "ticked"]);
    assert!(outcome.global::<bool>("waited"));
}

#[tokio::test]
async fn heartbeat_keeps_the_game_running_until_unbound_or_destroyed() {
    let outcome = run_script(
        r#"
        local Process = import("Process")
        ticks = 0
        Process.Heartbeat:BindHandler("count", function()
            ticks += 1
            if ticks == 3 then
                Process.Heartbeat:Destroy()
            end
        end)
        "#,
    )
    .await;
    outcome.assert_clean();
    assert_eq!(outcome.global::<i64>("ticks"), 3);
}

#[tokio::test]
async fn heartbeat_handlers_can_exit_the_game() {
    let outcome = run_script(
        r#"
        local Process = import("Process")
        frames = 0
        Process.Heartbeat:BindHandler("loop", function(dt)
            frames += 1
            if frames == 5 then
                Process.exit(0)
            end
        end)
        "#,
    )
    .await;
    outcome.assert_clean();
    assert_eq!(outcome.global::<i64>("frames"), 5);
    assert_eq!(outcome.runtime.engine().exit_code(), Some(0));
}

#[tokio::test]
async fn parallel_blocks_have_their_own_heartbeat() {
    let dir = main_script(
        r#"
        local Messenger = import("Messenger")
        Messenger:Subscribe("Ticks", function(count, thread)
            result = { count = count, thread = thread }
        end)

        EnterParallel()
        local Process = import("Process")
        local count = 0
        Process.Heartbeat:BindHandler("tick", function(dt)
            count += 1
            if count == 3 then
                Process.Heartbeat:UnBind("tick")
                Messenger:Fire("Ticks", count, threadName())
            end
        end)
        ExitParallel()
        "#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let result: Table = outcome.global("result");
        assert_eq!(result.get::<i64>("count").unwrap(), 3);
        assert_eq!(result.get::<String>("thread").unwrap(), "parallel block #1 of src/main.luau");
    }
}

#[tokio::test]
async fn games_without_a_heartbeat_listener_still_finish() {
    let started = Instant::now();
    let outcome = run_script("local Process = import(\"Process\")\nfinished = true\n").await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("finished"));
    assert!(started.elapsed() < Duration::from_secs(5));
}
