mod common;

use common::{global_string, main_script, run_both_with, run_with, workspace};
use mlua::Table;
use tempfile::TempDir;

fn scratch() -> (TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_string_lossy().replace('\\', "/");
    (dir, path)
}

async fn run_disk(source: &str) -> common::Outcome {
    let dir = main_script(source);
    let (_scratch, path) = scratch();
    let project = luv::project::Project::load(dir.path()).unwrap();
    run_with(
        std::sync::Arc::new(project.source_vfs()),
        "src/main.luau",
        global_string("scratch", path),
    )
    .await
}

#[tokio::test]
async fn reads_and_writes_files_like_lua_io() {
    let outcome = run_disk(
        r#"
        local FS = import("FS")
        local path = scratch .. "/data.txt"

        local file = assert(FS.open(path, "w"))
        assert(file.ClassName == "File")
        assert(file:write("first line\n", 42, " ", 3.5, "\n") == file)
        assert(file:write("last") == file)
        assert(file:close() == true)
        assert(FS.type(file) == "closed file")
        assert(not pcall(file.read, file))

        local input = assert(FS.open(path))
        assert(FS.type(input) == "file")
        assert(FS.type({}) == nil)
        assert(input:read() == "first line")
        local a, b = input:read("n", "*n")
        assert(a == 42 and b == 3.5)
        assert(input:read("L") == "\n")
        assert(input:read("a") == "last")
        assert(input:read("a") == "")
        assert(input:read("l") == nil)
        assert(input:read(0) == nil)
        assert(input:seek("set", 6) == 6)
        assert(input:read(4) == "line")
        assert(input:seek() == 10)
        assert(input:seek("end") == #"first line\n42 3.5\nlast")
        assert(input:setvbuf("full") == true)
        input:close()

        local lines = {}
        for line in FS.lines(path) do
            table.insert(lines, line)
        end
        assert(#lines == 3 and lines[1] == "first line" and lines[3] == "last")

        local handle = FS.open(path)
        local count = 0
        for line in handle:lines() do
            count += 1
        end
        assert(count == 3)
        assert(FS.type(handle) == "file")
        handle:close()

        local missing, message, code = FS.open(scratch .. "/missing.txt")
        assert(missing == nil and string.find(message, "missing.txt", 1, true) and type(code) == "number")
        assert(not pcall(FS.open, path, "rw"))
        done = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("done"));
}

#[tokio::test]
async fn supports_every_open_mode() {
    let outcome = run_disk(
        r#"
        local FS = import("FS")
        local path = scratch .. "/modes.txt"

        local file = FS.open(path, "w")
        file:write("abc")
        file:close()

        file = FS.open(path, "a")
        file:write("def")
        file:close()
        assert(FS.readFile(path) == "abcdef")

        file = FS.open(path, "r+")
        file:seek("set", 1)
        file:write("X")
        file:close()
        assert(FS.readFile(path) == "aXcdef")

        file = FS.open(path, "w+")
        file:write("new")
        file:seek("set")
        assert(file:read("a") == "new")
        file:close()

        file = FS.open(path, "r+b")
        assert(file:read(1) == "n")
        file:write("E")
        file:close()
        assert(FS.readFile(path) == "nEw")

        local reader = FS.open(path, "r")
        local ok, message = reader:write("nope")
        assert(ok == nil and type(message) == "string")
        reader:close()
        done = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("done"));
}

#[tokio::test]
async fn default_input_and_output_can_be_redirected() {
    let outcome = run_disk(
        r#"
        local FS = import("FS")
        local path = scratch .. "/out.txt"
        assert(FS.output() == FS.stdout)
        assert(FS.input() == FS.stdin)

        FS.output(path)
        FS.write("hello ", 1, "\n")
        FS.close()
        FS.output(FS.stdout)
        assert(FS.readFile(path) == "hello 1\n")

        FS.input(path)
        assert(FS.read() == "hello 1")
        for line in FS.lines() do
            error("the input should be empty")
        end
        done = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("done"));
}

#[tokio::test]
async fn file_utilities_work_on_disk() {
    let outcome = run_disk(
        r#"
        local FS = import("FS")
        local nested = scratch .. "/a/b/c"
        local file = nested .. "/file.txt"

        FS.makeDir(nested)
        assert(FS.isDir(nested) and not FS.isFile(nested))
        FS.writeFile(file, "data")
        FS.appendFile(file, buffer.fromstring("+more"))
        assert(FS.readFile(file) == "data+more")
        assert(FS.exists(file) and FS.isFile(file) and not FS.isDir(file))

        local meta = FS.metadata(file)
        assert(meta.kind == "file" and meta.size == 9 and meta.readonly == false)
        assert(type(meta.modified) == "number")
        assert(FS.metadata(nested).kind == "dir")

        FS.copy(scratch .. "/a", scratch .. "/copy")
        assert(FS.readFile(scratch .. "/copy/b/c/file.txt") == "data+more")
        FS.rename(scratch .. "/copy", scratch .. "/moved")
        assert(not FS.exists(scratch .. "/copy"))
        assert(table.concat(FS.readDir(scratch .. "/moved/b/c"), ",") == "file.txt")

        FS.remove(scratch .. "/moved/b/c/file.txt")
        FS.remove(scratch .. "/moved/b/c")
        FS.removeDir(scratch .. "/moved")
        assert(not FS.exists(scratch .. "/moved"))

        local ok, message = pcall(FS.readFile, scratch .. "/nope.txt")
        assert(not ok and string.find(tostring(message), "cannot read", 1, true))

        local temporary = FS.tmpname()
        assert(FS.isFile(temporary))
        FS.remove(temporary)
        done = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("done"));
}

#[tokio::test]
async fn file_operations_only_suspend_the_calling_coroutine() {
    let outcome = run_disk(
        r#"
        local FS = import("FS")
        log = {}
        coroutine.wrap(function()
            FS.writeFile(scratch .. "/big.txt", string.rep("x", 1000000))
            local file = FS.open(scratch .. "/big.txt")
            local data = file:read("a")
            file:close()
            table.insert(log, "done " .. #data)
        end)()
        table.insert(log, "main")
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["main", "done 1000000"]);
}

fn game() -> TempDir {
    workspace(&[
        ("types.d.luau", "export type Hidden = number\n"),
        (".luaurc", r#"{ "languageMode": "nonstrict", "aliases": { "Data": "./data", "Modules": "./src/modules", }, }"#),
        ("data/config.json", "{ \"volume\": 5 }\nsecond line\n"),
        ("data/levels/one.txt", "level one"),
        ("src/modules/alpha.luau", "return \"alpha\"\n"),
        ("src/modules/beta.luau", "return \"beta\"\n"),
        ("src/lib/init.luau", "local FS = import(\"FS\")\nreturn { siblings = FS.readDir(\"./\"), inside = FS.readDir(\"@self\") }\n"),
        ("src/lib/helper.luau", "return nil\n"),
        (
            "src/main.luau",
            r#"
local FS = import("FS")

results = {}
results.modules = FS.readDir("./modules")
results.root = FS.readDir("../")
results.config = FS.readFile("../data/config.json")
results.aliased = FS.readFile("@data/config.json")
results.levels = FS.readDir("@Data/levels")

local loaded = {}
for _, name in FS.readDir("@Modules") do
    table.insert(loaded, require("./modules/" .. name))
end
results.loaded = loaded
results.cached = require("./modules/alpha.luau") == require("./modules/alpha")

results.readScript = pcall(FS.readFile, "./main.luau")
results.scriptExists = FS.exists("./modules/alpha.luau")
results.isDir = FS.isDir("@Modules")
results.hidden = FS.exists("../build.toml") or FS.exists("../types.d.luau")

local ok, message = pcall(FS.writeFile, "./new.txt", "x")
results.writeError = tostring(message)
results.escape = pcall(FS.readFile, "../../outside.txt")

local file = FS.open("../data/config.json")
results.firstLine = file:read("l")
results.secondLine = file:read("l")
file:close()

local denied, reason = FS.open("../data/config.json", "w")
results.openDenied = denied == nil and reason

local meta = FS.metadata("@Data/config.json")
results.meta = { kind = meta.kind, size = meta.size, readonly = meta.readonly }

FS.copy("@Data", scratch .. "/extracted")
results.extracted = FS.readFile(scratch .. "/extracted/levels/one.txt")

local lib = require("./lib")
results.libSiblings = lib.siblings
results.libInside = lib.inside
"#,
        ),
    ])
}

#[tokio::test]
async fn game_files_are_read_from_the_workspace_and_the_package() {
    let dir = game();
    let (_scratch, path) = scratch();
    for outcome in run_both_with(dir.path(), global_string("scratch", path.clone())).await {
        outcome.assert_clean();
        let results: Table = outcome.global("results");
        let list = |key: &str| results.get::<Vec<String>>(key).unwrap();

        assert_eq!(list("modules"), ["alpha.luau", "beta.luau"]);
        assert_eq!(list("root"), [".luaurc", "data", "src"]);
        assert_eq!(results.get::<String>("config").unwrap(), "{ \"volume\": 5 }\nsecond line\n");
        assert_eq!(results.get::<String>("aliased").unwrap(), "{ \"volume\": 5 }\nsecond line\n");
        assert_eq!(list("levels"), ["one.txt"]);
        assert_eq!(list("loaded"), ["alpha", "beta"]);
        assert!(results.get::<bool>("cached").unwrap());

        assert!(!results.get::<bool>("readScript").unwrap());
        assert!(results.get::<bool>("scriptExists").unwrap());
        assert!(results.get::<bool>("isDir").unwrap());
        assert!(!results.get::<bool>("hidden").unwrap());
        assert!(results.get::<String>("writeError").unwrap().contains("read-only"));
        assert!(!results.get::<bool>("escape").unwrap());

        assert_eq!(results.get::<String>("firstLine").unwrap(), "{ \"volume\": 5 }");
        assert_eq!(results.get::<String>("secondLine").unwrap(), "second line");
        assert!(results.get::<String>("openDenied").unwrap().contains("read-only"));

        let meta: Table = results.get("meta").unwrap();
        assert_eq!(meta.get::<String>("kind").unwrap(), "file");
        assert_eq!(meta.get::<u64>("size").unwrap(), 28);
        assert!(meta.get::<bool>("readonly").unwrap());

        assert_eq!(results.get::<String>("extracted").unwrap(), "level one");
        assert_eq!(list("libSiblings"), ["lib", "main.luau", "modules"]);
        assert_eq!(list("libInside"), ["helper.luau", "init.luau"]);
    }
}

#[tokio::test]
async fn unknown_aliases_are_reported() {
    let dir = main_script(
        r#"
        local FS = import("FS")
        local ok, message = pcall(FS.readDir, "@Nowhere/thing")
        aliasError = tostring(message)
        "#,
    );
    let (_scratch, path) = scratch();
    for outcome in run_both_with(dir.path(), global_string("scratch", path.clone())).await {
        outcome.assert_clean();
        assert!(outcome.global::<String>("aliasError").contains("@nowhere is not a valid alias"));
    }
}
