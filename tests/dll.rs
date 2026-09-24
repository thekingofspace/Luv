mod common;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use common::native::{fixture, lua_path};
use common::{Outcome, main_script, run_with, write};
use luv::graphics::gpu::Gpu;
use luv::graphics::protocol::Capture;
use luv::objects::Window;
use luv::project::Project;
use luv::window::HeadlessWindows;
use mlua::{AnyUserData, Table};

async fn run_in(root: &Path) -> Outcome {
    let project = Project::load(root).unwrap();
    let directory = root.to_path_buf();
    run_with(Arc::new(project.source_vfs()), "src/main.luau", move |builder| {
        builder.game("Fixture", directory.clone())
    })
    .await
}

async fn run_dll(source: &str) -> Outcome {
    let dir = main_script(&format!("local FIXTURE = \"{}\"\n{source}", lua_path(&fixture())));
    run_in(dir.path()).await
}

fn text(results: &Table, name: &str) -> String {
    results.get::<String>(name).unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn number(results: &Table, name: &str) -> f64 {
    results.get::<f64>(name).unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn flag(results: &Table, name: &str) -> bool {
    results.get::<bool>(name).unwrap_or_else(|error| panic!("{name}: {error}"))
}

#[tokio::test]
async fn calls_functions_with_scalars_strings_and_buffers() {
    let outcome = run_dll(
        r#"
local DLL = import("DLL")
local lib = DLL.Load(FIXTURE)
results = {}
results.path = lib.Path
results.class = lib.ClassName
results.add = lib:GetFunction("add", "int", { "int", "int" })(40, 2)
results.addCall = lib:GetFunction("add", "i32", { "i32", "i32" }):Call(-5, 3)
results.mix = lib:GetFunction("mix", "double", { "double", "float", "i8", "u64" })(1.5, 2, -3, 1000)
results.even = lib:GetFunction("is_even", "bool", { "u32" })(10)
results.odd = lib:GetFunction("is_even", "bool", { "u32" })(7)
results.negate = lib:GetFunction("negate16", "short", { "short" })(-300)
results.length = lib:GetFunction("text_length", "size_t", { "string" })("hello")
results.nilLength = lib:GetFunction("text_length", "size_t", { "string" })(nil)
results.greeting = lib:GetFunction("greeting", "string")()
results.nothing = lib:GetFunction("nothing", "string")() == nil
results.wide = lib:GetFunction("wide_length", "usize", { "wstring" })("héllo wörld")
results.wideGreeting = lib:GetFunction("wide_greeting", "wstring")()
local bytes = buffer.create(6)
lib:GetFunction("fill", "void", { "pointer", "usize", "u8" })(bytes, 4, 65)
results.filled = buffer.tostring(bytes)
results.sum = lib:GetFunction("sum_bytes", "i64", { "pointer", "usize" })("\1\2\3\250", 4)
results.hasAdd = lib:HasSymbol("add")
results.hasMissing = lib:HasSymbol("definitely_missing")
local counter = lib:GetSymbol("counter")
results.counter = counter:Read("i32")
counter:Write("i32", 9)
results.counterAfter = lib:GetFunction("get_counter", "int")()
"#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!(text(&results, "path").contains("fixture"));
    assert_eq!(text(&results, "class"), "Library");
    assert_eq!(number(&results, "add"), 42.0);
    assert_eq!(number(&results, "addCall"), -2.0);
    assert_eq!(number(&results, "mix"), 1000.0);
    assert!(flag(&results, "even"));
    assert!(!flag(&results, "odd"));
    assert_eq!(number(&results, "negate"), 300.0);
    assert_eq!(number(&results, "length"), 5.0);
    assert_eq!(number(&results, "nilLength"), 0.0);
    assert_eq!(text(&results, "greeting"), "hello from C");
    assert!(flag(&results, "nothing"));
    assert_eq!(number(&results, "wide"), 11.0);
    assert_eq!(text(&results, "wideGreeting"), "wide hello");
    assert_eq!(text(&results, "filled"), "AAAA\0\0");
    assert_eq!(number(&results, "sum"), 256.0);
    assert!(flag(&results, "hasAdd"));
    assert!(!flag(&results, "hasMissing"));
    assert_eq!(number(&results, "counter"), 7.0);
    assert_eq!(number(&results, "counterAfter"), 9.0);
}

#[tokio::test]
async fn structs_arrays_and_native_memory() {
    let outcome = run_dll(
        r#"
local DLL = import("DLL")
local lib = DLL.Load(FIXTURE)
results = {}
local Vec2 = DLL.Struct({ { "x", "f32" }, { "y", "f32" } })
local Record = DLL.Struct({ { "id", "i32" }, { "weight", "f64" }, { Name = "tag", Type = DLL.Array("char", 8) } })
results.recordSize = Record.Size
results.recordAlign = Record.Alignment
results.tagOffset = Record:Offset("tag")
results.fields = table.concat(Record.Fields, ",")
local added = lib:GetFunction("vec2_add", Vec2, { Vec2, Vec2 })({ x = 1, y = 2 }, udim.new(3, 4))
results.added = `{added.x},{added.y}`
results.vectorLength = lib:GetFunction("vec2_length", "f32", { Vec2 })({ x = 3, y = 4 })
local record = lib:GetFunction("make_record", Record, { "i32", "f64" })(21, 2.5)
results.recordId = record.id
results.recordWeight = record.weight
results.tag = string.char(record.tag[1], record.tag[2], record.tag[3])
local stored = Record:New({ id = 5, weight = 1, tag = "abc" })
results.recordFromMemory = lib:GetFunction("record_id", "i32", { "pointer" })(stored)
results.storedTag = stored:ReadString(nil, Record:Offset("tag"))
local out = DLL.New("i32")
lib:GetFunction("set_out", "void", { "pointer", "i32" })(out, 77)
results.out = out:Read("i32")
local memory = DLL.Alloc(16)
memory:Write("u16", 513, 2)
results.u16 = memory:Read("u16", 2)
results.lowByte = memory:Read("u8", 2)
memory:WriteString("hi", 8)
results.string = memory:ReadString(nil, 8)
results.size = memory.Size
local tail = memory:Offset(12)
results.tailSize = tail.Size
results.bounds = tostring(select(2, pcall(function() tail:Read("i64") end)))
local copy = DLL.Alloc(4)
DLL.Fill(copy, 7, 4)
results.filled = copy:Read("u8", 3)
DLL.Copy(copy, memory:Offset(8), 3)
results.copied = copy:ReadString()
results.owned = DLL.String("owned"):ReadString()
results.wideOwned = DLL.String("wide ✓", true):ReadWideString()
results.raw = buffer.len(memory:ReadBuffer(4))
memory:Free()
results.freed = tostring(select(2, pcall(function() memory:Read("u8") end)))
results.doubleFree = tostring(select(2, pcall(function() memory:Free() end)))
results.derivedFree = tostring(select(2, pcall(function() copy:Offset(1):Free() end)))
results.nullRead = tostring(select(2, pcall(function() DLL.Null:Read("i32") end)))
results.sizeOfRecord = DLL.SizeOf(Record)
results.alignOfDouble = DLL.AlignOf("double")
results.equalPointers = DLL.Pointer(1234) == DLL.Pointer(1234)
results.nullText = tostring(DLL.Null)
results.nullFlag = DLL.Null.IsNull
results.extension = DLL.Extension
results.voidValue = tostring(select(2, pcall(function() DLL.New("void") end)))
results.badField = tostring(select(2, pcall(function() DLL.Struct({ { "x", "f32" }, { "x", "f32" } }) end)))
"#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(number(&results, "recordSize"), 24.0);
    assert_eq!(number(&results, "recordAlign"), 8.0);
    assert_eq!(number(&results, "tagOffset"), 16.0);
    assert_eq!(text(&results, "fields"), "id,weight,tag");
    assert_eq!(text(&results, "added"), "4,6");
    assert_eq!(number(&results, "vectorLength"), 5.0);
    assert_eq!(number(&results, "recordId"), 21.0);
    assert_eq!(number(&results, "recordWeight"), 2.5);
    assert_eq!(text(&results, "tag"), "rec");
    assert_eq!(number(&results, "recordFromMemory"), 10.0);
    assert_eq!(text(&results, "storedTag"), "abc");
    assert_eq!(number(&results, "out"), 77.0);
    assert_eq!(number(&results, "u16"), 513.0);
    assert_eq!(number(&results, "lowByte"), 1.0);
    assert_eq!(text(&results, "string"), "hi");
    assert_eq!(number(&results, "size"), 16.0);
    assert_eq!(number(&results, "tailSize"), 4.0);
    assert!(text(&results, "bounds").contains("outside the 16 bytes"));
    assert_eq!(number(&results, "filled"), 7.0);
    assert_eq!(text(&results, "copied"), "hi\0\u{7}".trim_end_matches(['\0', '\u{7}']));
    assert_eq!(text(&results, "owned"), "owned");
    assert_eq!(text(&results, "wideOwned"), "wide ✓");
    assert_eq!(number(&results, "raw"), 4.0);
    assert!(text(&results, "freed").contains("has been freed"));
    assert!(text(&results, "doubleFree").contains("has been freed"));
    assert!(text(&results, "derivedFree").contains("only the Pointer returned by"));
    assert!(text(&results, "nullRead").contains("null Pointer"));
    assert_eq!(number(&results, "sizeOfRecord"), 24.0);
    assert_eq!(number(&results, "alignOfDouble"), 8.0);
    assert!(flag(&results, "equalPointers"));
    assert_eq!(text(&results, "nullText"), "Pointer(null)");
    assert!(flag(&results, "nullFlag"));
    assert_eq!(text(&results, "extension"), format!(".{}", luv::plugins::EXTENSION));
    assert!(text(&results, "voidValue").contains("void"));
    assert!(text(&results, "badField").contains("more than one field named 'x'"));
}

#[tokio::test]
async fn callbacks_run_luau_from_any_native_thread() {
    let outcome = run_dll(
        r#"
local DLL = import("DLL")
local lib = DLL.Load(FIXTURE)
results = {}
local apply = lib:GetFunction("apply", "i32", { "pointer", "i32" })
local add = lib:GetFunction("add", "i32", { "i32", "i32" })
local calls = 0
local double = DLL.Callback("i32", { "i32" }, function(value)
    calls += 1
    return value * 2
end)
results.apply = apply(double, 20)
local combine = DLL.Callback("int", { "int", "int" }, function(a, b)
    return a + b
end)
local values = DLL.New(DLL.Array("i32", 5), { 1, 2, 3, 4, 5 })
results.reduce = lib:GetFunction("reduce", "i32", { "pointer", "usize", "pointer" })(values, 5, combine)
local nested = DLL.Callback("i32", { "i32" }, function(value)
    return add(value, 100)
end)
results.reenter = apply(nested, 1)
results.thread = lib:GetFunction("call_from_thread", "i32", { "pointer", "i32" })(double, 21)
local measure = DLL.Callback("i32", { "string" }, function(text)
    return #text
end)
results.measure = lib:GetFunction("measure", "i32", { "pointer", "string" })(measure, "abcdef")
local Vec2 = DLL.Struct({ { "x", "f32" }, { "y", "f32" } })
local swap = DLL.Callback(Vec2, { Vec2 }, function(value)
    return { x = value.y * 2, y = value.x * 2 }
end)
results.average = lib:GetFunction("average", "double", { "pointer", Vec2 })(swap, { x = 1, y = 3 })
local yielding = DLL.Callback("i32", { "i32" }, function(value)
    sleep(5)
    return value + 1000
end)
results.yielding = apply(yielding, 1)
results.calls = calls
results.pointerType = typeof(double.Pointer)
double:Destroy()
results.destroyed = tostring(select(2, pcall(function() return double.Pointer end)))
local failing = DLL.Callback("i32", { "i32" }, function()
    error("callback exploded")
end)
results.failed = apply(failing, 5)
"#,
    )
    .await;
    assert_eq!(outcome.errors.len(), 1, "{:#?}", outcome.errors);
    assert!(outcome.errors[0].contains("callback exploded"));
    let results: Table = outcome.global("results");
    assert_eq!(number(&results, "apply"), 41.0);
    assert_eq!(number(&results, "reduce"), 15.0);
    assert_eq!(number(&results, "reenter"), 102.0);
    assert_eq!(number(&results, "thread"), 42.0);
    assert_eq!(number(&results, "measure"), 6.0);
    assert_eq!(number(&results, "average"), 4.0);
    assert_eq!(number(&results, "yielding"), 1002.0);
    assert_eq!(number(&results, "calls"), 2.0);
    assert_eq!(text(&results, "pointerType"), "Pointer");
    assert!(text(&results, "destroyed").contains("has been destroyed"));
    assert_eq!(number(&results, "failed"), 1.0);
}

#[tokio::test]
async fn calls_only_block_their_coroutine_and_keep_thread_affinity() {
    let outcome = run_dll(
        r#"
local DLL = import("DLL")
local lib = DLL.Load(FIXTURE)
results = {}
local threadId = lib:GetFunction("thread_id", "u64")
local first = threadId()
results.sameThread = threadId() == first
results.parallelElsewhere = lib:GetFunction("thread_id", "u64", {}, { Parallel = true })() ~= first
local value, code = lib:GetFunction("set_error", "void", { "i32" }, { ErrorCode = true })(1234)
results.errorCode = code
results.errorValue = value == nil
local ticks = 0
local running = true
coroutine.wrap(function()
    while running do
        ticks += 1
        sleep(1)
    end
end)()
local before = ticks
lib:GetFunction("sleep_ms", "void", { "u32" })(150)
results.ticksDuringCall = ticks - before
running = false
local add = lib:GetFunction("add", "i32", { "i32", "i32" })
results.tooMany = tostring(select(2, pcall(add, 1, 2, 3)))
results.badArgument = tostring(select(2, pcall(add, 1.5, 2)))
results.badOption = tostring(select(2, pcall(function() lib:GetFunction("add", "i32", {}, { Nope = true }) end)))
results.badType = tostring(select(2, pcall(function() lib:GetFunction("add", "int32") end)))
results.missingSymbol = tostring(select(2, pcall(function() lib:GetFunction("missing_function", "void") end)))
results.arrayArgument = tostring(select(2, pcall(function() lib:GetFunction("add", "void", { DLL.Array("u8", 4) }) end)))
lib:Destroy()
results.afterDestroy = tostring(select(2, pcall(add, 1, 2)))
results.lookupAfterDestroy = tostring(select(2, pcall(function() lib:GetSymbol("add") end)))
"#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!(flag(&results, "sameThread"));
    assert!(flag(&results, "parallelElsewhere"));
    assert_eq!(number(&results, "errorCode"), 1234.0);
    assert!(flag(&results, "errorValue"));
    assert!(number(&results, "ticksDuringCall") >= 5.0, "the VM stalled during a native call");
    assert!(text(&results, "tooMany").contains("takes 2 arguments, got 3"));
    assert!(text(&results, "badArgument").contains("argument #1 of add"));
    assert!(text(&results, "badOption").contains("unknown function option 'Nope'"));
    assert!(text(&results, "badType").contains("'int32' is not a DLL type"));
    assert!(text(&results, "missingSymbol").contains("no exported symbol named 'missing_function'"));
    assert!(text(&results, "arrayArgument").contains("arrays cannot be passed by value"));
    assert!(text(&results, "afterDestroy").contains("unloaded"));
    assert!(text(&results, "lookupAfterDestroy").contains("has been destroyed"));
}

#[tokio::test]
async fn relative_paths_resolve_next_to_the_game() {
    let dir = main_script(
        r#"
local DLL = import("DLL")
local lib = DLL.Load("./moved")
results = {}
results.add = lib:GetFunction("add", "i32", { "i32", "i32" })(1, 2)
results.missing = tostring(select(2, pcall(DLL.Load, "./does_not_exist")))
"#,
    );
    std::fs::copy(fixture(), dir.path().join(luv::plugins::library_file("moved"))).unwrap();
    let outcome = run_in(dir.path()).await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(number(&results, "add"), 3.0);
    let missing = text(&results, "missing");
    assert!(missing.contains("cannot find './does_not_exist'"), "{missing}");
}

type Captures = Arc<Mutex<HashMap<String, Capture>>>;

#[tokio::test]
async fn render_hooks_feed_shaders_from_native_code() {
    if Gpu::get().is_err() {
        eprintln!("skipping a rendering test: no GPU");
        return;
    }
    let dir = main_script(&format!(
        "local FIXTURE = \"{}\"\n{}",
        lua_path(&fixture()),
        r#"
local DLL = import("DLL")
local Shader = import("Shader")
local Window = import("Window")
local lib = DLL.Load(FIXTURE)
local window = Window.new({ Title = "Hook", Size = udim.new(200, 100), BackgroundColor = color.new(0, 0, 0, 1) })
local Renderable = window:GetAPI("Renderable")
local State = DLL.Struct({ { "calls", "i32" }, { "vulkan", "i32" }, { "missing", "i32" }, { "width", "f32" } })
local state = State:New()
local shader = Shader.Compile(Shader.Combine({ Shader.Prelude, [==[
struct Params {
    tint: vec4<f32>,
}
@group(1) @binding(0) var<uniform> params: Params;
@group(1) @binding(1) var pattern: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((0x32u >> index) & 1u), f32((0x2cu >> index) & 1u));
    let world = vec2<f32>(10.0, 10.0) + corner * vec2<f32>(80.0, 40.0);
    return vec4<f32>(world.x / frame.resolution.x * 2.0 - 1.0, 1.0 - world.y / frame.resolution.y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if position.x < 50.0 {
        return params.tint;
    }
    return textureLoad(pattern, vec2<i32>(1, 0), 0);
}
]==] }, { Name = "hooked" }))
local hooked = Renderable.new("Renderable", {
    Shaders = { shader },
    VertexCount = 0,
    RenderHook = lib:GetSymbol("render_hook"),
    RenderHookData = state,
})
capture(window, "hooked")
results = state:Read(State)
hookType = typeof(hooked.RenderHook)
hooked.RenderHook = nil
capture(window, "unhooked")
window:Close()
"#
    ));
    let captures: Captures = Arc::default();
    let store = captures.clone();
    let project = Project::load(dir.path()).unwrap();
    let headless = Arc::new(HeadlessWindows::with_rendering());
    let outcome = run_with(Arc::new(project.source_vfs()), "src/main.luau", move |builder| {
        builder.windows(headless).setup(move |lua| {
            let store = store.clone();
            let capture = lua.create_async_function(move |_, (window, label): (AnyUserData, String)| {
                let store = store.clone();
                async move {
                    let capture = Window::capture(&window).await?;
                    store.lock().unwrap().insert(label, capture);
                    Ok(())
                }
            })?;
            lua.globals().set("capture", capture)
        })
    })
    .await;
    assert_eq!(
        outcome.errors,
        ["runtime error: a RenderHook wrote 'missing', but shader 'hooked' has no data named 'missing', it declares params, pattern"]
    );
    let results: Table = outcome.global("results");
    assert!(number(&results, "calls") >= 1.0);
    assert_eq!(number(&results, "vulkan"), 1.0);
    assert_eq!(number(&results, "missing"), -1.0);
    assert_eq!(number(&results, "width"), 200.0);
    assert_eq!(outcome.global::<String>("hookType"), "Pointer");
    let captures = captures.lock().unwrap();
    if let Ok(directory) = std::env::var("LUV_CAPTURE_DIR") {
        for (label, capture) in captures.iter() {
            image::RgbaImage::from_raw(capture.width, capture.height, capture.rgba.clone())
                .unwrap()
                .save(Path::new(&directory).join(format!("dll-{label}.png")))
                .unwrap();
        }
    }
    let pixel = |label: &str, x: u32, y: u32| {
        let capture = &captures[label];
        let index = ((y * capture.width + x) * 4) as usize;
        capture.rgba[index..index + 4].to_vec()
    };
    assert_eq!(pixel("hooked", 30, 25), [255, 0, 255, 255]);
    assert_eq!(pixel("hooked", 70, 25), [0, 255, 0, 255]);
    assert_eq!(pixel("hooked", 150, 80), [0, 0, 0, 255]);
    assert_eq!(pixel("unhooked", 30, 25), [0, 0, 0, 255]);
}

#[test]
fn native_plugins_build_from_sources_and_prebuilt_libraries() {
    let dir = common::workspace(&[("src/main.luau", "print(1)\n")]);
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
    std::fs::copy(fixture(), root.join("native").join(luv::plugins::library_file("prebuilt"))).unwrap();
    std::fs::copy(fixture(), root.join(luv::plugins::library_file("stray"))).unwrap();
    let project = Project::load(root).unwrap();
    let built = luv::plugins::build(&project).unwrap();
    let mut files: Vec<String> = built.iter().map(|library| library.file.clone()).collect();
    files.sort();
    let mut expected = vec![luv::plugins::library_file("math"), luv::plugins::library_file("prebuilt")];
    expected.sort();
    assert_eq!(files, expected);
    assert!(built.iter().all(|library| library.path.starts_with(project.output_dir()) && library.path.is_file()));
    let again = luv::plugins::build(&project).unwrap();
    assert!(again.iter().all(|library| !library.rebuilt));
}

#[tokio::test]
async fn libraries_register_classes_functions_and_events() {
    let outcome = run_dll(
        r#"
local DLL = import("DLL")
local lib = DLL.Load(FIXTURE)
local exports = lib.Exports
local Vec3 = exports.Vec3
local Counter = exports.Counter
results = {}
local a = Vec3.new(1, 2, 3)
local b = Vec3.new(4, 5, 6)
results.kind = typeof(a)
results.counterKind = typeof(Counter.new())
results.text = tostring(a)
results.x = a.X
a.X = 10
results.setX = a.X
results.magnitude = Vec3.new(3, 4, 0).Magnitude
results.readOnly = tostring(select(2, pcall(function() a.Magnitude = 5 end)))
results.unknown = tostring(select(2, pcall(function() return a.Nope end)))
results.dot = a:Dot(b)
results.sum = tostring(a + b)
results.scaled = tostring(a * 2)
results.scaledLeft = tostring(2 * a)
results.negated = tostring(-b)
results.equal = Vec3.new(1, 1, 1) == Vec3.new(1, 1, 1)
results.notEqual = Vec3.new(1, 1, 1) == Vec3.new(1, 2, 1)
results.length = #a
results.zero = tostring(Vec3.zero)
results.fromUDim = tostring(Vec3.fromUDim(udim.new(7, 8, 9)))
results.toUDim = tostring(b:ToUDim())
results.bumpSame = rawequal(b:Bump(), b)
results.bumped = b.X
results.badDot = tostring(select(2, pcall(function() return a:Dot(5) end)))
results.withoutColon = tostring(select(2, pcall(function() return a.Dot(5, b) end)))
results.classText = tostring(Vec3)
results.staticMissing = tostring(select(2, pcall(function() return Vec3.nope end)))
results.readonlyClass = not pcall(function() Vec3.extra = 1 end)
results.noSubtract = tostring(select(2, pcall(function() return a - b end)))
local scaled, workerThread = a:ScaledSlowly(3)
results.slow = tostring(scaled)
results.offThread = workerThread ~= exports.inline_thread()
local name, apiVersion = exports.version()
results.version = name
results.apiVersion = apiVersion
results.failure = tostring(select(2, pcall(exports.fail_on_purpose)))
local list = {}
local first, second, third = exports.echo(list, "s", a)
results.echo = rawequal(first, list) and second == "s" and rawequal(third, a)
results.kinds = table.concat({ exports.describe(nil, true, 1, "x", udim.new(), color.new(), a, DLL.Null, {}) }, ",")
results.badTypes = tostring(select(2, pcall(exports.check_types, "no", 1)))
local className, found = exports.class_of(a)
results.classOf = className
results.foundClass = found
results.rawX = lib:GetFunction("vec3_x_raw", "double", { "pointer" })(a)

local order = {}
coroutine.wrap(function()
    table.insert(order, "slow " .. exports.slow_add(2, 3))
end)()
table.insert(order, "main")
sleep(300)
results.order = table.concat(order, ",")

local ticks = {}
exports.listen(function(index, word)
    table.insert(ticks, word .. index)
end, 3)
sleep(300)
results.ticks = table.concat(ticks, ",")

local counter = Counter.new(5)
results.first = counter:Increment()
results.second = counter:Increment()
results.count = counter.Count
counter = nil
for _ = 1, 3 do
    Counter.new()
end
collectgarbage("collect")
sleep(200)
results.destroyed = lib:GetSymbol("destroyed_counters"):Read("i32")
"#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(text(&results, "kind"), "Vec3");
    assert_eq!(text(&results, "counterKind"), "Counter");
    assert_eq!(text(&results, "text"), "Vec3(1, 2, 3)");
    assert_eq!(number(&results, "x"), 1.0);
    assert_eq!(number(&results, "setX"), 10.0);
    assert_eq!(number(&results, "magnitude"), 5.0);
    assert!(text(&results, "readOnly").contains("Vec3.Magnitude is read only"));
    assert!(text(&results, "unknown").contains("'Nope' is not a valid member of Vec3"));
    assert_eq!(number(&results, "dot"), 68.0);
    assert_eq!(text(&results, "sum"), "Vec3(14, 7, 9)");
    assert_eq!(text(&results, "scaled"), "Vec3(20, 4, 6)");
    assert_eq!(text(&results, "scaledLeft"), "Vec3(20, 4, 6)");
    assert_eq!(text(&results, "negated"), "Vec3(-4, -5, -6)");
    assert!(flag(&results, "equal"));
    assert!(!flag(&results, "notEqual"));
    assert_eq!(number(&results, "length"), 3.0);
    assert_eq!(text(&results, "zero"), "Vec3(0, 0, 0)");
    assert_eq!(text(&results, "fromUDim"), "Vec3(7, 8, 9)");
    assert_eq!(text(&results, "toUDim"), "UDim(4, 5, 6)");
    assert!(flag(&results, "bumpSame"));
    assert_eq!(number(&results, "bumped"), 5.0);
    assert!(text(&results, "badDot").contains("Vec3:Dot: argument #1 must be a Vec3, got number"));
    assert!(text(&results, "withoutColon").contains("Vec3:Dot must be called on a Vec3 with ':'"));
    assert_eq!(text(&results, "classText"), "Vec3");
    assert!(text(&results, "staticMissing").contains("'nope' is not a valid member of Vec3"));
    assert!(flag(&results, "readonlyClass"));
    assert!(text(&results, "noSubtract").contains("Vec3 does not support the __sub operator"));
    assert_eq!(text(&results, "slow"), "Vec3(30, 6, 9)");
    assert!(flag(&results, "offThread"));
    assert_eq!(text(&results, "version"), "fixture");
    assert_eq!(number(&results, "apiVersion"), f64::from(luv::native::API_VERSION));
    assert!(text(&results, "failure").contains("fail_on_purpose: this always fails"));
    assert!(flag(&results, "echo"));
    assert_eq!(text(&results, "kinds"), "0,1,2,3,4,5,6,7,8");
    assert!(text(&results, "badTypes").contains("check_types: argument #1 must be a number, got string"));
    assert_eq!(text(&results, "classOf"), "Vec3");
    assert!(flag(&results, "foundClass"));
    assert_eq!(number(&results, "rawX"), 10.0);
    assert_eq!(text(&results, "order"), "main,slow 5");
    assert_eq!(text(&results, "ticks"), "tick1,tick2,tick3");
    assert_eq!(number(&results, "first"), 6.0);
    assert_eq!(number(&results, "second"), 7.0);
    assert_eq!(number(&results, "count"), 7.0);
    assert!(number(&results, "destroyed") >= 4.0, "destroyed {}", number(&results, "destroyed"));
}

#[tokio::test]
async fn destroying_a_library_lets_go_of_its_exports() {
    let outcome = run_dll(
        r#"
local DLL = import("DLL")
results = {}
local weak = setmetatable({}, { __mode = "v" })

local function load()
    local lib = DLL.Load(FIXTURE)
    weak.exports = lib.Exports
    return lib
end

local lib = load()
results.held = weak.exports ~= nil
lib:Destroy()
collectgarbage()
collectgarbage()
results.released = weak.exports == nil
results.blocked = tostring(select(2, pcall(function() return lib.Exports end)))
"#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!(flag(&results, "held"));
    assert!(flag(&results, "released"), "the Exports table stayed alive after Destroy");
    assert!(text(&results, "blocked").contains("destroyed"));
}

#[tokio::test]
async fn a_service_becomes_an_import_when_the_library_loads() {
    let outcome = run_dll(
        r#"
local DLL = import("DLL")
results = {}
results.before = tostring(select(2, pcall(import, "Fixture")))
local lib = DLL.Load(FIXTURE)
results.listed = table.concat(lib:GetServices(), ",")
local Fixture = import("Fixture")
results.version = Fixture.Version()
results.greet = Fixture.Greet("plugins")
results.greetDefault = Fixture.Greet()
results.level = Fixture.Level
Fixture.Level = 12
results.levelAfter = Fixture.Level
results.frozen = tostring(select(2, pcall(function() Fixture.Version = 1 end)))
results.missing = tostring(select(2, pcall(function() return Fixture.Nope end)))
"#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert!(text(&results, "before").contains("'Fixture' cannot be imported"));
    assert_eq!(text(&results, "listed"), "Fixture");
    assert_eq!(number(&results, "version"), f64::from(luv::native::API_VERSION));
    assert_eq!(text(&results, "greet"), "hello plugins");
    assert_eq!(text(&results, "greetDefault"), "hello world");
    assert_eq!(number(&results, "level"), 1.0);
    assert_eq!(number(&results, "levelAfter"), 12.0);
    assert!(text(&results, "frozen").contains("readonly"));
    assert!(text(&results, "missing").contains("'Nope' is not a valid member of Fixture"));
}

#[tokio::test]
async fn a_library_makes_signals_and_functions_for_luau() {
    let outcome = run_dll(
        r#"
local DLL = import("DLL")
local Process = import("Process")
local lib = DLL.Load(FIXTURE)
local function pause(beats)
    for index = 1, beats do
        Process.Heartbeat:Wait()
    end
end
results = {}
local signal = lib.Exports.make_signal()
results.class = signal.ClassName
results.bound = signal:IsBound("native")
signal:Fire(21)
pause(2)
results.heard = lib.Exports.heard()
results.reported = lib.Exports.call_member(signal)
local _, found = lib.Exports.call_member(signal)
results.found = found
lib.Exports.release_signal()

local double = lib.Exports.make_function()
results.doubleType = typeof(double)
results.doubled = double(20)

local made = lib.Exports.imported()
results.madeClass = made.ClassName
"#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(text(&results, "class"), "Signal");
    assert!(flag(&results, "bound"));
    assert_eq!(number(&results, "heard"), 21.0);
    assert_eq!(number(&results, "reported"), 1.0);
    assert!(flag(&results, "found"));
    assert_eq!(text(&results, "doubleType"), "function");
    assert_eq!(number(&results, "doubled"), 40.5);
    assert_eq!(text(&results, "madeClass"), "Signal");
}

#[tokio::test]
async fn a_library_reads_and_writes_objects_and_globals() {
    let outcome = run_dll(
        r#"
local DLL = import("DLL")
local Process = import("Process")
local Signal = import("Signal")
local lib = DLL.Load(FIXTURE)
local function pause(beats)
    for index = 1, beats do
        Process.Heartbeat:Wait()
    end
end
results = {}
local signal = Signal.new()
signal.Name = "before"
local was, code = lib.Exports.members(signal)
results.was = was
results.code = code
results.now = signal.Name

results.setCode = select(1, lib.Exports.globals())
local _, greeting = lib.Exports.globals()
results.greeting = greeting
results.global = pluginGreeting

local posted, fired, offThread = lib.Exports.post_name(signal)
pause(2)
results.posted = posted
results.fired = fired
results.offThread = offThread
results.postedName = signal.Name
"#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(text(&results, "was"), "before");
    assert_eq!(number(&results, "code"), 0.0);
    assert_eq!(text(&results, "now"), "renamed");
    assert_eq!(number(&results, "setCode"), 0.0);
    assert_eq!(text(&results, "greeting"), "from the plugin");
    assert_eq!(text(&results, "global"), "from the plugin");
    assert_eq!(number(&results, "posted"), 0.0);
    assert_eq!(number(&results, "fired"), 0.0);
    assert!(flag(&results, "offThread"), "a worker call should not be on the game thread");
    assert_eq!(text(&results, "postedName"), "posted");
}

#[tokio::test]
async fn a_library_makes_buffers_and_runs_on_a_timer() {
    let outcome = run_dll(
        r#"
local DLL = import("DLL")
local Process = import("Process")
local lib = DLL.Load(FIXTURE)
local function pause(beats)
    for index = 1, beats do
        Process.Heartbeat:Wait()
    end
end
results = {}
local samples = lib.Exports.samples(8)
results.kind = typeof(samples)
results.length = buffer.len(samples)
results.first = buffer.readu8(samples, 0)
results.last = buffer.readu8(samples, 7)
results.empty = buffer.len(lib.Exports.samples(0))

local seen = {}
results.started = lib.Exports.start_ticks(function(count)
    table.insert(seen, count)
end)
pause(6)
results.ticks = lib.Exports.stop_ticks()
pause(2)
results.seen = #seen
local after = lib.Exports.stop_ticks()
pause(4)
results.stopped = lib.Exports.stop_ticks() == after
"#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(text(&results, "kind"), "buffer");
    assert_eq!(number(&results, "length"), 8.0);
    assert_eq!(number(&results, "first"), 1.0);
    assert_eq!(number(&results, "last"), 22.0);
    assert_eq!(number(&results, "empty"), 0.0);
    assert!(flag(&results, "started"));
    assert!(number(&results, "ticks") >= 2.0, "ticks {}", number(&results, "ticks"));
    assert!(number(&results, "seen") >= 2.0, "seen {}", number(&results, "seen"));
    assert!(flag(&results, "stopped"), "the task kept running after cancel");
}

#[tokio::test]
async fn a_library_makes_a_renderable_in_a_window() {
    let dir = main_script(&format!(
        "local FIXTURE = \"{}\"\n{}",
        lua_path(&fixture()),
        r#"
local DLL = import("DLL")
local Window = import("Window")
local lib = DLL.Load(FIXTURE)
local window = Window.new({ Title = "Native", Size = udim.new(200, 100) })
local shape = lib.Exports.window_shape(window)
results = {}
results.class = shape.ClassName
results.width = shape.Size.X
results.height = shape.Size.Y
results.listed = #window:GetAPI("Renderable").GetRenderables()
window:Close()
"#
    ));
    let project = Project::load(dir.path()).unwrap();
    let headless = Arc::new(HeadlessWindows::new());
    let outcome = run_with(Arc::new(project.source_vfs()), "src/main.luau", move |builder| {
        builder.windows(headless)
    })
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    assert_eq!(text(&results, "class"), "RenderableShape");
    assert_eq!(number(&results, "width"), 64.0);
    assert_eq!(number(&results, "height"), 48.0);
    assert_eq!(number(&results, "listed"), 1.0);
}
