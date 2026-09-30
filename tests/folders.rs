mod common;

use std::fs;
use std::sync::Arc;

use common::native::fixture;
use common::{build, run, workspace, write};
use luv::plugins::{self, library_file};
use luv::project::{GameInfo, Project};
use luv::vfs::Vfs;

#[tokio::test]
async fn native_inter_libraries_travel_inside_the_package() {
    let dir = workspace(&[(
        "src/main.luau",
        r#"
        local DLL = import("DLL")
        local library = DLL.Load("embedded")
        result = library:GetFunction("add", "int", { "int", "int" })(40, 2)
        path = library.Path
        "#,
    )]);
    let root = dir.path();
    fs::create_dir_all(root.join("nativeInter")).unwrap();
    fs::copy(fixture(), root.join("nativeInter").join(library_file("embedded"))).unwrap();
    write(root, "nativeInter/embedded.d.luau", "export type Embedded_API = {\n\tAdd: (a: number, b: number) -> number,\n}\n");

    let pak = build(root).await;
    let inside = format!("{}/{}", plugins::EMBEDDED_DIR, library_file("embedded"));
    assert!(pak.is_file(&inside), "the library should be packed as {inside}");
    assert!(!pak.is_file(&format!("nativeInter/{}", library_file("embedded"))));
    assert!(!pak.is_file("nativeInter/embedded.d.luau"));

    let game = GameInfo::from_manifest(pak.manifest()).unwrap();
    let outcome = run(Arc::new(pak), &game.main).await;
    outcome.assert_clean();
    assert_eq!(outcome.global::<i64>("result"), 42);
    let path: String = outcome.global("path");
    assert!(path.contains("luv-natives"), "the library should load from its extracted copy, got {path}");

    let types = luv::typegen::generate(&Project::load(root).unwrap()).unwrap().0;
    assert!(types.contains("export type Embedded_API = {"), "nativeInter type files merge too");
}

#[test]
fn export_copies_its_whole_tree_next_to_the_game() {
    let dir = workspace(&[("src/main.luau", "print(1)\n")]);
    let root = dir.path();
    write(root, "export/data/readme.txt", "hello");
    write(root, "export/data/levels/one.json", "{}");
    fs::create_dir_all(root.join("export")).unwrap();
    fs::copy(fixture(), root.join("export").join(library_file("constant"))).unwrap();

    let project = Project::load(root).unwrap();
    let target = root.join("build").join("package");
    let mut exported = plugins::export(&project, &target).unwrap();
    exported.sort();
    let mut expected = vec![
        "data/levels/one.json".to_owned(),
        "data/readme.txt".to_owned(),
        library_file("constant"),
    ];
    expected.sort();
    assert_eq!(exported, expected);
    assert_eq!(fs::read_to_string(target.join("data/readme.txt")).unwrap(), "hello");
    assert!(target.join(library_file("constant")).is_file());
    assert!(project.container_folders().is_empty(), "export is never a container");
}

#[tokio::test]
async fn export_is_not_packed_into_the_game() {
    let dir = workspace(&[("src/main.luau", "print(1)\n")]);
    let root = dir.path();
    write(root, "export/data/readme.txt", "hello");
    let pak = build(root).await;
    assert!(!pak.is_file("export/data/readme.txt"));
}
