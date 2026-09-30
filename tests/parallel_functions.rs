mod common;

use common::{main_script, run_source, workspace};
use luv::script::{ClusterKind, FUNCTION_HOOK, HOOK, split};
use mlua::Table;

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

fn error(source: &str) -> (usize, String) {
    let error = split(source).unwrap_err();
    (error.line, error.message)
}

#[test]
fn task_markers_split_like_enter_parallel() {
    let source = "local a = 1\ntask.desynchronize()\nprint(a)\ntask.synchronize()\nprint(\"after\")\n";
    let units = split(source).unwrap();
    let main: Vec<&str> = units.main.lines().collect();
    assert_eq!(main.len(), source.lines().count());
    assert_eq!(main[1], format!("{HOOK}(1, \"a\", a)"));
    assert_eq!(main[4], "print(\"after\")");
    assert_eq!(units.clusters.len(), 1);
    assert_eq!(units.clusters[0].kind, ClusterKind::Block);
    assert_eq!(units.clusters[0].captures, ["a"]);
    assert_eq!(units.clusters[0].source.lines().nth(2), Some("print(a)"));
}

#[test]
fn task_markers_report_problems_with_their_own_names() {
    assert_eq!(
        error("task.desynchronize()\nprint(1)\n"),
        (1, "task.desynchronize() has no matching task.synchronize() in the same block".to_owned())
    );
    assert_eq!(
        error("task.synchronize()\n"),
        (1, "task.synchronize() has no matching task.desynchronize() in the same block".to_owned())
    );
    assert_eq!(
        error("task.desynchronize(1)\ntask.synchronize()\n"),
        (1, "task.desynchronize() does not take any arguments".to_owned())
    );
    assert_eq!(
        error("local x = task.desynchronize()\n"),
        (1, "task.desynchronize() must be called on its own as a statement".to_owned())
    );
    assert_eq!(
        error("task.desynchronize()\nprint(...)\ntask.synchronize()\n").1,
        "`...` cannot be used directly inside a parallel block, store it in a local before task.desynchronize()"
    );
    assert!(split("local task = {}\ntask.desynchronize()\n").is_ok());
}

#[test]
fn function_literals_become_units_with_their_captures() {
    let source = "local speed = 2\nlocal skip = 9\ntask.parallel(function(count)\n\treturn count * speed\nend, 5)\nsignal:BindParallel(\"move\", function(x)\n\tprint(x, speed, skip)\nend)\n";
    let units = split(source).unwrap();
    assert_eq!(units.clusters.len(), 2);
    let main: Vec<&str> = units.main.lines().collect();
    assert_eq!(main.len(), source.lines().count());
    assert_eq!(main[2], format!("task.parallel({FUNCTION_HOOK}(1, \"speed\", speed)"));
    assert_eq!(main[4], ", 5)");
    assert_eq!(main[5], format!("signal:BindParallel(\"move\", {FUNCTION_HOOK}(2, \"speed,skip\", speed, skip)"));

    let spawned = &units.clusters[0];
    assert_eq!(spawned.kind, ClusterKind::Function);
    assert_eq!(spawned.line, 3);
    assert_eq!(spawned.captures, ["speed"]);
    let lines: Vec<&str> = spawned.source.lines().collect();
    assert_eq!(lines[0], "local speed = ...;");
    assert_eq!(lines[2], "return function(count)");
    assert_eq!(lines[3], "\treturn count * speed");

    assert_eq!(units.clusters[1].captures, ["speed", "skip"]);
}

#[test]
fn only_literals_written_in_the_call_are_moved() {
    let units = split("local f = function() end\ntask.parallel(f)\nsignal:BindParallel(\"x\", f)\n").unwrap();
    assert!(units.clusters.is_empty());
    let units = split("local task = {}\ntask.parallel(function() end)\n").unwrap();
    assert!(units.clusters.is_empty(), "a local named task is not the task library");
}

#[test]
fn methods_capture_self_and_literals_nest() {
    let source = "local Enemy = {}\nfunction Enemy:Start()\n\ttask.parallel(function()\n\t\tlocal inner = self.name\n\t\ttask.parallel(function() print(inner) end)\n\tend)\nend\n";
    let units = split(source).unwrap();
    assert_eq!(units.clusters.len(), 2);
    assert_eq!(units.clusters[0].captures, ["self"]);
    assert_eq!(units.clusters[1].captures, ["inner"]);
    assert!(units.clusters[0].source.contains(&format!("{FUNCTION_HOOK}(2, \"inner\", inner)")));
    assert!(!units.main.contains("inner"));
}

#[tokio::test]
async fn task_parallel_runs_a_function_on_its_own_thread() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        local Thread = import("Thread")
        local factor = 3

        local worker = task.parallel(function(first, second)
            Thread.Set("Maths")
            Messenger:Fire("answer", (first + second) * factor, Thread.Running().IsMain)
        end, 4, 10)

        local answer, onMain = Messenger:Wait("answer")
        local ok, problem = pcall(function()
            local plain = function() end
            task.parallel(plain)
        end)
        result = {
            answer = answer,
            onMain = onMain,
            class = worker.ClassName,
            name = worker.Name,
            ok = ok,
            problem = tostring(problem),
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert_eq!(result.get::<i64>("answer").unwrap(), 42);
    assert!(!result.get::<bool>("onMain").unwrap());
    assert_eq!(result.get::<String>("class").unwrap(), "Thread");
    assert!(!result.get::<bool>("ok").unwrap());
    assert!(
        result.get::<String>("problem").unwrap().contains("task.parallel needs the function written inside the call"),
        "{}",
        result.get::<String>("problem").unwrap()
    );
}

