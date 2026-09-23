mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{global_string, main_script, run_both_with, run_source, run_with};
use luv::project::Project;
use mlua::Table;

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[tokio::test]
async fn exposes_process_information() {
    let dir = main_script(
        r#"
        local Process = import("Process")
        info = {
            args = table.concat(Process.args, ","),
            os = Process.os,
            arch = Process.arch,
            pid = Process.pid,
            cwd = Process.cwd(),
            path = Process.env.PATH or Process.env.Path,
        }
        frozen = not pcall(function() Process.env.NEW = "x" end)
        "#,
    );
    let project = Project::load(dir.path()).unwrap();
    let outcome = run_with(Arc::new(project.source_vfs()), "src/main.luau", |builder| {
        builder.args(["one", "two"])
    })
    .await;
    outcome.assert_clean();
    let info: Table = outcome.global("info");
    assert_eq!(info.get::<String>("args").unwrap(), "one,two");
    assert_eq!(info.get::<String>("os").unwrap(), std::env::consts::OS);
    assert_eq!(info.get::<String>("arch").unwrap(), std::env::consts::ARCH);
    assert_eq!(info.get::<u32>("pid").unwrap(), std::process::id());
    assert!(!info.get::<String>("cwd").unwrap().is_empty());
    assert!(!info.get::<String>("path").unwrap().is_empty());
    assert!(outcome.global::<bool>("frozen"));
}

#[tokio::test]
async fn spawn_runs_commands_and_captures_output() {
    let outcome = run_script(
        r#"
        local Process = import("Process")
        local function clean(text)
            return (string.gsub(text, "\r", ""))
        end

        local hello = Process.spawn("echo hello", nil, { shell = true })
        assert(hello.ok and hello.code == 0, hello.stderr)
        assert(clean(hello.stdout) == "hello\n", hello.stdout)

        local failed = Process.spawn("exit 3", nil, { shell = true })
        assert(not failed.ok and failed.code == 3)

        local sorted = Process.spawn("sort", nil, { shell = true, stdin = "b\na\n" })
        assert(sorted.ok and clean(sorted.stdout) == "a\nb\n", sorted.stdout)

        local command = Process.os == "windows" and "echo %LUV_TEST%" or "echo $LUV_TEST"
        local env = Process.spawn(command, nil, { shell = true, env = { LUV_TEST = "works" } })
        assert(clean(env.stdout) == "works\n", env.stdout)

        local ok, message = pcall(Process.spawn, "luv-program-that-does-not-exist")
        assert(not ok and string.find(tostring(message), "cannot start", 1, true))
        done = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("done"));
}

#[tokio::test]
async fn spawn_honours_the_working_directory() {
    let scratch = tempfile::tempdir().unwrap();
    std::fs::write(scratch.path().join("marker.txt"), "x").unwrap();
    let dir = main_script(
        r#"
        local Process = import("Process")
        local listing = Process.spawn(Process.os == "windows" and "dir /b" or "ls", nil, { shell = true, cwd = scratch })
        found = string.find(listing.stdout, "marker.txt", 1, true) ~= nil
        "#,
    );
    let path = scratch.path().to_string_lossy().into_owned();
    for outcome in run_both_with(dir.path(), global_string("scratch", path.clone())).await {
        outcome.assert_clean();
        assert!(outcome.global::<bool>("found"));
    }
}

#[tokio::test]
async fn started_processes_stream_through_files() {
    let outcome = run_script(
        r#"
        local Process = import("Process")
        local child = Process.start("sort", nil, { shell = true })
        assert(child.ClassName == "Child")
        assert(type(child.Pid) == "number")
        child.Stdin:write("pear\n", "apple\n")
        child.Stdin:close()
        output = string.gsub(child.Stdout:read("a"), "\r", "")
        local status = child:Wait()
        ok = status.ok
        code = status.code
        "#,
    )
    .await;
    outcome.assert_clean();
    assert_eq!(outcome.global::<String>("output"), "apple\npear\n");
    assert!(outcome.global::<bool>("ok"));
    assert_eq!(outcome.global::<i64>("code"), 0);
}

