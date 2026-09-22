mod common;

use common::{main_script, run_both, workspace};
use luv::script::{HOOK, split};
use mlua::Table;

fn captures(source: &str) -> Vec<Vec<String>> {
    split(source)
        .unwrap()
        .clusters
        .into_iter()
        .map(|cluster| cluster.captures)
        .collect()
}

fn error(source: &str) -> (usize, String) {
    let error = split(source).unwrap_err();
    (error.line, error.message)
}

#[test]
fn leaves_scripts_without_parallel_blocks_alone() {
    let source = "local x = 1\nprint(x)\n";
    let units = split(source).unwrap();
    assert_eq!(units.main, source);
    assert!(units.clusters.is_empty());
}

#[test]
fn splits_blocks_into_clusters_and_keeps_line_numbers() {
    let source = "local a = 1\nprint(a)\nEnterParallel()\nlocal b = a + 1\nprint(b)\nExitParallel()\nprint(\"after\")\n";
    let units = split(source).unwrap();

    let main: Vec<&str> = units.main.lines().collect();
    assert_eq!(main.len(), source.lines().count());
    assert_eq!(main[2], format!("{HOOK}(1, \"a\", a)"));
    assert_eq!(main[3], "");
    assert_eq!(main[6], "print(\"after\")");

    assert_eq!(units.clusters.len(), 1);
    let cluster = &units.clusters[0];
    assert_eq!(cluster.line, 3);
    let lines: Vec<&str> = cluster.source.lines().collect();
    assert_eq!(lines[0], "local a = ...;");
    assert_eq!(lines[3], "local b = a + 1");
    assert_eq!(lines[4], "print(b)");
    assert!(!cluster.source.contains("print(\"after\")"));
}

#[test]
fn numbers_every_block_in_order() {
    let source = "EnterParallel()\nprint(1)\nExitParallel()\nEnterParallel(); print(2); ExitParallel();\n";
    let units = split(source).unwrap();
    assert_eq!(units.clusters.len(), 2);
    assert!(units.main.contains(&format!("{HOOK}(1, \"\")")));
    assert!(units.main.contains(&format!("{HOOK}(2, \"\")")));
    assert_eq!(units.clusters[1].source, "\n\n\n print(2); ");
    assert_eq!(units.clusters[1].line, 4);
}

#[test]
fn captures_outer_locals_used_inside_the_block() {
    let source = r#"
local a, b, c = 1, 2, 3
local function helper() end
EnterParallel()
local d = a + c
print(d, missing)
ExitParallel()
"#;
    assert_eq!(captures(source), [["a", "c"]]);
}

#[test]
fn captures_through_nested_functions() {
    let source = r#"
local speed = 5
EnterParallel()
local function move(distance)
    return distance * speed
end
print(move(2))
ExitParallel()
"#;
    assert_eq!(captures(source), [["speed"]]);
}

#[test]
fn ignores_globals_parameters_and_block_locals() {
    let source = r#"
local list = {}
EnterParallel()
local total = 0
for index, value in ipairs({}) do
    total += index + value
end
for i = 1, 10 do
    total += i
end
local function add(list, amount)
    return #list + amount
end
print(total, add({}, 1), math.pi)
ExitParallel()
"#;
    assert_eq!(captures(source), [Vec::<String>::new()]);
}

#[test]
fn resolves_shadowing_with_block_scopes() {
    let source = r#"
local x = 1
EnterParallel()
do
    local x = 2
end
print(x)
ExitParallel()
"#;
    assert_eq!(captures(source), [["x"]]);
}

#[test]
fn locals_declared_before_use_in_the_block_are_not_captured() {
    let source = r#"
local x = 1
EnterParallel()
local x = 2
print(x)
ExitParallel()
"#;
    assert_eq!(captures(source), [Vec::<String>::new()]);
}

#[test]
fn repeat_conditions_see_the_loop_body() {
    let source = r#"
local done = false
EnterParallel()
repeat
    local done = true
until done
ExitParallel()
"#;
    assert_eq!(captures(source), [Vec::<String>::new()]);
}

#[test]
fn if_branches_do_not_leak_locals() {
    let source = r#"
local flag = true
EnterParallel()
if flag then
    local value = 1
else
    print(value)
end
ExitParallel()
"#;
    assert_eq!(captures(source), [["flag"]]);
}

#[test]
fn for_loop_limits_are_evaluated_outside_the_loop() {
    let source = r#"
local i = 10
EnterParallel()
for i = 1, i do
    print(i)
end
ExitParallel()
"#;
    assert_eq!(captures(source), [["i"]]);
}

