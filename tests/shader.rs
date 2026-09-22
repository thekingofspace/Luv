mod common;

use common::{main_script, run_both, run_source, workspace};
use mlua::Table;

const TRIANGLE: &str = r#"
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(index) - 1);
    let y = f32(i32(index & 1u) * 2 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
"#;

const COMPUTE: &str = r#"
@compute @workgroup_size(8, 4, 1)
fn main() {}
"#;

const BROKEN: &str = "@fragment\nfn fs_main() -> @location(0) vec4<f32> {\n    return missing;\n}\n";

const GLSL: &str = "#version 450\nlayout(location = 0) out vec4 color;\nvoid main() {\n    color = vec4(1.0);\n}\n";

fn script(body: &str) -> String {
    format!(
        "local Shader = import(\"Shader\")\nlocal TRIANGLE = [==[{TRIANGLE}]==]\nlocal COMPUTE = [==[{COMPUTE}]==]\nlocal BROKEN = [==[{BROKEN}]==]\nlocal GLSL = [==[{GLSL}]==]\n{body}"
    )
}

#[tokio::test]
async fn compiles_wgsl_into_vulkan_spirv() {
    let dir = main_script(&script(
        r#"
local shader = Shader.Compile(TRIANGLE)
local spirv = shader:GetSpirv()
local entries = {}
for _, entry in shader.EntryPoints do
    table.insert(entries, `{entry.Name}:{entry.Stage}`)
end
local compute = Shader.Compile({ Source = COMPUTE, Name = "particles" })
results = {
    className = shader.ClassName,
    name = shader.Name,
    language = shader.Language,
    compiled = shader.Compiled,
    error = shader.Error,
    size = shader.Size,
    spirvLength = buffer.len(spirv),
    magic = buffer.readu32(spirv, 0),
    entries = entries,
    computeName = compute.Name,
    workgroup = tostring(compute.EntryPoints[1].WorkgroupSize),
}
"#,
    ));
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(results.get::<String>("className").unwrap(), "Shader");
    assert_eq!(results.get::<String>("name").unwrap(), "shader");
    assert_eq!(results.get::<String>("language").unwrap(), "wgsl");
    assert!(results.get::<bool>("compiled").unwrap());
    assert_eq!(results.get::<Option<String>>("error").unwrap(), None);
    let size: usize = results.get("size").unwrap();
    assert!(size > 0 && size.is_multiple_of(4));
    assert_eq!(results.get::<usize>("spirvLength").unwrap(), size);
    assert_eq!(results.get::<u32>("magic").unwrap(), 0x0723_0203);
    assert_eq!(results.get::<Vec<String>>("entries").unwrap(), ["vs_main:vertex", "fs_main:fragment"]);
    assert_eq!(results.get::<String>("computeName").unwrap(), "particles");
    assert_eq!(results.get::<String>("workgroup").unwrap(), "UDim(8, 4, 1)");
}

#[tokio::test]
async fn compiles_arrays_and_reports_errors_per_shader() {
    let dir = main_script(&script(
        r#"
local shaders = Shader.Compile({ TRIANGLE, BROKEN, { Source = GLSL, Language = "glsl", Stage = "fragment", Name = "solid" } })
results = {
    count = #shaders,
    first = shaders[1].Compiled,
    second = shaders[2].Compiled,
    secondError = shaders[2].Error,
    secondSize = shaders[2].Size,
    third = shaders[3].Compiled,
    thirdName = shaders[3].Name,
    thirdLanguage = shaders[3].Language,
    getSpirvFails = not pcall(shaders[2].GetSpirv, shaders[2]),
}
local glsl = Shader.Compile({ Source = GLSL, Language = "glsl" })
results.glslWithoutStage = glsl.Error
"#,
    ));
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(results.get::<i64>("count").unwrap(), 3);
    assert!(results.get::<bool>("first").unwrap());
    assert!(!results.get::<bool>("second").unwrap());
    let error: String = results.get("secondError").unwrap();
    assert!(error.contains("missing"), "{error}");
    assert!(error.contains("shader:3:"), "{error}");
    assert_eq!(results.get::<i64>("secondSize").unwrap(), 0);
    assert!(results.get::<bool>("third").unwrap());
    assert_eq!(results.get::<String>("thirdName").unwrap(), "solid");
    assert_eq!(results.get::<String>("thirdLanguage").unwrap(), "glsl");
    assert!(results.get::<bool>("getSpirvFails").unwrap());
    assert!(results.get::<String>("glslWithoutStage").unwrap().contains("needs a Stage"));
}

#[tokio::test]
async fn callbacks_receive_each_shader_without_blocking() {
    let dir = main_script(&script(
        r#"
log = {}
local seen = {}
local returned = Shader.Compile({ TRIANGLE, COMPUTE, BROKEN }, function(shader, index)
    seen[index] = shader.Compiled
    table.insert(log, "compiled")
end)
table.insert(log, "returned")
results = { returned = returned == nil, seen = seen }
"#,
    ));
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["returned", "compiled", "compiled", "compiled"]);
    let results: Table = outcome.global("results");
    assert!(results.get::<bool>("returned").unwrap());
    let seen: Table = results.get("seen").unwrap();
    assert!(seen.get::<bool>(1).unwrap());
    assert!(seen.get::<bool>(2).unwrap());
    assert!(!seen.get::<bool>(3).unwrap());
}

