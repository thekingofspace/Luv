mod common;

use common::{main_script, run_source};

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[tokio::test]
async fn a_promise_runs_its_body_on_a_coroutine_and_hands_back_what_it_resolved() {
    let outcome = run_script(
        r#"
        order = {}
        local handle = promise.new(function(resolve, reject, first, second)
            table.insert(order, "body")
            sleep(10)
            resolve(first + second, "extra")
        end, 2, 3)

        table.insert(order, "caller")
        assert(promise.is(handle), "promise.is should know its own")
        assert(handle.Status == "pending", "it should still be pending")

        local sum, extra = handle:Await()
        assert(sum == 5, "the resolved sum should come back")
        assert(extra == "extra", "every resolved value should come back")
        assert(handle.Status == "resolved", "it should be resolved now")
        table.insert(order, "awaited")
        "#,
    )
    .await;
    outcome.assert_clean();
    let order: Vec<String> = outcome.global("order");
    assert_eq!(order, ["body", "caller", "awaited"]);
}

#[tokio::test]
async fn a_rejected_promise_reaches_catch_and_raises_from_await() {
    let outcome = run_script(
        r#"
        caught = nil
        local handle = promise.new(function(resolve, reject)
            reject("nope")
        end)
        handle:Catch(function(reason)
            caught = reason
        end)
        assert(handle.Status == "rejected", "it should be rejected")

        task.wait()
        assert(caught == "nope", "Catch should see the reason, got " .. tostring(caught))

        local ok, problem = pcall(function()
            return handle:Await()
        end)
        assert(not ok, "Await should raise on a rejected promise")
        assert(string.find(tostring(problem), "nope", 1, true) ~= nil, "the reason should be in the error")

        local status, reason = handle:AwaitStatus()
        assert(status == "rejected", "AwaitStatus should report rejected")
        assert(reason == "nope", "AwaitStatus should hand back the reason")
        "#,
    )
    .await;
    outcome.assert_clean();
}

#[tokio::test]
async fn an_error_inside_a_promise_body_rejects_it() {
    let outcome = run_script(
        r#"
        local broken = promise.new(function()
            error("split")
        end)
        local status, reason = broken:AwaitStatus()
        assert(status == "rejected", "an error should reject, got " .. status)
        assert(string.find(reason, "split", 1, true) ~= nil, "the error text should be the reason")

        local thrown = promise.call(function()
            sleep(5)
            error("kaboom")
        end)
        local kind, why = thrown:AwaitStatus()
        assert(kind == "rejected", "promise.call should reject on an error")
        assert(string.find(why, "kaboom", 1, true) ~= nil, "the error text should be the reason")
        "#,
    )
    .await;
    outcome.assert_clean();
}

#[tokio::test]
async fn and_then_chains_and_adopts_a_promise_a_handler_returns() {
    let outcome = run_script(
        r#"
        local chained = promise.resolve(2)
            :AndThen(function(number)
                return number * 3
            end)
            :AndThen(function(number)
                return promise.delay(0.01, number + 1)
            end)
        assert(chained:Await() == 7, "each step should feed the next")

        local nested = promise.call(function()
            return promise.resolve("inner")
        end)
        assert(nested:Await() == "inner", "a returned promise should be adopted")

        local skipped = false
        local recovered = promise.reject("bad")
            :AndThen(function()
                skipped = true
            end)
            :Catch(function(reason)
                return reason .. " handled"
            end)
        assert(recovered:Await() == "bad handled", "Catch should recover the chain")
        assert(not skipped, "AndThen should be skipped on a rejection")
        "#,
    )
    .await;
    outcome.assert_clean();
}

#[tokio::test]
async fn finally_runs_on_every_outcome_and_passes_it_through() {
    let outcome = run_script(
        r#"
        seen = {}
        local kept = promise.resolve(9):Finally(function(status)
            table.insert(seen, status)
        end)
        assert(kept:Await() == 9, "Finally should pass the value through")

        local failed = promise.reject("bad"):Finally(function(status)
            table.insert(seen, status)
        end)
        local status, reason = failed:AwaitStatus()
        assert(status == "rejected" and reason == "bad", "Finally should pass the rejection through")
        "#,
    )
    .await;
    outcome.assert_clean();
    let seen: Vec<String> = outcome.global("seen");
    assert_eq!(seen, ["resolved", "rejected"]);
}