#[test]
fn types_are_not_captures() {
    let source = r#"
local Point = { x = 0 }
EnterParallel()
local p: typeof(Point) = { x = 1 }
print(p)
ExitParallel()
"#;
    assert_eq!(captures(source), [Vec::<String>::new()]);
}

#[test]
fn blocks_inside_functions_capture_parameters() {
    let source = r#"
local function start(count, label)
    EnterParallel()
    print(count)
    ExitParallel()
end
start(1, "x")
"#;
    assert_eq!(captures(source), [["count"]]);
}

#[test]
fn reports_unbalanced_markers() {
    assert_eq!(
        error("EnterParallel()\nprint(1)\n"),
        (1, "EnterParallel() has no matching ExitParallel() in the same block".to_owned())
    );
    assert_eq!(
        error("print(1)\nExitParallel()\n"),
        (2, "ExitParallel() has no matching EnterParallel() in the same block".to_owned())
    );
    assert_eq!(
        error("EnterParallel()\ndo\nExitParallel()\nend\n").1,
        "EnterParallel() has no matching ExitParallel() in the same block"
    );
}

#[test]
fn reports_nested_blocks() {
    assert_eq!(
        error("EnterParallel()\nEnterParallel()\nExitParallel()\nExitParallel()\n"),
        (2, "EnterParallel() cannot be nested, call ExitParallel() first".to_owned())
    );
    assert_eq!(
        error("EnterParallel()\nlocal function f()\nEnterParallel()\nExitParallel()\nend\nExitParallel()\n"),
        (3, "EnterParallel() cannot be used inside another parallel block".to_owned())
    );
}

#[test]
fn reports_misused_markers() {
    assert_eq!(
        error("EnterParallel(1)\nExitParallel()\n"),
        (1, "EnterParallel() does not take any arguments".to_owned())
    );
    assert_eq!(
        error("local x = EnterParallel()\n"),
        (1, "EnterParallel() must be called on its own as a statement".to_owned())
    );
    assert_eq!(
        error("local f = ExitParallel\n"),
        (1, "ExitParallel() must be called on its own as a statement".to_owned())
    );
}

#[test]
fn reports_varargs_used_directly_in_a_block() {
    assert_eq!(
        error("EnterParallel()\nprint(...)\nExitParallel()\n").1,
        "`...` cannot be used directly inside a parallel block, store it in a local before EnterParallel()"
    );
    assert!(split("EnterParallel()\nlocal function f(...) return ... end\nExitParallel()\n").is_ok());
}

#[tokio::test]
async fn runs_parallel_blocks_on_their_own_threads() {
    let dir = main_script(
        r#"
local Messenger = import("Messenger")
local limit = 1000

Messenger:Subscribe("Result", function(total, thread)
    result = { total = total, thread = thread }
end)

EnterParallel()
local total = 0
for i = 1, limit do
    total += i
end
Messenger:Fire("Result", total, threadName())
ExitParallel()

mainThread = threadName()
"#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let result: Table = outcome.global("result");
        assert_eq!(result.get::<i64>("total").unwrap(), 500500);
        let thread: String = result.get("thread").unwrap();
        assert_eq!(thread, "parallel block #1 of src/main.luau");
        assert_ne!(outcome.global::<String>("mainThread"), thread);
    }
}

#[tokio::test]
async fn parallel_threads_get_the_same_globals() {
    let dir = workspace(&[
        ("src/util.luau", "return { value = 7 }\n"),
        (
            "src/spawner.luau",
            r#"
local Messenger = import("Messenger")
return function()
    EnterParallel()
    Messenger:Fire("Nested", threadName())
    ExitParallel()
end
"#,
        ),
        (
            "src/main.luau",
            r#"
local Messenger = import("Messenger")
Messenger:Subscribe("Globals", function(found)
    globals = found
end)
Messenger:Subscribe("Nested", function(thread)
    nested = thread
end)

EnterParallel()
local Messenger = import("Messenger")
local Signal = import("Signal")
local signal = Signal.new()
local fired = false
signal:BindHandler("check", function() fired = true end)
signal:Fire()
Messenger:Fire("Globals", {
    import = typeof(import),
    require = typeof(require),
    util = require("./util").value,
    fired = fired,
})
require("./spawner")()
ExitParallel()
"#,
        ),
    ]);
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let globals: Table = outcome.global("globals");
        assert_eq!(globals.get::<String>("import").unwrap(), "function");
        assert_eq!(globals.get::<String>("require").unwrap(), "function");
        assert_eq!(globals.get::<i64>("util").unwrap(), 7);
        assert!(globals.get::<bool>("fired").unwrap());
        assert_eq!(outcome.global::<String>("nested"), "parallel block #1 of src/spawner.luau");
    }
}