#[tokio::test]
async fn compiling_only_suspends_the_calling_coroutine() {
    let dir = main_script(&script(
        r#"
log = {}
coroutine.wrap(function()
    local shaders = Shader.Compile({ TRIANGLE, COMPUTE })
    table.insert(log, "compiled " .. #shaders)
end)()
table.insert(log, "main")
"#,
    ));
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    let log: Vec<String> = outcome.global("log");
    assert_eq!(log, ["main", "compiled 2"]);
}

#[tokio::test]
async fn spirv_round_trips_through_buffers() {
    let dir = main_script(&script(
        r#"
local original = Shader.Compile(TRIANGLE)
local reloaded = Shader.Compile(original:GetSpirv())
results = {
    compiled = reloaded.Compiled,
    language = reloaded.Language,
    entries = #reloaded.EntryPoints,
}
"#,
    ));
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!(results.get::<bool>("compiled").unwrap());
    assert_eq!(results.get::<String>("language").unwrap(), "spirv");
    assert_eq!(results.get::<i64>("entries").unwrap(), 2);
}

#[tokio::test]
async fn compiles_assets_and_files_in_every_mode() {
    let dir = workspace(&[
        ("assets/shaders/triangle.wgsl", TRIANGLE),
        ("assets/shaders/solid.frag", GLSL),
        (
            "src/main.luau",
            r#"
local Shader = import("Shader")
local Asset = import("Asset")
local triangle = Asset.Load("shaders/triangle")
local solid = Asset.Load("shaders/solid.frag")
local shaders = Shader.Compile({ triangle, solid, triangle:Open() })
results = {}
for index, shader in shaders do
    results[index] = `{shader.Name}|{shader.Language}|{tostring(shader.Compiled)}|{#shader.EntryPoints}`
end
"#,
        ),
    ]);
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let results: Vec<String> = outcome.global("results");
        assert_eq!(
            results,
            [
                "shaders/triangle.wgsl|wgsl|true|2",
                "shaders/solid.frag|glsl|true|1",
                "shaders/triangle.wgsl|wgsl|true|2",
            ]
        );
    }
}

