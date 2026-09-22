mod common;

use common::{main_script, run_source};

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[tokio::test]
async fn messenger_is_imported_and_inherits_base_game_object() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        assert(Messenger == import("Messenger"))
        assert(Messenger.ClassName == "Messenger")
        assert(Messenger.Name == "Messenger")

        local ok, err = pcall(import, "Nope")
        assert(not ok and string.find(tostring(err), "'Nope' cannot be imported, the available imports are Asset, Bulk, Container, Crypto, DLL, FS, Messenger, Net, Process, Random, Serde, Shader, Signal, Viewport, Window", 1, true))
        done = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("done"));
}

#[tokio::test]
async fn fire_never_blocks_and_delivers_to_subscribers_later() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        log = {}
        Messenger:Subscribe("Ping", function(a, b)
            table.insert(log, `received {a} {b}`)
        end)
        Messenger:Fire("Ping", "hello", 5)
        table.insert(log, "fired")
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["fired", "received hello 5"]);
}

#[tokio::test]
async fn subscribers_only_receive_their_topic() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        log = {}
        Messenger:Subscribe("A", function(value) table.insert(log, "A" .. value) end)
        Messenger:Subscribe("B", function(value) table.insert(log, "B" .. value) end)
        Messenger:Fire("A", 1)
        Messenger:Fire("B", 2)
        Messenger:Fire("C", 3)
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["A1", "B2"]);
}

#[tokio::test]
async fn unsubscribe_stops_delivery() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        count = 0
        local id = Messenger:Subscribe("Tick", function() count += 1 end)
        Messenger:Fire("Tick")
        Messenger:Subscribe("Stop", function()
            assert(Messenger:Unsubscribe(id) == true)
            assert(Messenger:Unsubscribe(id) == false)
            Messenger:Fire("Tick")
        end)
        Messenger:Fire("Stop")
        "#,
    )
    .await;
    outcome.assert_clean();
    assert_eq!(outcome.global::<i64>("count"), 1);
}

#[tokio::test]
async fn wait_suspends_only_the_waiting_coroutine() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        log = {}
        coroutine.wrap(function()
            local value, extra = Messenger:Wait("Ready")
            table.insert(log, `waiter got {value} {extra}`)
        end)()
        table.insert(log, "main continued")
        Messenger:Fire("Ready", "go", true)
        table.insert(log, "main fired")
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["main continued", "main fired", "waiter got go true"]);
}

#[tokio::test]
async fn messages_are_copies_of_low_level_values() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        local original = { 1, 2, nested = { x = 3 }, [true] = "yes" }
        local data = buffer.create(4)
        buffer.writeu32(data, 0, 123456)

        Messenger:Subscribe("Data", function(nothing, flag, number, text, list, raw, point)
            assert(nothing == nil)
            assert(flag == false)
            assert(number == 1.5)
            assert(text == "text")
            assert(list ~= original)
            assert(list[1] == 1 and list[2] == 2 and list.nested.x == 3 and list[true] == "yes")
            assert(raw ~= data and buffer.readu32(raw, 0) == 123456)
            assert(point == vector.create(1, 2, 3))
            received = true
        end)
        Messenger:Fire("Data", nil, false, 1.5, "text", original, data, vector.create(1, 2, 3))
        original.nested.x = 99
        buffer.writeu32(data, 0, 0)
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("received"));
}

#[tokio::test]
async fn high_level_values_cannot_be_sent() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        local Signal = import("Signal")

        local function check(value, expected)
            local ok, err = pcall(Messenger.Fire, Messenger, "Topic", value)
            assert(not ok, "expected an error for " .. expected)
            assert(string.find(tostring(err), expected, 1, true), tostring(err))
        end

        check(function() end, "function values cannot be sent between threads")
        check(coroutine.create(function() end), "thread values cannot be sent between threads")
        check(Signal.new(), "objects cannot be sent between threads")
        check({ inner = { print } }, "function values cannot be sent between threads")

        local cyclic = {}
        cyclic.self = cyclic
        check(cyclic, "tables that contain themselves cannot be sent between threads")

        local shared = { 1 }
        Messenger:Fire("Topic", { a = shared, b = shared })
        done = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("done"));
}

#[tokio::test]
async fn destroying_the_messenger_releases_waiters() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        coroutine.wrap(function()
            local ok, err = pcall(Messenger.Wait, Messenger, "Never")
            waitError = tostring(err)
        end)()
        Messenger:Subscribe("Anything", function() end)
        Messenger:Destroy()
        assert(not pcall(Messenger.Subscribe, Messenger, "Again", function() end))
        assert(not pcall(Messenger.Fire, Messenger, "Again"))
        "#,
    )
    .await;
    outcome.assert_clean();
    let wait_error: String = outcome.global("waitError");
    assert!(wait_error.contains("Messenger was destroyed while waiting for 'Never'"), "{wait_error}");
}

#[tokio::test]
async fn unanswered_waits_do_not_keep_the_game_running() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        coroutine.wrap(function()
            Messenger:Wait("Never")
            resumed = true
        end)()
        finished = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("finished"));
    assert_eq!(outcome.global::<Option<bool>>("resumed"), None);
}
