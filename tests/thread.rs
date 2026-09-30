mod common;

use common::{main_script, run_source};
use mlua::Table;

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[tokio::test]
async fn the_main_thread_knows_itself() {
    let outcome = run_script(
        r#"
        local Thread = import("Thread")
        local me = Thread.Running()
        Thread.Set("Game", { version = 2 })
        local all = Thread.Get()
        result = {
            name = me.Name,
            main = me.IsMain,
            current = me.IsCurrent,
            alive = me.IsAlive,
            state = me.State,
            version = me.Data.version,
            class = me.ClassName,
            count = #all,
            same = all[1] == me,
            text = tostring(me),
            byState = #Thread.Get("Game"),
            none = #Thread.Get("Nothing"),
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert_eq!(result.get::<String>("name").unwrap(), "main");
    assert!(result.get::<bool>("main").unwrap());
    assert!(result.get::<bool>("current").unwrap());
    assert!(result.get::<bool>("alive").unwrap());
    assert_eq!(result.get::<String>("state").unwrap(), "Game");
    assert_eq!(result.get::<i64>("version").unwrap(), 2);
    assert_eq!(result.get::<String>("class").unwrap(), "Thread");
    assert_eq!(result.get::<i64>("count").unwrap(), 1);
    assert!(result.get::<bool>("same").unwrap());
    assert_eq!(result.get::<String>("text").unwrap(), "Thread(main)");
    assert_eq!(result.get::<i64>("byState").unwrap(), 1);
    assert_eq!(result.get::<i64>("none").unwrap(), 0);
}

#[tokio::test]
async fn threads_find_each_other_and_talk_directly() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        local Thread = import("Thread")

        EnterParallel()
        local Messenger = import("Messenger")
        local Thread = import("Thread")
        Thread.Set("Entities", { level = 3 })
        local subscription
        subscription = Messenger:Subscribe("spawn", function(count)
            Messenger:Unsubscribe(subscription)
            Thread.Get()[1]:Send("spawned", count * 2, Thread.Running().Name)
        end)
        Thread.Running():MarkReady()
        ExitParallel()

        local worker = Thread.WaitFor("Entities", 2)
        local ready = worker:WaitReady(2)
        result = {
            found = worker ~= nil,
            state = worker.State,
            level = worker.Data.level,
            main = worker.IsMain,
            name = worker.Name,
            ready = ready,
            isReady = worker.IsReady,
        }
        local delivered = worker:Send("spawn", 21)
        local doubled, from = Messenger:Wait("spawned")
        result = {
            found = result.found,
            state = result.state,
            level = result.level,
            main = result.main,
            name = result.name,
            ready = result.ready,
            isReady = result.isReady,
            delivered = delivered,
            doubled = doubled,
            from = from,
            timedOut = Thread.WaitFor("Missing", 0.05) == nil,
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert!(result.get::<bool>("found").unwrap());
    assert_eq!(result.get::<String>("state").unwrap(), "Entities");
    assert_eq!(result.get::<i64>("level").unwrap(), 3);
    assert!(!result.get::<bool>("main").unwrap());
    assert!(result.get::<String>("name").unwrap().starts_with("parallel block #1"));
    assert!(result.get::<bool>("ready").unwrap());
    assert!(result.get::<bool>("isReady").unwrap());
    assert!(result.get::<bool>("delivered").unwrap());
    assert_eq!(result.get::<i64>("doubled").unwrap(), 42);
    assert!(result.get::<String>("from").unwrap().starts_with("parallel block #1"));
    assert!(result.get::<bool>("timedOut").unwrap());
}

#[tokio::test]
async fn a_direct_send_only_reaches_its_thread() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        local Thread = import("Thread")

        for index = 1, 2 do
            EnterParallel()
            local Messenger = import("Messenger")
            local Thread = import("Thread")
            Thread.Set("Worker" .. index)
            local subscription
            subscription = Messenger:Subscribe("ping", function()
                Messenger:Unsubscribe(subscription)
                Messenger:Fire("pong", index)
            end)
            Thread.Running():MarkReady()
            ExitParallel()
        end

        local first = Thread.WaitFor("Worker1", 2)
        local second = Thread.WaitFor("Worker2", 2)
        first:WaitReady(2)
        second:WaitReady(2)
        first:Send("ping")
        local answer = Messenger:Wait("pong")
        task.wait(0.1)
        local stillWaiting = second.IsAlive
        second:Send("ping")
        local other = Messenger:Wait("pong")
        result = { answer = answer, other = other, stillWaiting = stillWaiting }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert_eq!(result.get::<i64>("answer").unwrap(), 1);
    assert!(result.get::<bool>("stillWaiting").unwrap(), "the second worker never got the first ping");
    assert_eq!(result.get::<i64>("other").unwrap(), 2);
}

#[tokio::test]
async fn only_a_thread_can_mark_itself_ready() {
    let outcome = run_script(
        r#"
        local Thread = import("Thread")

        EnterParallel()
        local Thread = import("Thread")
        Thread.Set("Slow")
        task.wait(0.2)
        ExitParallel()

        local worker = Thread.WaitFor("Slow", 2)
        local ok, problem = pcall(function()
            worker:MarkReady()
        end)
        local first = Thread.Running():MarkReady()
        local again = Thread.Running():MarkReady()
        local gaveUp = worker:WaitReady(0.05)
        task.wait(0.4)
        result = {
            ok = ok,
            problem = tostring(problem),
            first = first,
            again = again,
            gaveUp = gaveUp,
            ended = not worker.IsAlive,
            endedState = worker.State == nil,
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert!(!result.get::<bool>("ok").unwrap());
    assert!(result.get::<String>("problem").unwrap().contains("only a thread can mark itself ready"));
    assert!(result.get::<bool>("first").unwrap());
    assert!(!result.get::<bool>("again").unwrap());
    assert!(!result.get::<bool>("gaveUp").unwrap());
    assert!(result.get::<bool>("ended").unwrap());
    assert!(result.get::<bool>("endedState").unwrap());
}