#[tokio::test]
async fn combos_join_shader_modules_before_compiling() {
    let dir = workspace(&[
        (
            "assets/shaders/utils.wgsl",
            "enable f16;\nfn brighten(color: vec4<f32>) -> vec4<f32> {\n    return min(color * 2.0, vec4<f32>(1.0));\n}\n",
        ),
        (
            "assets/shaders/main.wgsl",
            "enable f16;\n@fragment\nfn main() -> @location(0) vec4<f32> {\n    return brighten(vec4<f32>(0.25));\n}\n",
        ),
        (
            "assets/shaders/broken.wgsl",
            "@fragment\nfn main() -> @location(0) vec4<f32> {\n    return brighten(missing);\n}\n",
        ),
        ("assets/shaders/tint.frag", "#version 450\nvec4 tint() { return vec4(1.0); }\n"),
        (
            "assets/shaders/solid.frag",
            "#version 450\nlayout(location = 0) out vec4 color;\nvoid main() {\n    color = tint();\n}\n",
        ),
        (
            "src/main.luau",
            r#"
local Shader = import("Shader")
local Asset = import("Asset")
local utils = Asset.Load("shaders/utils.wgsl")
local combo = Shader.Combine({ utils, Asset.Load("shaders/main.wgsl") }, { Name = "bright" })
local nested = Shader.Combine({ combo, utils, Shader.Prelude }, { Name = "nested" })
local compiled = Shader.Compile(nested)
local broken = Shader.Compile(Shader.Combine({ utils, Asset.Load("shaders/broken.wgsl") }, { Name = "broken" }))
local glsl = Shader.Compile(Shader.Combine({ Asset.Load("shaders/tint.frag"), Asset.Load("shaders/solid.frag") }))
local ok, mixed = pcall(Shader.Combine, { utils, Asset.Load("shaders/tint.frag") })
local spirvOk, spirv = pcall(Shader.Combine, { compiled:GetSpirv() })
results = {
    className = combo.ClassName,
    name = combo.Name,
    language = combo.Language,
    parts = nested.Parts,
    compiled = compiled.Compiled,
    compiledName = compiled.Name,
    entries = #compiled.EntryPoints,
    directives = select(2, string.gsub(nested.Source, "enable f16;", "")),
    brokenError = broken.Error,
    glsl = glsl.Compiled,
    glslError = glsl.Error,
    mixed = not ok and tostring(mixed) or "",
    spirv = not spirvOk and tostring(spirv) or "",
}
"#,
        ),
    ]);
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap_or_default();
    assert_eq!(text("className"), "ShaderCombo");
    assert_eq!(text("name"), "bright");
    assert_eq!(text("language"), "wgsl");
    assert_eq!(
        results.get::<Vec<String>>("parts").unwrap(),
        ["shaders/utils.wgsl", "shaders/main.wgsl", "prelude"]
    );
    assert!(results.get::<bool>("compiled").unwrap(), "{:?}", results.get::<String>("brokenError"));
    assert_eq!(text("compiledName"), "nested");
    assert_eq!(results.get::<i64>("entries").unwrap(), 1);
    assert_eq!(results.get::<i64>("directives").unwrap(), 1);
    let broken = text("brokenError");
    assert!(broken.starts_with("shaders/broken.wgsl:3:"), "{broken}");
    assert!(broken.contains("missing"), "{broken}");
    assert!(results.get::<bool>("glsl").unwrap(), "{}", text("glslError"));
    assert!(text("mixed").contains("every part of a combo must use the same language"), "{}", text("mixed"));
    assert!(text("spirv").contains("cannot be combined"), "{}", text("spirv"));
}

#[tokio::test]
async fn rejects_values_that_are_not_shaders() {
    let dir = main_script(&script(
        r#"
local ok, message = pcall(Shader.Compile, 42)
badSource = tostring(message)
local ok2, message2 = pcall(Shader.Compile, { Source = TRIANGLE, Language = "hlsl" })
badLanguage = tostring(message2)
"#,
    ));
    let outcome = run_source(dir.path()).await;
    outcome.assert_clean();
    assert!(outcome.global::<String>("badSource").contains("bad shader source"));
    assert!(outcome.global::<String>("badLanguage").contains("unknown shader language 'hlsl'"));
}
