mod common;

use std::sync::Arc;

use common::{main_script, run_source};
use luv::objects::{BaseGameObject, GameObject, Signal};
use luv::runtime::{Engine, Runtime};
use luv::vfs::DirVfs;
use mlua::{AnyUserData, Function, IntoLuaMulti, Lua};

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[test]
fn base_game_object_exposes_class_name_and_name() {
    let lua = Lua::new();
    lua.globals().set("object", BaseGameObject::new("Part")).unwrap();
    lua.load(
        r#"
        assert(object.ClassName == "Part")
        assert(object.Name == "Part")
        object.Name = "Floor"
        assert(object.Name == "Floor")
        assert(tostring(object) == "Floor")
        assert(not pcall(function() object.ClassName = "Other" end))
        object:Destroy()
        object:Destroy()
        "#,
    )
    .exec()
    .unwrap();

    let object: AnyUserData = lua.globals().get("object").unwrap();
    let object = object.borrow::<BaseGameObject>().unwrap();
    assert!(object.is_destroyed());
    assert!(object.ensure_alive().is_err());
}

#[tokio::test]
async fn signals_are_imported_and_inherit_base_game_object() {
    let outcome = run_script(
        r#"
        local Signal = import("Signal")
        local signal = Signal.new()
        assert(signal.ClassName == "Signal")
        assert(signal.Name == "Signal")
        signal.Name = "Touched"
        assert(tostring(signal) == "Touched")
        assert(not pcall(function() Signal.new = nil end))
        done = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("done"));
}

#[tokio::test]
async fn binds_and_invokes_handlers_by_id() {
    let outcome = run_script(
        r#"
        local signal = import("Signal").new()
        signal:BindHandler("double", function(x) return x * 2, "extra" end)
        assert(signal:IsBound("double"))
        assert(not signal:IsBound("triple"))
        local value, extra = signal:Invoke("double", 21)
        assert(value == 42 and extra == "extra")

        local ok, err = pcall(signal.Invoke, signal, "missing")
        assert(not ok and string.find(tostring(err), "no handler is bound to 'missing'", 1, true))
        done = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("done"));
}

#[tokio::test]
async fn rebinding_an_id_requires_unbind() {
    let outcome = run_script(
        r#"
        local signal = import("Signal").new()
        signal.Name = "Touched"
        signal:BindHandler("handler", function() return 1 end)
        local ok, err = pcall(signal.BindHandler, signal, "handler", function() return 2 end)
        assert(not ok and string.find(tostring(err), "handler 'handler' is already bound to Touched", 1, true))
        assert(signal:Invoke("handler") == 1)

        assert(signal:UnBind("handler") == true)
        assert(signal:UnBind("handler") == false)
        signal:BindHandler("handler", function() return 2 end)
        assert(signal:Invoke("handler") == 2)
        done = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("done"));
}

#[tokio::test]
async fn fire_runs_every_handler_on_its_own_coroutine() {
    let outcome = run_script(
        r#"
        local signal = import("Signal").new()
        log = {}
        local main = coroutine.running()
        signal:BindHandler("slow", function(value)
            assert(coroutine.running() ~= main)
            table.insert(log, "slow start " .. value)
            sleep(20)
            table.insert(log, "slow end " .. value)
        end)
        signal:BindHandler("fast", function(value)
            assert(coroutine.running() ~= main)
            table.insert(log, "fast " .. value)
        end)
        signal:Fire(1)
        table.insert(log, "fired")
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["slow start 1", "fast 1", "fired", "slow end 1"]);
}

#[tokio::test]
async fn fire_skips_handlers_unbound_while_firing() {
    let outcome = run_script(
        r#"
        local signal = import("Signal").new()
        log = {}
        signal:BindHandler("a", function()
            table.insert(log, "a")
            signal:UnBind("b")
            signal:UnBind("c")
            signal:BindHandler("c", function() table.insert(log, "new c") end)
            signal:BindHandler("d", function() table.insert(log, "d") end)
        end)
        signal:BindHandler("b", function() table.insert(log, "b") end)
        signal:BindHandler("c", function() table.insert(log, "old c") end)
        signal:Fire()
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["a", "new c"]);
}

#[tokio::test]
async fn wait_suspends_only_the_waiting_coroutine() {
    let outcome = run_script(
        r#"
        local signal = import("Signal").new()
        log = {}
        coroutine.wrap(function()
            local a, b = signal:Wait()
            table.insert(log, `waiter got {a} {b}`)
        end)()
        table.insert(log, "main continued")
        signal:Fire("x", 2)
        table.insert(log, "main fired")
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["main continued", "main fired", "waiter got x 2"]);
}

