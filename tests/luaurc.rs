mod common;

use std::fs;
use std::path::Path;

use common::{workspace, write};
use luv::luaurc::{self, ALIAS_DUMP, AliasReport};
use luv::project::Project;
use serde_json::{Map, Value};

fn container(root: &Path, folder: &str, name: &str) {
    write(
        root,
        &format!("{folder}/container.toml"),
        &format!("[container]\nname = \"{name}\"\nmain = \"src/{folder}/init.luau\"\n"),
    );
    write(root, &format!("{folder}/src/{folder}/init.luau"), "return {}\n");
}

fn sync(root: &Path) -> AliasReport {
    luaurc::sync(&Project::load(root).unwrap()).unwrap()
}

fn read(path: &Path) -> Option<Value> {
    fs::read_to_string(path).ok().map(|text| serde_json::from_str(&text).unwrap())
}

fn aliases(root: &Path) -> Map<String, Value> {
    read(&root.join(".luaurc"))
        .and_then(|document| document.get("aliases").and_then(Value::as_object).cloned())
        .unwrap_or_default()
}

fn dumped(root: &Path) -> Map<String, Value> {
    read(&root.join("build").join(ALIAS_DUMP))
        .and_then(|document| document.get("aliases").and_then(Value::as_object).cloned())
        .unwrap_or_default()
}

fn names(map: &Map<String, Value>) -> Vec<String> {
    map.keys().cloned().collect()
}

#[test]
fn containers_become_aliases_and_leave_when_they_do() {
    let dir = workspace(&[("src/main.luau", "print(\"hi\")\n")]);
    let root = dir.path();
    container(root, "expansion", "Expansion");
    container(root, "bonus", "Bonus Pack");

    let report = sync(root);
    assert_eq!(report.added, ["Bonus-Pack", "Expansion"]);
    assert!(report.removed.is_empty() && report.updated.is_empty());
    assert_eq!(
        aliases(root)["Expansion"],
        Value::String("./expansion/src/expansion".to_owned())
    );
    assert_eq!(
        aliases(root)["Bonus-Pack"],
        Value::String("./bonus/src/bonus".to_owned())
    );
    assert_eq!(names(&dumped(root)), ["Bonus-Pack", "Expansion"]);

    let again = sync(root);
    assert!(!again.changed(), "{}", again.summary());

    fs::remove_dir_all(root.join("bonus")).unwrap();
    let after = sync(root);
    assert_eq!(after.removed, ["Bonus-Pack"]);
    assert_eq!(names(&aliases(root)), ["Expansion"]);
    assert_eq!(names(&dumped(root)), ["Expansion"]);
}

#[test]
fn hand_written_entries_survive() {
    let dir = workspace(&[("src/main.luau", "print(\"hi\")\n")]);
    let root = dir.path();
    write(
        root,
        ".luaurc",
        "{\n    \"languageMode\": \"strict\",\n    \"aliases\": {\n        \"lib\": \"./lib\"\n    }\n}\n",
    );
    container(root, "expansion", "Expansion");

    sync(root);
    let document = read(&root.join(".luaurc")).unwrap();
    assert_eq!(document["languageMode"], Value::String("strict".to_owned()));
    assert_eq!(names(&aliases(root)), ["Expansion", "lib"]);

    fs::remove_dir_all(root.join("expansion")).unwrap();
    sync(root);
    assert_eq!(names(&aliases(root)), ["lib"]);
    assert!(!root.join("build").join(ALIAS_DUMP).exists());
}

#[test]
fn edited_entries_are_left_alone() {
    let dir = workspace(&[("src/main.luau", "print(\"hi\")\n")]);
    let root = dir.path();
    container(root, "expansion", "Expansion");
    sync(root);

    write(
        root,
        ".luaurc",
        "{\n    \"aliases\": {\n        \"Expansion\": \"./somewhere/else\"\n    }\n}\n",
    );
    let report = sync(root);
    assert!(!report.changed(), "{}", report.summary());
    assert_eq!(
        aliases(root)["Expansion"],
        Value::String("./somewhere/else".to_owned())
    );
    assert!(dumped(root).is_empty());

    fs::remove_dir_all(root.join("expansion")).unwrap();
    sync(root);
    assert_eq!(names(&aliases(root)), ["Expansion"]);
}

#[test]
fn comments_are_read_and_the_file_is_left_alone_when_nothing_changes() {
    let dir = workspace(&[("src/main.luau", "print(\"hi\")\n")]);
    let root = dir.path();
    let source = "{\n    // the alias for shared code\n    \"aliases\": { \"lib\": \"./lib\", },\n}\n";
    write(root, ".luaurc", source);

    let report = sync(root);
    assert!(!report.changed());
    assert_eq!(fs::read_to_string(root.join(".luaurc")).unwrap(), source);
}

#[test]
fn building_writes_the_file() {
    let dir = workspace(&[("src/main.luau", "print(\"hi\")\n")]);
    let root = dir.path();
    container(root, "expansion", "Expansion");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_luv"))
        .arg("build")
        .arg(root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(output.status.success(), "{stdout}{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("Updated .luaurc (added Expansion)"), "{stdout}");
    assert_eq!(names(&aliases(root)), ["Expansion"]);
    assert_eq!(names(&dumped(root)), ["Expansion"]);

    let again = std::process::Command::new(env!("CARGO_BIN_EXE_luv"))
        .arg("build")
        .arg(root)
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&again.stdout).contains("Updated .luaurc"));
}

#[test]
fn the_command_writes_the_file_on_its_own() {
    let dir = workspace(&[("src/main.luau", "print(\"hi\")\n")]);
    let root = dir.path();
    container(root, "expansion", "Expansion");

    let luv = |command: &str| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_luv"))
            .arg(command)
            .arg(root)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    let first = luv("luaurc");
    assert!(first.contains("Updated") && first.contains("+ Expansion"), "{first}");
    assert_eq!(names(&aliases(root)), ["Expansion"]);
    assert_eq!(names(&dumped(root)), ["Expansion"]);
    assert!(!root.join("build").join("Fixture.luvit").exists());

    assert!(luv("aliases").contains("already up to date"));

    fs::remove_dir_all(root.join("expansion")).unwrap();
    assert!(luv("luaurc").contains("- Expansion (removed)"));
    assert!(aliases(root).is_empty());
}

#[test]
fn config_luau_projects_are_skipped() {
    let dir = workspace(&[("src/main.luau", "print(\"hi\")\n")]);
    let root = dir.path();
    write(root, ".config.luau", "return { luau = { aliases = {} } }\n");
    container(root, "expansion", "Expansion");

    let report = sync(root);
    assert!(!report.changed());
    assert!(report.note.is_some());
    assert!(!root.join(".luaurc").exists());
}

#[test]
fn the_build_setting_turns_it_off() {
    let dir = workspace(&[("src/main.luau", "print(\"hi\")\n")]);
    let root = dir.path();
    write(root, "build.toml", "[game]\nname = \"Fixture\"\n\n[build]\naliases = false\n");
    container(root, "expansion", "Expansion");

    let report = sync(root);
    assert!(!report.changed());
    assert!(!root.join(".luaurc").exists());
}
