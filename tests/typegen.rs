mod common;

use std::fs;

use common::write;
use luv::project::{self, Project};
use luv::typegen;

#[test]
fn type_files_anywhere_in_the_workspace_are_merged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    project::init(root, None).unwrap();

    write(
        root,
        "src/Items.d.luau",
        "export type Item = {\n\tName: string,\n}\n\nexport type Items_API = {\n\tGet: (id: string) -> Item?,\n}\n\nexport type Imports = {\n\tItems: Items_API,\n}\n",
    );
    write(
        root,
        "Hud.d.luau",
        "export type Hud_API = {\n\tShow: (text: string) -> (),\n}\n\nexport type WindowAPIs = {\n\tHud: Hud_API,\n}\n",
    );
    write(root, "src/types.d.luau", "export type ShouldNotAppear = number\n");
    write(root, "src/Nested/TYPES.d.luau", "export type AlsoSkipped = number\n");
    write(root, "build/Output.d.luau", "export type FromBuild = number\n");
    write(root, ".hidden/Secret.d.luau", "export type Hidden = number\n");
    write(root, "Packages/_Index/pkg/Package.d.luau", "export type Vendored = number\n");

    let project = Project::load(root).unwrap();
    let report = typegen::sync(&project).unwrap();
    assert_eq!(report.sources, ["Hud.d.luau", "src/Items.d.luau"]);

    let types = fs::read_to_string(root.join("types.d.luau")).unwrap();
    assert!(types.contains("\t-- from src/Items.d.luau\n\tItems: Items_API,\n"));
    assert!(types.contains("\t-- from Hud.d.luau\n\tHud: Hud_API,\n"));
    assert!(types.contains("export type Item = {"));
    assert!(types.contains("export type Hud_API = {"));
    for skipped in ["ShouldNotAppear", "AlsoSkipped", "FromBuild", "Hidden", "Vendored"] {
        assert!(!types.contains(skipped), "{skipped} should not be merged");
    }
    assert_eq!(types.matches("export type Imports = {").count(), 1);

    let settings = fs::read_to_string(root.join(".vscode/settings.json")).unwrap();
    assert!(settings.contains("\"**/*.d.luau\""), "{settings}");

    let again = typegen::sync(&project).unwrap();
    assert!(!again.changed, "a second sync changes nothing");
}

#[test]
fn an_old_settings_file_learns_to_ignore_type_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    project::init(root, None).unwrap();
    write(
        root,
        ".vscode/settings.json",
        "{\n    \"luau-lsp.ignoreGlobs\": [\"**/_Index/**\", \"**/native/**/*.d.luau\"],\n    \"editor.tabSize\": 4\n}\n",
    );
    write(root, "src/Extra.d.luau", "export type Extra = number\n");

    typegen::sync(&Project::load(root).unwrap()).unwrap();
    let settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".vscode/settings.json")).unwrap()).unwrap();
    let globs = settings["luau-lsp.ignoreGlobs"].as_array().unwrap();
    assert!(globs.iter().any(|glob| glob == "**/*.d.luau"));
    assert!(globs.iter().any(|glob| glob == "**/native/**/*.d.luau"), "existing globs are kept");
    assert_eq!(settings["editor.tabSize"], 4, "other settings are kept");
    assert!(
        settings["luau-lsp.completion.imports.ignoreGlobs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|glob| glob == "**/*.d.luau")
    );
}