#[tokio::test]
async fn invoke_waits_without_blocking_other_coroutines() {
    let outcome = run_script(
        r#"
        local signal = import("Signal").new()
        log = {}
        signal:BindHandler("slow", function(value)
            sleep(20)
            return value * 10
        end)
        coroutine.wrap(function()
            table.insert(log, "invoked " .. signal:Invoke("slow", 4))
        end)()
        table.insert(log, "main continued")
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["main continued", "invoked 40"]);
}

#[tokio::test]
async fn handler_errors_are_reported_without_stopping_fire() {
    let outcome = run_script(
        r#"
        local signal = import("Signal").new()
        signal:BindHandler("broken", function() error("handler failed") end)
        signal:BindHandler("working", function() worked = true end)
        signal:Fire()
        fired = true
        "#,
    )
    .await;
    assert!(outcome.global::<bool>("worked"));
    assert!(outcome.global::<bool>("fired"));
    assert_eq!(outcome.errors.len(), 1, "{:#?}", outcome.errors);
    assert!(outcome.errors[0].contains("handler failed"), "{}", outcome.errors[0]);
}

#[tokio::test]
async fn destroy_releases_handlers_and_waiters() {
    let outcome = run_script(
        r#"
        local signal = import("Signal").new()
        calls = 0
        signal:BindHandler("first", function()
            calls += 1
            signal:Destroy()
        end)
        signal:BindHandler("second", function() calls += 1 end)

        coroutine.wrap(function()
            waitOk = pcall(signal.Wait, signal)
        end)()

        signal:Fire()
        assert(not signal:IsBound("first"))
        assert(signal:UnBind("second") == false)

        local ok, err = pcall(signal.BindHandler, signal, "again", function() end)
        assert(not ok and string.find(tostring(err), "Signal 'Signal' has been destroyed", 1, true))
        assert(not pcall(signal.Fire, signal))
        assert(not pcall(signal.Invoke, signal, "first"))
        "#,
    )
    .await;
    outcome.assert_clean();
    assert_eq!(outcome.global::<i64>("calls"), 1);
    assert!(outcome.global::<bool>("waitOk"));
}

#[tokio::test]
async fn destroying_a_signal_wakes_its_waiters_with_an_error() {
    let outcome = run_script(
        r#"
        local signal = import("Signal").new()
        coroutine.wrap(function()
            local ok, err = pcall(signal.Wait, signal)
            waitError = tostring(err)
        end)()
        signal:Destroy()
        "#,
    )
    .await;
    outcome.assert_clean();
    let wait_error: String = outcome.global("waitError");
    assert!(wait_error.contains("was destroyed while it was being waited on"), "{wait_error}");
}

#[test]
fn runaway_signal_recursion_is_reported_instead_of_crashing() {
    let outcome = std::thread::Builder::new()
        .stack_size(luv::runtime::THREAD_STACK_SIZE)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let outcome = run_script(
                        r#"
                        local signal = import("Signal").new()
                        depth = 0
                        signal:BindHandler("recurse", function()
                            depth += 1
                            signal:Fire()
                        end)
                        signal:Fire()
                        "#,
                    )
                    .await;
                    (outcome.errors.clone(), outcome.global::<i64>("depth"))
                })
        })
        .unwrap()
        .join()
        .unwrap();
    let (errors, depth) = outcome;
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].contains("C stack overflow"), "{}", errors[0]);
    assert!(depth > 50, "{depth}");
}

#[tokio::test]
async fn rust_can_bind_fire_and_invoke() {
    let dir = main_script("return nil\n");
    let runtime = Runtime::new(Engine::new(Arc::new(DirVfs::new(dir.path())))).unwrap();
    let lua = runtime.lua();

    let signal = lua.create_userdata(Signal::new()).unwrap();
    let total: Function = lua
        .load("local total = 0 return function(amount) total += amount return total end")
        .eval()
        .unwrap();
    signal.borrow_mut::<Signal>().unwrap().bind_handler("total", total).unwrap();

    Signal::fire(lua, &signal, 5.into_lua_multi(lua).unwrap()).unwrap();
    let result = Signal::invoke(&signal, "total", 2.into_lua_multi(lua).unwrap())
        .await
        .unwrap();
    assert_eq!(result.into_iter().next().unwrap().as_i64(), Some(7));

    assert_eq!(signal.borrow::<Signal>().unwrap().handler_ids(), ["total"]);
    signal.borrow_mut::<Signal>().unwrap().destroy();
    assert!(signal.borrow::<Signal>().unwrap().handler_ids().is_empty());
}
