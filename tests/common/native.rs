use std::path::PathBuf;
use std::sync::OnceLock;

static FIXTURE: OnceLock<PathBuf> = OnceLock::new();

pub fn fixture() -> PathBuf {
    FIXTURE
        .get_or_init(|| {
            let binary = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.file_stem().map(|stem| stem.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "tests".to_owned());
            let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("native-fixture").join(binary);
            std::fs::create_dir_all(&directory).unwrap();
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let output = directory.join(luv::plugins::library_file("fixture"));
            luv::plugins::compile_c(
                "fixture",
                &[root.join("tests").join("fixtures").join("native.c")],
                &[root.join("templates")],
                &output,
                &directory.join("objects"),
            )
            .unwrap();
            output
        })
        .clone()
}

pub fn lua_path(path: &std::path::Path) -> String {
    path.display().to_string().replace('\\', "/")
}
