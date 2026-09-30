mod common;

use common::{run_both, run_source, workspace};
use luv::script::{Compiled, capture_line, headers};
use mlua::Table;

#[test]
fn headers_are_read_from_the_top_of_a_script() {
    let found = headers(b"--!strict\n---@start\n---@bootready\n\nlocal x = 1\n---@boot\n");
    assert!(found.start);
    assert!(found.boot_ready);
    assert!(!found.boot, "a header after the first line of code does not count");
    assert!(!found.start_async);
    assert_eq!(found.capture, None);

    let found = headers(b"---@startasync\n---@capture[\"took %time% to complete\"]\n");
    assert!(found.start_async);
    assert!(!found.start, "startasync is not the same as start");
    assert_eq!(found.capture, Some(Some("took %time% to complete".to_owned())));
    assert_eq!(headers(b"---@capture\nprint(1)").capture, Some(None));
    assert!(headers(b"-- a comment\n---@boot\n").boot);
}

#[test]
fn capture_lines_fill_in_their_placeholders() {
    let compiled = Compiled {
        path: "src/Systems/Physics.luau",
        seconds: 0.2,
        chunks: 3,
        lines: 120,
        bytes: 4096,
    };
    assert_eq!(
        capture_line(Some("took %time% to complete"), &compiled),
        "Physics.luau finished compiling took 0.2 to complete"
    );
    assert_eq!(
        capture_line(Some("%chunks% chunks, %lines% lines, %size% bytes in %path%"), &compiled),
        "Physics.luau finished compiling 3 chunks, 120 lines, 4096 bytes in src/Systems/Physics.luau"
    );
    assert_eq!(
        capture_line(None, &compiled),
        "Physics.luau finished compiling in 0.2s with 3 parallel chunks"
    );
}

fn game() -> tempfile::TempDir {
    workspace(&[
        (
            "src/main.luau",
            r#"
            local Messenger = import("Messenger")
            log = log or {}
            table.insert(log, "main")
            local blocked, problem = pcall(require, "./Systems/Started")
            requireBlocked = not blocked and string.find(tostring(problem), "is started by luv on its own and cannot be required", 1, true) ~= nil
            local mainBlocked = pcall(require, "./main")
            mainRequireBlocked = not mainBlocked
            fromAsync = Messenger:Wait("async")
            task.wait(0.05)
            "#,
        ),
        (
            "src/Systems/Booted.luau",
            "---@boot\nbootCount = (bootCount or 0) + 1\nlog = log or {}\ntable.insert(log, \"boot\")\nbootedHere = true\n",
        ),
        (
            "src/Systems/Started.luau",
            "---@start\nlog = log or {}\ntable.insert(log, \"start\")\n",
        ),
        (
            "src/Systems/Worker.luau",
            "---@startasync\nlocal Messenger = import(\"Messenger\")\nlocal Thread = import(\"Thread\")\nThread.Running():MarkReady()\nMessenger:Fire(\"async\", bootedHere == true, Thread.Running().Name, readyRan == true)\n",
        ),
        (
            "src/Systems/Ready.luau",
            "---@bootready\nreadyRan = true\n",
        ),
    ])
}

#[tokio::test]
async fn header_scripts_boot_on_their_own_threads() {
    let dir = game();
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let log: Vec<String> = outcome.global("log");
        assert_eq!(log, ["boot", "start", "main"]);
        assert!(outcome.global::<bool>("requireBlocked"));
        assert!(outcome.global::<bool>("mainRequireBlocked"));
        assert_eq!(outcome.global::<i64>("bootCount"), 1);
        let from: Table = outcome.runtime.lua().load("return { fromAsync }").eval().unwrap();
        assert!(from.get::<bool>(1).unwrap(), "boot scripts run in the async thread too");
        assert_eq!(outcome.global::<Option<bool>>("readyRan"), None, "the main thread never marked itself ready");
    }
}

#[tokio::test]
async fn startasync_threads_run_bootready_when_they_mark_ready() {
    let dir = workspace(&[
        (
            "src/main.luau",
            r#"
            local Messenger = import("Messenger")
            local fromBoot, name, ready = Messenger:Wait("async")
            result = { fromBoot = fromBoot, name = name, ready = ready }
            "#,
        ),
        (
            "src/Worker.luau",
            "---@startasync\nlocal Messenger = import(\"Messenger\")\nlocal Thread = import(\"Thread\")\nThread.Running():MarkReady()\nMessenger:Fire(\"async\", bootedHere == true, Thread.Running().Name, readyRan == true)\n",
        ),
        ("src/Booted.luau", "---@boot\nbootedHere = true\n"),
        ("src/Ready.luau", "---@bootready\nreadyRan = true\n"),
    ]);
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    let result: Table = outcome.global("result");
    assert!(result.get::<bool>("fromBoot").unwrap());
    assert_eq!(result.get::<String>("name").unwrap(), "src/Worker.luau");
    assert!(result.get::<bool>("ready").unwrap(), "MarkReady should run the bootready scripts first");
}

#[tokio::test]
async fn boot_scripts_run_in_every_new_thread() {
    let dir = workspace(&[
        (
            "src/main.luau",
            r#"
            local Messenger = import("Messenger")
            task.parallel(function()
                Messenger:Fire("seen", sharedSetting)
            end)
            seenInParallel = Messenger:Wait("seen")
            "#,
        ),
        ("src/Settings.luau", "---@boot\nsharedSetting = \"from boot\"\n"),
    ]);
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    assert_eq!(outcome.global::<String>("seenInParallel"), "from boot");
    assert_eq!(outcome.global::<String>("sharedSetting"), "from boot");
}

#[tokio::test]
async fn building_reports_stages_and_captured_scripts() {
    let dir = workspace(&[
        ("src/main.luau", "print(\"main\")\n"),
        (
            "src/Systems/Physics.luau",
            "---@capture[\"took %time% with %chunks% chunks\"]\nlocal Messenger = import(\"Messenger\")\ntask.parallel(function() Messenger:Fire(\"x\") end)\nreturn {}\n",
        ),
        ("src/Systems/Quiet.luau", "---@capture\nreturn {}\n"),
        ("assets/data.txt", "hello"),
    ]);
    let project = luv::project::Project::load(dir.path()).unwrap();
    let progress = luv::progress::Progress::quiet();
    luv::builder::build_with(&project, &progress).await.unwrap();
    let lines = progress.lines();
    assert!(lines.contains(&"Scripts complete (3)".to_owned()), "{lines:?}");
    assert!(lines.contains(&"Assets complete (1)".to_owned()), "{lines:?}");
    assert_eq!(lines.last().map(String::as_str), Some("Package written"));
    let physics = lines
        .iter()
        .find(|line| line.starts_with("Physics.luau finished compiling took "))
        .unwrap_or_else(|| panic!("no capture line in {lines:?}"));
    assert!(physics.ends_with(" with 1 chunks"), "{physics}");
    assert!(
        lines.iter().any(|line| line.starts_with("Quiet.luau finished compiling in ") && line.ends_with("with 0 parallel chunks")),
        "{lines:?}"
    );
}