#[tokio::test]
async fn started_processes_can_be_killed() {
    let started = Instant::now();
    let outcome = run_script(
        r#"
        local Process = import("Process")
        local windows = Process.os == "windows"
        local child = Process.start(windows and "ping" or "sleep", windows and { "-n", "30", "127.0.0.1" } or { "30" })
        child:Kill()
        local status = child:Wait()
        killed = not status.ok
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("killed"));
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[tokio::test]
async fn waiting_for_processes_only_suspends_the_calling_coroutine() {
    let outcome = run_script(
        r#"
        local Process = import("Process")
        log = {}
        coroutine.wrap(function()
            Process.spawn(Process.os == "windows" and "ping -n 2 127.0.0.1" or "sleep 1", nil, { shell = true })
            table.insert(log, "spawned")
        end)()
        table.insert(log, "main")
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["main", "spawned"]);
}

#[tokio::test]
async fn exit_stops_the_game_with_a_code() {
    let outcome = run_script(
        r#"
        local Process = import("Process")
        Process.exit(7)
        reached = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert_eq!(outcome.runtime.engine().exit_code(), Some(7));
    assert_eq!(outcome.global::<Option<bool>>("reached"), None);
}

#[tokio::test]
async fn exit_from_a_parallel_block_stops_every_thread() {
    let started = Instant::now();
    let outcome = run_script(
        r#"
        EnterParallel()
        local Process = import("Process")
        Process.exit(5)
        ExitParallel()
        sleep(20000)
        reached = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert_eq!(outcome.runtime.engine().exit_code(), Some(5));
    assert_eq!(outcome.global::<Option<bool>>("reached"), None);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn exposes_useful_folders() {
    let dir = main_script(
        r#"
        local Process = import("Process")
        folders = Process.dirs
        gameName = Process.gameName
        executable = Process.executable
        frozen = not pcall(function() Process.dirs.temp = "x" end)
        "#,
    );
    let project = Project::load(dir.path()).unwrap();
    let outcome = run_with(Arc::new(project.source_vfs()), "src/main.luau", |builder| {
        builder.game("My: Game", "C:/games/mine")
    })
    .await;
    outcome.assert_clean();
    let folders: Table = outcome.global("folders");
    assert_eq!(
        folders.get::<String>("temp").unwrap(),
        std::env::temp_dir().to_string_lossy()
    );
    assert_eq!(folders.get::<String>("game").unwrap(), "C:/games/mine");
    if let Some(data) = dirs::data_dir() {
        assert_eq!(folders.get::<String>("appData").unwrap(), data.to_string_lossy());
        assert_eq!(
            folders.get::<String>("save").unwrap(),
            data.join("My_ Game").to_string_lossy()
        );
    }
    if let Some(home) = dirs::home_dir() {
        assert_eq!(folders.get::<String>("home").unwrap(), home.to_string_lossy());
    }
    assert_eq!(outcome.global::<String>("gameName"), "My: Game");
    assert!(!outcome.global::<String>("executable").is_empty());
    assert!(outcome.global::<bool>("frozen"));
}

#[tokio::test]
async fn destroying_a_child_closes_its_pipes() {
    let outcome = run_script(
        r#"
        local Process = import("Process")
        local windows = Process.os == "windows"
        local child = Process.start(windows and "ping" or "sleep", windows and { "-n", "30", "127.0.0.1" } or { "30" })
        local stdin = child.Stdin
        child:Destroy()
        wrote, message = pcall(function() stdin:write("late\n") end)
        note = tostring(message)
        same = child.Stdin == stdin
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(!outcome.global::<bool>("wrote"));
    let note: String = outcome.global("note");
    assert!(note.contains("closed") || note.contains("destroyed"), "{note}");
    assert!(outcome.global::<bool>("same"));
}
