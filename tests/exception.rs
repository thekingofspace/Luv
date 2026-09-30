mod common;

use common::{main_script, run_source};
use mlua::Table;

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[tokio::test]
async fn an_uncaught_error_fires_raised_with_where_it_was_thrown() {
    let outcome = run_script(
        r#"
        local Exception = import("Exception")
        seen = nil
        Exception.Raised:BindHandler("log", function(exception)
            seen = exception
        end)

        local parts = {}
        function parts.explode()
            error("boom")
        end

        task.spawn(function()
            parts.explode()
        end)
        task.wait(0.05)

        result = {
            message = seen.Message,
            source = seen.Source,
            line = seen.Line,
            thread = seen.Thread,
            caught = seen.Caught,
            top = seen.Stack[1].Name,
            topLine = seen.Stack[1].Line,
            frames = #seen.Stack,
            traceback = seen.Traceback,
            count = Exception.Count,
            recent = #Exception.GetRecent(),
            frozen = table.isfrozen(seen),
        }
        "#,
    )
    .await;
    assert_eq!(outcome.errors.len(), 1, "the error should still be reported: {:?}", outcome.errors);
    assert!(outcome.errors[0].contains("boom"));
    let result: Table = outcome.global("result");
    let text = |key: &str| result.get::<String>(key).unwrap();
    assert_eq!(text("message"), "src/main.luau:10: boom");
    assert_eq!(text("source"), "src/main.luau");
    assert_eq!(result.get::<i64>("line").unwrap(), 10);
    assert_eq!(text("thread"), "main");
    assert!(!result.get::<bool>("caught").unwrap());
    assert_eq!(text("top"), "explode");
    assert_eq!(result.get::<i64>("topLine").unwrap(), 10);
    assert!(result.get::<i64>("frames").unwrap() >= 2);
    assert!(text("traceback").contains("at src/main.luau:10 in explode"), "{}", text("traceback"));
    assert_eq!(result.get::<i64>("count").unwrap(), 1);
    assert_eq!(result.get::<i64>("recent").unwrap(), 1);
    assert!(result.get::<bool>("frozen").unwrap());
}

