mod common;

use common::{run_both, workspace};
use mlua::Table;
use tempfile::TempDir;

fn game(main: &str) -> TempDir {
    workspace(&[
        ("assets/test.txt", "hello from assets\nsecond line\n"),
        ("assets/gui/menu.json", "{ \"title\": \"Menu\" }"),
        ("assets/gui/icon.png", "not really a png"),
        ("assets/sounds/click.wav", "click"),
        ("assets/sounds/click.ogg", "click"),
        ("assets/scripts/tool.luau", "return 1\n"),
        ("src/main.luau", main),
    ])
}

#[tokio::test]
async fn load_string_reads_assets_by_path_or_name() {
    let dir = game(
        r#"
local Asset = import("Asset")
results = {
    named = Asset.LoadString("test"),
    exact = Asset.LoadString("test.txt"),
    nested = Asset.LoadString("gui/menu.json"),
    nestedName = Asset.LoadString("gui/menu"),
}
"#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let results: Table = outcome.global("results");
        assert_eq!(results.get::<String>("named").unwrap(), "hello from assets\nsecond line\n");
        assert_eq!(results.get::<String>("exact").unwrap(), "hello from assets\nsecond line\n");
        assert_eq!(results.get::<String>("nested").unwrap(), "{ \"title\": \"Menu\" }");
        assert_eq!(results.get::<String>("nestedName").unwrap(), "{ \"title\": \"Menu\" }");
    }
}

#[tokio::test]
async fn load_returns_an_asset_handle() {
    let dir = game(
        r#"
local Asset = import("Asset")
local FS = import("FS")
local icon = Asset.Load("gui/icon")
local text = Asset.Load("test")
local file = text:Open()
results = {
    className = icon.ClassName,
    name = icon.Name,
    path = icon.Path,
    size = icon.Size,
    extension = icon.Extension,
    contents = icon:ReadString(),
    buffer = buffer.tostring(icon:ReadBuffer()),
    fileType = FS.type(file),
    firstLine = file:read("l"),
    rest = file:read("a"),
    readOnly = select(2, file:write("nope")),
}
file:close()
icon:Destroy()
results.destroyed = not pcall(icon.ReadString, icon)
results.destroyedSize = icon.Size
"#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let results: Table = outcome.global("results");
        assert_eq!(results.get::<String>("className").unwrap(), "Asset");
        assert_eq!(results.get::<String>("name").unwrap(), "icon.png");
        assert_eq!(results.get::<String>("path").unwrap(), "gui/icon.png");
        assert_eq!(results.get::<i64>("size").unwrap(), 16);
        assert_eq!(results.get::<String>("extension").unwrap(), "png");
        assert_eq!(results.get::<String>("contents").unwrap(), "not really a png");
        assert_eq!(results.get::<String>("buffer").unwrap(), "not really a png");
        assert_eq!(results.get::<String>("fileType").unwrap(), "file");
        assert_eq!(results.get::<String>("firstLine").unwrap(), "hello from assets");
        assert_eq!(results.get::<String>("rest").unwrap(), "second line\n");
        assert!(results.get::<String>("readOnly").unwrap().contains("read-only"));
        assert!(results.get::<bool>("destroyed").unwrap());
        assert_eq!(results.get::<i64>("destroyedSize").unwrap(), 0);
    }
}

#[tokio::test]
async fn reports_missing_ambiguous_and_script_assets() {
    let dir = game(
        r#"
local Asset = import("Asset")
local function failure(...)
    local ok, message = pcall(...)
    assert(not ok, "expected a failure")
    return tostring(message)
end
errors = {
    failure(Asset.LoadString, "missing.txt"),
    failure(Asset.Load, "sounds/click"),
    failure(Asset.LoadString, "scripts/tool.luau"),
    failure(Asset.LoadString, "../src/main.luau"),
}
"#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let errors: Vec<String> = outcome.global("errors");
        let expected = [
            "cannot load asset 'missing.txt': no such asset",
            "the name is ambiguous, it matches assets/sounds/click.ogg, assets/sounds/click.wav",
            "scripts can only be loaded with require",
            "cannot load asset '../src/main.luau'",
        ];
        for (error, expected) in errors.iter().zip(expected) {
            assert!(error.contains(expected), "{error:?} should contain {expected:?}");
        }
    }
}

#[tokio::test]
async fn loading_only_suspends_the_calling_coroutine() {
    let dir = game(
        r#"
local Asset = import("Asset")
log = {}
coroutine.wrap(function()
    local text = Asset.LoadString("test")
    table.insert(log, "loaded " .. #text)
end)()
table.insert(log, "main")
"#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let log: Vec<String> = outcome.global("log");
        assert_eq!(log, ["main", "loaded 30"]);
    }
}