#[tokio::test]
async fn captured_values_are_copied_into_the_thread() {
    let dir = main_script(
        r#"
local Messenger = import("Messenger")
local settings = { speed = 5, tags = { "a", "b" } }
local data = buffer.create(1)
buffer.writeu8(data, 0, 9)
local origin = vector.create(1, 2, 3)
local enabled, label = true, "worker"

Messenger:Subscribe("Copied", function(speed, tag, byte, x, flag, text)
    copied = { speed = speed, tag = tag, byte = byte, x = x, flag = flag, text = text }
end)

EnterParallel()
settings.speed = 99
Messenger:Fire("Copied", settings.speed, settings.tags[2], buffer.readu8(data, 0), origin.x, enabled, label)
ExitParallel()

settingsSpeed = settings.speed
"#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        assert_eq!(outcome.global::<i64>("settingsSpeed"), 5);
        let copied: Table = outcome.global("copied");
        assert_eq!(copied.get::<i64>("speed").unwrap(), 99);
        assert_eq!(copied.get::<String>("tag").unwrap(), "b");
        assert_eq!(copied.get::<i64>("byte").unwrap(), 9);
        assert_eq!(copied.get::<f64>("x").unwrap(), 1.0);
        assert!(copied.get::<bool>("flag").unwrap());
        assert_eq!(copied.get::<String>("text").unwrap(), "worker");
    }
}

#[tokio::test]
async fn objects_cannot_be_captured() {
    let dir = main_script(
        r#"
local signal = import("Signal").new()
EnterParallel()
signal:Fire()
ExitParallel()
"#,
    );
    for outcome in run_both(dir.path()).await {
        assert_eq!(outcome.errors.len(), 1, "{:#?}", outcome.errors);
        let message = &outcome.errors[0];
        assert!(message.contains("cannot pass `signal` into parallel block #1"), "{message}");
        assert!(message.contains("objects cannot be sent between threads"), "{message}");
        assert!(message.contains("src/main.luau:3"), "{message}");
    }
}

#[tokio::test]
async fn workers_keep_running_while_they_are_subscribed() {
    let dir = main_script(
        r#"
local Messenger = import("Messenger")
results = {}
Messenger:Subscribe("Done", function(job, answer)
    results[job] = answer
end)

EnterParallel()
Messenger:Subscribe("Job", function(job, value)
    Messenger:Fire("Done", job, value * value)
end)
ExitParallel()

for job = 1, 3 do
    Messenger:Fire("Job", job, job + 1)
end
"#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let results: Vec<i64> = outcome.global("results");
        assert_eq!(results, [4, 9, 16]);
    }
}

#[tokio::test]
async fn parallel_blocks_can_wait_for_messages() {
    let dir = main_script(
        r#"
local Messenger = import("Messenger")

EnterParallel()
local first = Messenger:Wait("Numbers")
local second = Messenger:Wait("Numbers")
Messenger:Fire("Sum", first + second)
ExitParallel()

Messenger:Fire("Numbers", 20)
Messenger:Fire("Numbers", 22)
sum = Messenger:Wait("Sum")
"#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        assert_eq!(outcome.global::<i64>("sum"), 42);
    }
}

#[tokio::test]
async fn parallel_blocks_run_from_required_modules() {
    let dir = workspace(&[
        (
            "src/jobs/worker.luau",
            r#"
local Messenger = import("Messenger")
return function(seed)
    EnterParallel()
    Messenger:Fire("Worker", seed * 2, threadName())
    ExitParallel()
end
"#,
        ),
        (
            "src/main.luau",
            r#"
local Messenger = import("Messenger")
Messenger:Subscribe("Worker", function(value, thread)
    worker = { value = value, thread = thread }
end)
require("./jobs/worker")(21)
"#,
        ),
    ]);
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let worker: Table = outcome.global("worker");
        assert_eq!(worker.get::<i64>("value").unwrap(), 42);
        assert_eq!(
            worker.get::<String>("thread").unwrap(),
            "parallel block #1 of src/jobs/worker.luau"
        );
    }
}

#[tokio::test]
async fn errors_in_parallel_blocks_point_at_the_original_line() {
    let dir = main_script("local x = 1\n\nEnterParallel()\nlocal y = x + 1\nerror(`boom {y}`)\nExitParallel()\n");
    for outcome in run_both(dir.path()).await {
        assert_eq!(outcome.errors.len(), 1, "{:#?}", outcome.errors);
        let message = &outcome.errors[0];
        assert!(message.starts_with("[parallel block #1 of src/main.luau]"), "{message}");
        assert!(message.contains("src/main.luau:5: boom 2"), "{message}");
    }
}