#[tokio::test]
async fn task_parallel_runs_many_threads_at_once() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        local total = 0
        for index = 1, 8 do
            task.parallel(function(value)
                local Messenger = import("Messenger")
                local sum = 0
                for step = 1, 2000 do
                    sum += value
                end
                Messenger:Fire("sum", sum)
            end, index)
        end
        for _ = 1, 8 do
            total += Messenger:Wait("sum")
        end
        result = total
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: i64 = outcome.global("result");
    assert_eq!(result, 2000 * (1 + 2 + 3 + 4 + 5 + 6 + 7 + 8));
}

#[tokio::test]
async fn bind_parallel_handles_every_fire_on_one_worker() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        local Signal = import("Signal")
        local Thread = import("Thread")
        local moved = Signal.new()
        local scale = 10

        local worker = moved:BindParallel("mover", function(x, y)
            Messenger:Fire("moved", (x + y) * scale, Thread.Running().Id)
        end)

        local bound = moved:IsBound("mover")
        moved:Fire(1, 2)
        local first, firstThread = Messenger:Wait("moved")
        moved:Fire(3, 4)
        local second, secondThread = Messenger:Wait("moved")

        local invoked, invokeProblem = pcall(function()
            moved:Invoke("mover", 1, 1)
        end)
        local duplicate = pcall(function()
            moved:BindParallel("mover", function() end)
        end)
        local plain = pcall(function()
            local f = function() end
            moved:BindParallel("plain", f)
        end)

        local aliveBefore = worker.IsAlive
        local unbound = moved:UnBind("mover")
        local waited = 0
        while worker.IsAlive and waited < 2 do
            waited += task.wait(0.02)
        end

        result = {
            bound = bound,
            first = first,
            second = second,
            sameWorker = firstThread == secondThread and firstThread == worker.Id,
            invoked = invoked,
            invokeProblem = tostring(invokeProblem),
            duplicate = duplicate,
            plain = plain,
            aliveBefore = aliveBefore,
            unbound = unbound,
            stopped = not worker.IsAlive,
            stillBound = moved:IsBound("mover"),
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert!(result.get::<bool>("bound").unwrap());
    assert_eq!(result.get::<i64>("first").unwrap(), 30);
    assert_eq!(result.get::<i64>("second").unwrap(), 70);
    assert!(result.get::<bool>("sameWorker").unwrap());
    assert!(!result.get::<bool>("invoked").unwrap());
    assert!(result.get::<String>("invokeProblem").unwrap().contains("runs on its own thread"));
    assert!(!result.get::<bool>("duplicate").unwrap());
    assert!(!result.get::<bool>("plain").unwrap());
    assert!(result.get::<bool>("aliveBefore").unwrap());
    assert!(result.get::<bool>("unbound").unwrap());
    assert!(result.get::<bool>("stopped").unwrap(), "unbinding should end the worker");
    assert!(!result.get::<bool>("stillBound").unwrap());
}

#[tokio::test]
async fn a_bound_worker_does_not_keep_the_game_open() {
    let outcome = run_script(
        r#"
        local Signal = import("Signal")
        keep = Signal.new()
        keep:BindParallel("idle", function() end)
        finished = true
        "#,
    )
    .await;
    outcome.assert_clean();
    let finished: bool = outcome.global("finished");
    assert!(finished);
}

#[tokio::test]
async fn task_desynchronize_runs_the_block_on_another_thread() {
    let outcome = run_script(
        r#"
        local Messenger = import("Messenger")
        local base = 5
        task.desynchronize()
        local Messenger = import("Messenger")
        local Thread = import("Thread")
        Messenger:Fire("block", base * 2, Thread.Running().IsMain)
        task.synchronize()
        local value, onMain = Messenger:Wait("block")
        result = { value = value, onMain = onMain }
        "#,
    )
    .await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert_eq!(result.get::<i64>("value").unwrap(), 10);
    assert!(!result.get::<bool>("onMain").unwrap());
}

#[tokio::test]
async fn parallel_functions_can_use_imports_and_modules_from_the_top_of_the_file() {
    let dir = workspace(&[
        (
            "src/Maths.luau",
            r#"
            local Maths = {}
            Maths.loads = (Maths.loads or 0) + 1
            function Maths.square(value)
                return value * value
            end
            return Maths
            "#,
        ),
        (
            "src/main.luau",
            r#"
            local Messenger = import("Messenger")
            local Thread = import("Thread")
            local Maths = require("./Maths")
            local config = { offset = 1 }

            task.parallel(function(value)
                Messenger:Fire("result", Maths.square(value) + config.offset, Thread.Running().IsMain, Maths.loads)
            end, 6)

            local answer, onMain, loads = Messenger:Wait("result")
            result = { answer = answer, onMain = onMain, loads = loads, localLoads = Maths.loads }
            "#,
        ),
    ]);
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert_eq!(result.get::<i64>("answer").unwrap(), 37);
    assert!(!result.get::<bool>("onMain").unwrap());
    assert_eq!(result.get::<i64>("loads").unwrap(), 1, "the worker loads its own copy of the module once");
    assert_eq!(result.get::<i64>("localLoads").unwrap(), 1);
}
