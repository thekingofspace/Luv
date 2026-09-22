mod common;

use common::{run_both, workspace};
use mlua::Table;

#[tokio::test]
async fn folders_with_init_files_are_modules() {
    let dir = workspace(&[
        (".luaurc", r#"{ "aliases": { "Test": "./src/test", "Game": "./", "Chained": "@Test/child" } }"#),
        ("src/sibling.luau", "return \"sibling\"\n"),
        (
            "src/test/init.luau",
            "return { name = \"init\", sibling = require(\"./sibling\"), child = require(\"@self/child\") }\n",
        ),
        ("src/test/child.luau", "return \"child\"\n"),
        (
            "src/main.luau",
            r#"
local relative = require("./test")
local aliased = require("@Test")
local viaGame = require("@Game/src/test")
results = {
    name = relative.name,
    sibling = relative.sibling,
    child = relative.child,
    same = relative == aliased and aliased == viaGame,
    chained = require("@Chained"),
}
"#,
        ),
    ]);
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let results: Table = outcome.global("results");
        assert_eq!(results.get::<String>("name").unwrap(), "init");
        assert_eq!(results.get::<String>("sibling").unwrap(), "sibling");
        assert_eq!(results.get::<String>("child").unwrap(), "child");
        assert!(results.get::<bool>("same").unwrap());
        assert_eq!(results.get::<String>("chained").unwrap(), "child");
    }
}

#[tokio::test]
async fn modules_can_be_required_with_their_extension() {
    let dir = workspace(&[
        ("src/modules/alpha.luau", "return { }\n"),
        (
            "src/main.luau",
            "same = require(\"./modules/alpha.luau\") == require(\"./modules/alpha\")\n",
        ),
    ]);
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        assert!(outcome.global::<bool>("same"));
    }
}

#[tokio::test]
async fn luaurc_aliases_are_case_insensitive() {
    let dir = workspace(&[
        (".luaurc", "{\n    // comment\n    \"aliases\": { \"BaseGame\": \"./Scenes/BaseGame\", },\n}\n"),
        ("Scenes/BaseGame/init.luau", "return \"base game\"\n"),
        ("src/main.luau", "result = require(\"@basegame\") .. \" / \" .. require(\"@BaseGame\")\n"),
    ]);
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        assert_eq!(outcome.global::<String>("result"), "base game / base game");
    }
}