#[tokio::test]
async fn epcall_is_pcall_that_also_reports() {
    let outcome = run_script(
        r#"
        local Exception = import("Exception")
        seen = {}
        Exception.Raised:BindHandler("log", function(exception)
            table.insert(seen, exception)
        end)

        local parts = {}
        function parts.inner()
            error({ code = 7 })
        end
        function parts.outer()
            parts.inner()
        end
        local outer = parts.outer

        local ok, problem = epcall(outer)
        local fine, a, b = epcall(function(x, y)
            return x + y, "done"
        end, 2, 3)
        local plain, plainProblem = pcall(outer)
        task.wait(0.05)

        result = {
            ok = ok,
            code = problem.code,
            fine = fine,
            a = a,
            b = b,
            plain = plain,
            plainCode = plainProblem.code,
            fired = #seen,
            caught = seen[1].Caught,
            top = seen[1].Stack[1].Name,
            second = seen[1].Stack[2].Name,
            valueCode = seen[1].Value.code,
            line = seen[1].Line,
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert!(!result.get::<bool>("ok").unwrap());
    assert_eq!(result.get::<i64>("code").unwrap(), 7, "epcall hands back the error value untouched");
    assert!(result.get::<bool>("fine").unwrap());
    assert_eq!(result.get::<i64>("a").unwrap(), 5);
    assert_eq!(result.get::<String>("b").unwrap(), "done");
    assert!(!result.get::<bool>("plain").unwrap());
    assert_eq!(result.get::<i64>("plainCode").unwrap(), 7);
    assert_eq!(result.get::<i64>("fired").unwrap(), 1, "only epcall reports, plain pcall stays silent");
    assert!(result.get::<bool>("caught").unwrap());
    assert_eq!(result.get::<String>("top").unwrap(), "inner");
    assert_eq!(result.get::<String>("second").unwrap(), "outer");
    assert_eq!(result.get::<i64>("valueCode").unwrap(), 7);
    assert_eq!(result.get::<i64>("line").unwrap(), 10);
}

#[tokio::test]
async fn epcall_works_across_yields() {
    let outcome = run_script(
        r#"
        local ok, value = epcall(function()
            task.wait(0.01)
            return "after the wait"
        end)
        local failed, problem = epcall(function()
            task.wait(0.01)
            error("late failure", 0)
        end)
        result = { ok = ok, value = value, failed = failed, problem = problem }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert!(result.get::<bool>("ok").unwrap());
    assert_eq!(result.get::<String>("value").unwrap(), "after the wait");
    assert!(!result.get::<bool>("failed").unwrap());
    assert_eq!(result.get::<String>("problem").unwrap(), "late failure");
}

#[tokio::test]
async fn try_hands_back_an_exception_without_reporting_it() {
    let outcome = run_script(
        r#"
        local Exception = import("Exception")
        local fired = 0
        Exception.Raised:BindHandler("count", function()
            fired += 1
        end)

        local parts = {}
        function parts.deep()
            local nothing = nil
            return nothing.field
        end

        local ok, exception = Exception.Try(parts.deep)
        local fine, value = Exception.Try(function(x)
            return x * 2
        end, 21)
        task.wait(0.05)

        result = {
            ok = ok,
            top = exception.Stack[1].Name,
            line = exception.Line,
            caught = exception.Caught,
            message = exception.Message,
            fine = fine,
            value = value,
            fired = fired,
            count = Exception.Count,
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert!(!result.get::<bool>("ok").unwrap());
    assert_eq!(result.get::<String>("top").unwrap(), "deep");
    assert_eq!(result.get::<i64>("line").unwrap(), 11);
    assert!(result.get::<bool>("caught").unwrap());
    assert!(result.get::<String>("message").unwrap().contains("attempt to index nil"));
    assert!(result.get::<bool>("fine").unwrap());
    assert_eq!(result.get::<i64>("value").unwrap(), 42);
    assert_eq!(result.get::<i64>("fired").unwrap(), 0);
    assert_eq!(result.get::<i64>("count").unwrap(), 0);
}

#[tokio::test]
async fn stack_tools_describe_the_running_code() {
    let outcome = run_script(
        r#"
        local Exception = import("Exception")

        local parts = {}
        function parts.whoCalled()
            return Exception.Caller()
        end

        function parts.leaf()
            local stack, trace = Exception.GetStack(), Exception.Traceback("here")
            return stack, trace
        end

        function parts.branch()
            local stack, trace = parts.leaf()
            return stack, trace
        end

        function parts.named()
            local frame = parts.whoCalled()
            return frame
        end

        local stack, trace = parts.branch()
        local caller = parts.named()

        result = {
            first = stack[1].Name,
            firstLine = stack[1].Line,
            second = stack[2].Name,
            source = stack[1].Source,
            native = stack[1].IsNative,
            trace = trace,
            caller = caller.Name,
            callerLine = caller.Line,
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    let text = |key: &str| result.get::<String>(key).unwrap();
    assert_eq!(text("first"), "leaf");
    assert_eq!(result.get::<i64>("firstLine").unwrap(), 10);
    assert_eq!(text("second"), "branch");
    assert_eq!(text("source"), "src/main.luau");
    assert!(!result.get::<bool>("native").unwrap());
    assert!(text("trace").starts_with("here\n  at src/main.luau:10 in leaf"), "{}", text("trace"));
    assert_eq!(text("caller"), "named");
    assert_eq!(result.get::<i64>("callerLine").unwrap(), 20);
}

#[tokio::test]
async fn errors_in_parallel_blocks_reach_the_main_thread() {
    let outcome = run_script(
        r#"
        local Exception = import("Exception")
        seen = nil
        Exception.Raised:BindHandler("log", function(exception)
            seen = exception
        end)

        EnterParallel()
        local parts = {}
        function parts.fromWorker()
            error("worker failed")
        end
        parts.fromWorker()
        ExitParallel()

        local waited = 0
        while seen == nil and waited < 2 do
            waited += task.wait(0.02)
        end
        result = {
            arrived = seen ~= nil,
            message = seen and seen.Message,
            thread = seen and seen.Thread,
            top = seen and seen.Stack[1] and seen.Stack[1].Name,
            line = seen and seen.Line,
        }
        "#,
    )
    .await;
    assert_eq!(outcome.errors.len(), 1, "{:?}", outcome.errors);
    let result: Table = outcome.global("result");
    assert!(result.get::<bool>("arrived").unwrap(), "the error never reached the main thread");
    let text = |key: &str| result.get::<String>(key).unwrap();
    assert_eq!(text("message"), "src/main.luau:11: worker failed");
    assert!(text("thread").starts_with("parallel block #1"), "{}", text("thread"));
    assert_eq!(text("top"), "fromWorker");
    assert_eq!(result.get::<i64>("line").unwrap(), 11);
}

#[tokio::test]
async fn a_raised_handler_that_errors_does_not_loop() {
    let outcome = run_script(
        r#"
        local Exception = import("Exception")
        calls = 0
        Exception.Raised:BindHandler("broken", function()
            calls += 1
            error("the handler broke")
        end)
        task.spawn(function()
            error("first")
        end)
        task.wait(0.1)
        "#,
    )
    .await;
    let calls: i64 = outcome.global("calls");
    assert_eq!(calls, 1, "the handler should run once, not for its own error");
    assert_eq!(outcome.errors.len(), 2, "{:?}", outcome.errors);
    assert!(outcome.errors.iter().any(|error| error.contains("the handler broke")));
}
