mod common;

use common::{main_script, run_source};

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[tokio::test]
async fn engine_waits_inside_plain_coroutines_only_suspend_that_coroutine() {
    let outcome = run_script(
        r#"
        log = {}
        local co = coroutine.create(function(label)
            table.insert(log, label .. " start")
            sleep(10)
            table.insert(log, label .. " end")
        end)
        local ok, extra = coroutine.resume(co, "co")
        assert(ok == true and extra == nil)
        assert(coroutine.status(co) == "suspended")

        local again, message = coroutine.resume(co)
        assert(again == false and message == "cannot resume a coroutine that is waiting on the engine")

        coroutine.wrap(function()
            sleep(5)
            table.insert(log, "wrapped")
        end)()
        table.insert(log, "main")
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["co start", "main", "wrapped", "co end"]);
}

#[tokio::test]
async fn plain_coroutine_yields_behave_normally() {
    let outcome = run_script(
        r#"
        local co = coroutine.create(function(a)
            local b = coroutine.yield(a + 1)
            return b * 2
        end)
        local ok, first = coroutine.resume(co, 1)
        assert(ok and first == 2)
        local ok2, second = coroutine.resume(co, 5)
        assert(ok2 and second == 10)
        assert(coroutine.status(co) == "dead")

        local gen = coroutine.wrap(function()
            for i = 1, 3 do
                coroutine.yield(i)
            end
        end)
        values = { gen(), gen(), gen() }
        "#,
    )
    .await;
    outcome.assert_clean();
    let values: Vec<i64> = outcome.global("values");
    assert_eq!(values, [1, 2, 3]);
}

#[tokio::test]
async fn coroutine_errors_keep_their_values() {
    let outcome = run_script(
        r#"
        local co = coroutine.create(function()
            error({ code = 7 })
        end)
        local ok, err = coroutine.resume(co)
        assert(not ok and type(err) == "table" and err.code == 7)

        local wrapped = coroutine.wrap(function()
            error("wrapped failure", 0)
        end)
        local ok2, err2 = pcall(wrapped)
        assert(not ok2 and err2 == "wrapped failure")
        done = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("done"));
}

#[tokio::test]
async fn coroutines_that_yield_after_an_engine_wait_can_be_resumed_again() {
    let outcome = run_script(
        r#"
        log = {}
        local co
        co = coroutine.create(function()
            sleep(5)
            table.insert(log, "slept")
            local value = coroutine.yield()
            table.insert(log, "resumed with " .. value)
        end)
        coroutine.resume(co)
        sleep(30)
        assert(coroutine.status(co) == "suspended")
        local ok = coroutine.resume(co, "hello")
        assert(ok)
        assert(coroutine.status(co) == "dead")
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["slept", "resumed with hello"]);
}

#[tokio::test]
async fn errors_after_an_engine_wait_are_reported() {
    let outcome = run_script(
        r#"
        coroutine.wrap(function()
            sleep(5)
            error("late failure")
        end)()
        "#,
    )
    .await;
    assert_eq!(outcome.errors.len(), 1, "{:#?}", outcome.errors);
    assert!(outcome.errors[0].contains("late failure"), "{}", outcome.errors[0]);
}

#[tokio::test]
async fn import_works_inside_coroutines() {
    let outcome = run_script(
        r#"
        coroutine.wrap(function()
            local Signal = import("Signal")
            sleep(5)
            local Messenger = import("Messenger")
            imported = Signal ~= nil and Messenger ~= nil
        end)()
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("imported"));
}
