mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::native::fixture;
use common::{workspace, write};
use luv::plugins::library_file;
use luv::vfs::Pak;

const SCRIPT: &str = r#"
local DLL = import("DLL")
local Process = import("Process")
local fixture = DLL.Load("./fixture")
print("sum", fixture:GetFunction("add", "i32", { "i32", "i32" })(20, 22))
local math = DLL.Load("./math")
print("triple", math:GetFunction("triple", "int", { "int" })(5))
print("args", table.concat(Process.args, ","))
Process.exit(3)
"#;

fn project() -> tempfile::TempDir {
    let dir = workspace(&[("src/main.luau", SCRIPT)]);
    let root = dir.path();
    write(
        root,
        "native/math.c",
        "#include \"luv.h\"\nLUV_EXPORT int triple(int value) { return value * 3; }\n",
    );
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates").join("luv.h"),
        root.join("native").join("luv.h"),
    )
    .unwrap();
    std::fs::copy(fixture(), root.join("native").join(library_file("fixture"))).unwrap();
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::copy(fixture(), root.join("assets").join(library_file("stray"))).unwrap();
    dir
}

fn package(root: &Path, console: bool) -> (String, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_luv"));
    command.arg("package").arg(root);
    if console {
        command.arg("--console");
    }
    let output = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "luv package failed:\n{stdout}\n{stderr}");
    (stdout, stderr)
}

fn executable(root: &Path) -> PathBuf {
    root.join("build")
        .join("package")
        .join(if cfg!(windows) { "Fixture.exe" } else { "Fixture" })
}

#[cfg(windows)]
fn subsystem(path: &Path) -> u16 {
    let bytes = std::fs::read(path).unwrap();
    let header = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    u16::from_le_bytes(bytes[header + 92..header + 94].try_into().unwrap())
}

#[test]
fn packages_a_standalone_executable_with_its_native_libraries_beside_it() {
    let dir = project();
    let root = dir.path();
    let (stdout, stderr) = package(root, false);
    assert!(stdout.contains("Packaged Fixture"), "{stdout}");
    assert!(stdout.contains(&library_file("math")) && stdout.contains(&library_file("fixture")), "{stdout}");
    assert!(stderr.contains(&format!("assets/{} is a native library", library_file("stray"))), "{stderr}");

    let game = executable(root);
    let directory = game.parent().unwrap();
    let mut shipped: Vec<String> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    shipped.sort();
    let mut expected = vec![
        game.file_name().unwrap().to_string_lossy().into_owned(),
        library_file("fixture"),
        library_file("math"),
    ];
    expected.sort();
    assert_eq!(shipped, expected);

    let pak = Pak::open(&game).unwrap();
    assert!(pak.entries().all(|(path, _)| !luv::project::is_native_library(path) && !path.starts_with("native/")));
    assert!(pak.entry("src/main.luau").is_some());

    #[cfg(windows)]
    assert_eq!(subsystem(&game), 2);

    let output = Command::new(&game).args(["one", "two"]).current_dir(std::env::temp_dir()).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert_eq!(output.status.code(), Some(3), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("sum\t42"), "{stdout}");
    assert!(stdout.contains("triple\t15"), "{stdout}");
    assert!(stdout.contains("args\tone,two"), "{stdout}");
}

fn icon_project(icon: &str) -> tempfile::TempDir {
    let dir = workspace(&[("src/main.luau", "print(\"icon game\")\n")]);
    write(dir.path(), "build.toml", &format!("[game]\nname = \"Fixture\"\nicon = \"{icon}\"\n"));
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    dir
}

#[cfg(windows)]
fn main_icon_size(game: &Path) -> (u32, u32) {
    let program = editpe::Image::parse_file(game).unwrap();
    let data = program.resource_directory().unwrap().get_main_icon().unwrap().unwrap().to_vec();
    let icon = image::load_from_memory(&data).unwrap();
    (icon.width(), icon.height())
}

#[test]
fn packages_carry_the_game_icon_from_any_image_format() {
    let dir = icon_project("assets/icon.png");
    let root = dir.path();
    image::RgbaImage::from_pixel(40, 20, image::Rgba([255, 0, 128, 255]))
        .save(root.join("assets").join("icon.png"))
        .unwrap();
    let (stdout, _) = package(root, false);
    assert!(stdout.contains("icon from assets/icon.png"), "{stdout}");
    let game = executable(root);

    #[cfg(windows)]
    assert_eq!(main_icon_size(&game), (256, 256));
    #[cfg(not(windows))]
    {
        let icon = image::open(game.with_file_name("Fixture.png")).unwrap();
        assert_eq!((icon.width(), icon.height()), (40, 40));
    }

    let output = Command::new(&game).current_dir(std::env::temp_dir()).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("icon game"));
}

#[test]
fn packages_use_ico_files_as_they_are() {
    let dir = icon_project("assets/icon.ico");
    let root = dir.path();
    image::RgbaImage::from_pixel(32, 32, image::Rgba([0, 128, 255, 255]))
        .save(root.join("assets").join("icon.ico"))
        .unwrap();
    let (stdout, _) = package(root, false);
    assert!(stdout.contains("icon from assets/icon.ico"), "{stdout}");
    let game = executable(root);

    #[cfg(windows)]
    assert_eq!(main_icon_size(&game), (32, 32));
    #[cfg(not(windows))]
    {
        let icon = image::open(game.with_file_name("Fixture.png")).unwrap();
        assert_eq!((icon.width(), icon.height()), (32, 32));
    }
}

#[test]
fn packages_warn_when_the_icon_is_missing() {
    let dir = icon_project("assets/missing.png");
    let (stdout, stderr) = package(dir.path(), false);
    assert!(!stdout.contains("icon from"), "{stdout}");
    assert!(stderr.contains("the icon assets/missing.png set in build.toml does not exist"), "{stderr}");
    assert!(executable(dir.path()).is_file());
}

#[cfg(windows)]
#[test]
fn console_packages_keep_their_console() {
    let dir = project();
    package(dir.path(), true);
    assert_eq!(subsystem(&executable(dir.path())), 3);
}