#[tokio::test]
async fn all_and_race_wait_on_a_list_of_promises() {
    let outcome = run_script(
        r#"
        local values = promise.all({ promise.delay(0.02, "a"), promise.resolve("b"), 42 }):Await()
        assert(values[1] == "a", "the order of the list should be kept")
        assert(values[2] == "b", "an already resolved promise should be kept in place")
        assert(values[3] == 42, "a plain value should be kept in place")

        local empty = promise.all({}):Await()
        assert(#empty == 0, "an empty list should resolve with an empty table")

        local status, reason = promise.all({ promise.delay(0.05, 1), promise.reject("boom") }):AwaitStatus()
        assert(status == "rejected", "one rejection should reject the whole thing")
        assert(reason == "boom", "the first rejection should be the reason")

        local first = promise.race({ promise.delay(0.05, "slow"), promise.delay(0.01, "fast") }):Await()
        assert(first == "fast", "race should settle with the first one, got " .. tostring(first))

        local ok = pcall(promise.race, {})
        assert(not ok, "race with nothing to race should fail")
        "#,
    )
    .await;
    outcome.assert_clean();
}

#[tokio::test]
async fn cancelling_a_promise_stops_its_handlers() {
    let outcome = run_script(
        r#"
        ran = false
        local handle = promise.new(function(resolve)
            sleep(30)
            resolve("late")
        end)
        handle:AndThen(function()
            ran = true
        end)
        assert(handle:Cancel() == true, "cancelling a pending promise should take")
        assert(handle.Status == "cancelled", "it should read as cancelled")
        assert(handle:Cancel() == false, "cancelling twice should do nothing")

        local status = handle:AwaitStatus()
        assert(status == "cancelled", "AwaitStatus should report cancelled")

        local ok, problem = pcall(function()
            return handle:Await()
        end)
        assert(not ok and string.find(tostring(problem), "cancelled", 1, true) ~= nil, "Await should raise")

        sleep(60)
        assert(not ran, "a cancelled promise should not run AndThen")
        "#,
    )
    .await;
    outcome.assert_clean();
}

#[tokio::test]
async fn a_switch_runs_the_matching_case_on_its_own_coroutine() {
    let outcome = run_script(
        r#"
        local here = coroutine.running()
        where = nil

        local pick = switch.new({
            greet = function(name)
                where = coroutine.running()
                return "hello " .. name
            end,
            pair = function(first, second)
                return second, first
            end,
            slow = function()
                sleep(20)
                return "done"
            end,
            broken = function()
                error("inside")
            end,
        })

        assert(pick("greet", "world") == "hello world", "the case should get the extra values")
        assert(where ~= here, "the case should run on a new coroutine")

        local left, right = pick("pair", 1, 2)
        assert(left == 2 and right == 1, "every returned value should come back")

        assert(pick("slow") == "done", "the caller should wait for a case that yields")

        local ok, problem = pcall(pick, "broken")
        assert(not ok, "an error in a case should reach the caller")
        assert(string.find(tostring(problem), "inside", 1, true) ~= nil, "the error should be the one raised")

        local missing, why = pcall(pick, "nothing")
        assert(not missing, "an unknown key should fail")
        assert(string.find(tostring(why), "no case for 'nothing'", 1, true) ~= nil, "it should name the key")
        "#,
    )
    .await;
    outcome.assert_clean();
}

#[tokio::test]
async fn a_switch_can_take_a_fallback_and_refuses_a_bad_table() {
    let outcome = run_script(
        r#"
        fell = nil
        local pick = switch.new({
            known = function()
                return "known"
            end,
        }, function(key, extra)
            fell = key .. ":" .. tostring(extra)
            return "fallback"
        end)

        assert(pick("known") == "known", "a known case should still win")
        assert(pick("other", 7) == "fallback", "the fallback should run")
        assert(fell == "other:7", "the fallback should get the key and the rest")

        local ok = pcall(switch.new, { [1] = function() end })
        assert(not ok, "a table with a number key should be refused")

        local fine = pcall(switch.new, { name = 5 })
        assert(not fine, "a case that is not a function should be refused")

        local empty = pcall(switch.new, {})
        assert(not empty, "a switch with no cases should be refused")
        "#,
    )
    .await;
    outcome.assert_clean();
}

#[tokio::test]
async fn task_spawn_runs_now_and_task_defer_runs_after() {
    let outcome = run_script(
        r#"
        order = {}
        table.insert(order, "start")
        task.defer(function()
            table.insert(order, "deferred")
        end)
        local thread = task.spawn(function(label)
            table.insert(order, label)
        end, "spawned")
        assert(type(thread) == "thread", "task.spawn should hand back the coroutine")
        table.insert(order, "finish")
        task.wait(0.02)
        "#,
    )
    .await;
    outcome.assert_clean();
    let order: Vec<String> = outcome.global("order");
    assert_eq!(order, ["start", "spawned", "finish", "deferred"]);
}

#[tokio::test]
async fn task_wait_only_holds_up_its_own_coroutine() {
    let outcome = run_script(
        r#"
        order = {}
        task.spawn(function()
            local slept = task.wait(0.05)
            assert(slept >= 0.04, "task.wait should report how long it waited, got " .. tostring(slept))
            table.insert(order, "slow")
        end)
        task.spawn(function()
            task.wait(0.01)
            table.insert(order, "quick")
        end)
        table.insert(order, "main")
        task.wait(0.1)
        "#,
    )
    .await;
    outcome.assert_clean();
    let order: Vec<String> = outcome.global("order");
    assert_eq!(order, ["main", "quick", "slow"]);
}

#[tokio::test]
async fn task_delay_runs_later_and_keeps_the_game_alive() {
    let outcome = run_script(
        r#"
        order = {}
        task.delay(0.05, function(label)
            table.insert(order, label)
        end, "late")
        task.delay(0.01, function()
            table.insert(order, "early")
        end)
        table.insert(order, "now")
        "#,
    )
    .await;
    outcome.assert_clean();
    let order: Vec<String> = outcome.global("order");
    assert_eq!(order, ["now", "early", "late"]);
}

#[tokio::test]
async fn task_create_makes_a_thread_you_can_run_again() {
    let outcome = run_script(
        r#"
        total = 0
        local worker = task.create(function(amount)
            sleep(20)
            total += amount
        end)
        assert(worker.Exclusive == false, "it should share by default")

        worker(1)
        worker(2)
        assert(worker.Running == 2, "both runs should be live, got " .. tostring(worker.Running))
        worker:Wait()
        assert(worker.Running == 0, "Wait should hold until they are done")
        assert(total == 3, "every run should have happened, got " .. tostring(total))

        local thread = worker(4)
        assert(type(thread) == "thread", "a run should hand back its coroutine")
        worker:Wait()
        assert(total == 7, "the same task should run again")
        "#,
    )
    .await;
    outcome.assert_clean();
}

#[tokio::test]
async fn an_exclusive_task_only_runs_one_at_a_time() {
    let outcome = run_script(
        r#"
        runs = 0
        local once = task.create(function()
            runs += 1
            sleep(30)
        end, true)
        assert(once.Exclusive == true, "it should say it is exclusive")

        local first = once()
        local second = once()
        assert(first ~= nil, "the first run should start")
        assert(second == nil, "the second run should be turned away")
        assert(once.Running == 1, "only one should be live")

        once:Wait()
        assert(runs == 1, "only one run should have happened")

        local third = once()
        assert(third ~= nil, "it should run again once the first is done")
        once:Wait()
        assert(runs == 2, "the second run should have happened")
        "#,
    )
    .await;
    outcome.assert_clean();
}
