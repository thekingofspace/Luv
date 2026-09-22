mod common;

use common::{main_script, run_source};

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(&format!(
        r#"
local Serde = import("Serde")
local function same(a, b)
    if type(a) ~= type(b) then
        return false
    end
    if type(a) ~= "table" then
        return a == b
    end
    for key, value in a do
        if not same(value, b[key]) then
            return false
        end
    end
    for key in b do
        if a[key] == nil then
            return false
        end
    end
    return true
end
{source}
"#
    ));
    run_source(dir.path()).await
}

#[tokio::test]
async fn every_format_round_trips() {
    let outcome = run_script(
        r#"
        local value = {
            name = "Luv",
            version = 2,
            ratio = 0.5,
            enabled = true,
            tags = { "a", "b" },
            nested = { deep = { answer = 42 } },
            list = { { id = 1 }, { id = 2 } },
        }
        for _, format in { "json", "jsonc", "toml", "yaml" } do
            for _, pretty in { false, true } do
                local text = Serde.Encode(format, value, pretty)
                assert(same(Serde.Decode(format, text), value), `{format} round trip failed:\n{text}`)
            end
        end
        done = true
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<bool>("done"));
}

#[tokio::test]
async fn json_is_compact_by_default_and_pretty_on_request() {
    let outcome = run_script(
        r#"
        compact = Serde.Encode("json", { b = 1, a = { 1, 2.5, "x" }, c = {} })
        pretty = Serde.Encode("json", { b = 1, a = { 1 } }, true)
        "#,
    )
    .await;
    outcome.assert_clean();
    assert_eq!(outcome.global::<String>("compact"), r#"{"a":[1,2.5,"x"],"b":1,"c":[]}"#);
    assert_eq!(outcome.global::<String>("pretty"), "{\n  \"a\": [\n    1\n  ],\n  \"b\": 1\n}");
}

#[tokio::test]
async fn jsonc_allows_comments_and_trailing_commas() {
    let outcome = run_script(
        r#"
        local decoded = Serde.Decode("jsonc", [[
{
    // the game's name
    "name": "Luv", /* inline comment */
    "list": [1, 2, 3,],
    "url": "http://example.com/*not a comment*/",
    "quote": "say \"hi\" // still text",
}
]])
        assert(decoded.name == "Luv")
        assert(#decoded.list == 3 and decoded.list[3] == 3)
        assert(decoded.url == "http://example.com/*not a comment*/")
        assert(decoded.quote == "say \"hi\" // still text")

        local ok, message = pcall(Serde.Decode, "jsonc", "{\n  // comment\n  \"a\": nope\n}")
        jsoncError = tostring(message)
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<String>("jsoncError").contains("line 3"), "{}", outcome.global::<String>("jsoncError"));
}

#[tokio::test]
async fn toml_documents_are_tables() {
    let outcome = run_script(
        r#"
        local decoded = Serde.Decode("toml", [=[
title = "Game"
released = 1979-05-27T07:32:00Z

[window]
width = 1280
height = 720

[[players]]
name = "one"

[[players]]
name = "two"
]=])
        assert(decoded.title == "Game")
        assert(decoded.released == "1979-05-27T07:32:00Z")
        assert(decoded.window.width == 1280 and decoded.window.height == 720)
        assert(#decoded.players == 2 and decoded.players[2].name == "two")

        assert(Serde.Encode("toml", {}) == "")
        local ok, message = pcall(Serde.Encode, "toml", { 1, 2 })
        tomlError = tostring(message)
        text = Serde.Encode("toml", { window = { width = 1280 }, title = "Game" }, true)
        "#,
    )
    .await;
    outcome.assert_clean();
    assert!(outcome.global::<String>("tomlError").contains("toml documents must be tables"));
    let text: String = outcome.global("text");
    assert!(text.contains("title = \"Game\""), "{text}");
    assert!(text.contains("[window]"), "{text}");
    assert!(text.contains("width = 1280"), "{text}");
}

#[tokio::test]
async fn yaml_is_flow_by_default_and_block_when_pretty() {
    let outcome = run_script(
        r#"
        local decoded = Serde.Decode("yaml", "name: Luv\nlist:\n  - 1\n  - two\nnested:\n  flag: true\n")
        assert(decoded.name == "Luv")
        assert(decoded.list[1] == 1 and decoded.list[2] == "two")
        assert(decoded.nested.flag == true)
        assert(Serde.Decode("yml", "a: 1").a == 1)

        compact = Serde.Encode("yaml", { a = 1, b = { "x" } })
        pretty = Serde.Encode("yaml", { a = 1, b = { "x" } }, true)
        "#,
    )
    .await;
    outcome.assert_clean();
    assert_eq!(outcome.global::<String>("compact"), r#"{"a":1,"b":["x"]}"#);
    let pretty: String = outcome.global("pretty");
    assert!(pretty.starts_with("a: 1\nb:\n"), "{pretty}");
    assert!(pretty.contains("- x"), "{pretty}");
}

#[tokio::test]
async fn converts_values_between_lua_and_data() {
    let outcome = run_script(
        r#"
        local decoded = Serde.Decode("json", '{"a": null, "b": [1, null, 3], "c": 1.5, "d": "text", "e": {}}')
        assert(decoded.a == nil)
        assert(decoded.b[1] == 1 and decoded.b[2] == nil and decoded.b[3] == 3)
        assert(decoded.c == 1.5 and decoded.d == "text")
        assert(type(decoded.e) == "table" and next(decoded.e) == nil)

        sparse = Serde.Encode("json", { [1] = "a", [3] = "c" })
        mixed = Serde.Encode("json", { "first", name = "second" })
        vector = Serde.Encode("json", { position = vector.create(1, 2.5, 3) })
        scalar = Serde.Encode("json", "just text")
        nothing = Serde.Encode("json", nil)
        "#,
    )
    .await;
    outcome.assert_clean();
    assert_eq!(outcome.global::<String>("sparse"), r#"{"1":"a","3":"c"}"#);
    assert_eq!(outcome.global::<String>("mixed"), r#"{"1":"first","name":"second"}"#);
    assert_eq!(outcome.global::<String>("vector"), r#"{"position":[1,2.5,3]}"#);
    assert_eq!(outcome.global::<String>("scalar"), r#""just text""#);
    assert_eq!(outcome.global::<String>("nothing"), "null");
}

#[tokio::test]
async fn reports_values_that_cannot_be_encoded_or_decoded() {
    let outcome = run_script(
        r#"
        local function failure(...)
            local ok, message = pcall(...)
            assert(not ok, "expected a failure")
            return tostring(message)
        end
        local cyclic = {}
        cyclic.self = cyclic
        errors = {
            failure(Serde.Encode, "json", { callback = print }),
            failure(Serde.Encode, "json", cyclic),
            failure(Serde.Encode, "json", { [true] = 1 }),
            failure(Serde.Encode, "xml", {}),
            failure(Serde.Decode, "json", "{ broken"),
            failure(Serde.Decode, "yaml", "a: [1, 2"),
            failure(Serde.Decode, "toml", "= nope"),
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let errors: Vec<String> = outcome.global("errors");
    let expected = [
        "function values cannot be encoded",
        "tables that contain themselves cannot be encoded",
        "table keys must be strings or numbers to be encoded, got boolean",
        "unknown format 'xml'",
        "cannot decode json",
        "cannot decode yaml",
        "cannot decode toml",
    ];
    for (error, expected) in errors.iter().zip(expected) {
        assert!(error.contains(expected), "{error:?} should contain {expected:?}");
    }
}

#[tokio::test]
async fn encoding_and_decoding_only_suspend_the_calling_coroutine() {
    let outcome = run_script(
        r#"
        log = {}
        local big = {}
        for index = 1, 20000 do
            big[index] = { id = index, name = "item " .. index }
        end
        coroutine.wrap(function()
            local text = Serde.Encode("json", big, true)
            local decoded = Serde.Decode("json", text)
            table.insert(log, "done " .. #decoded)
        end)()
        table.insert(log, "main")
        "#,
    )
    .await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["main", "done 20000"]);
}
