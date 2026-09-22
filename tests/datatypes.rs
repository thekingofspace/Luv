mod common;

use common::{main_script, run_both, run_source};
use mlua::Table;

async fn run_script(source: &str) -> common::Outcome {
    let dir = main_script(source);
    run_source(dir.path()).await
}

#[tokio::test]
async fn udim_holds_x_y_and_layout_z() {
    let outcome = run_script(
        r#"
        local a = udim.new(10, 20, 1)
        local b = udim.new(1, 2, 3)
        results = {
            fields = `{a.X},{a.Y},{a.Z}`,
            defaults = tostring(udim.new()),
            zero = udim.zero == udim.new(0, 0, 0),
            added = tostring(a + b),
            subtracted = tostring(a - b),
            multiplied = tostring(a * b),
            scaled = tostring(a * 2),
            scaledLeft = tostring(2 * a),
            divided = tostring(a / 2),
            dividedBy = tostring(a / b),
            negated = tostring(-b),
            equal = udim.new(1, 2, 3) == b,
            notEqual = a ~= b,
            lerp = tostring(udim.new(0, 0, 0):Lerp(udim.new(10, 20, 30), 0.5)),
            typeName = typeof(a),
            immutable = not pcall(function() a.X = 5 end),
            badMath = not pcall(function() return a + 1 end),
        }
        "#,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap();
    assert_eq!(text("fields"), "10,20,1");
    assert_eq!(text("defaults"), "UDim(0, 0, 0)");
    assert_eq!(text("added"), "UDim(11, 22, 4)");
    assert_eq!(text("subtracted"), "UDim(9, 18, -2)");
    assert_eq!(text("multiplied"), "UDim(10, 40, 3)");
    assert_eq!(text("scaled"), "UDim(20, 40, 2)");
    assert_eq!(text("scaledLeft"), "UDim(20, 40, 2)");
    assert_eq!(text("divided"), "UDim(5, 10, 0.5)");
    assert_eq!(text("dividedBy"), "UDim(10, 10, 0.3333333333333333)");
    assert_eq!(text("negated"), "UDim(-1, -2, -3)");
    assert_eq!(text("lerp"), "UDim(5, 10, 15)");
    assert_eq!(text("typeName"), "UDim");
    for key in ["zero", "equal", "notEqual", "immutable", "badMath"] {
        assert!(results.get::<bool>(key).unwrap(), "{key}");
    }
}

#[tokio::test]
async fn color_is_rgba_with_conversions() {
    let outcome = run_script(
        r##"
        local orange = color.fromRGB(255, 128, 0)
        local translucent = color.new(1, 0, 0, 0.5)
        local r, g, b, a = orange:ToRGB()
        local h, s, v = color.fromRGB(0, 255, 0):ToHSV()
        results = {
            fields = `{translucent.R},{translucent.G},{translucent.B},{translucent.A}`,
            defaultAlpha = color.new(0.2, 0.4, 0.6).A,
            rgb = `{r},{g},{b},{a}`,
            hex = orange:ToHex(),
            hexAlpha = translucent:ToHex(true),
            fromHex = color.fromHex("#ff8000") == orange,
            fromShortHex = color.fromHex("f00") == color.new(1, 0, 0, 1),
            fromHexAlpha = color.fromHex("#ff000080"):ToHex(true),
            hsv = `{h},{s},{v}`,
            fromHSV = color.fromHSV(1 / 3, 1, 1):ToHex(),
            added = tostring(color.new(0.25, 0, 0, 0.5) + color.new(0.25, 0.5, 0, 0.5)),
            darkened = tostring(color.new(1, 1, 1, 0.5) * 0.5),
            tinted = tostring(color.new(1, 0.5, 1, 1) * color.new(0.5, 0.5, 0, 1)),
            lerp = color.black:Lerp(color.white, 0.5):ToHex(),
            constants = color.white:ToHex() .. color.black:ToHex() .. color.transparent:ToHex(true),
            typeName = typeof(orange),
            badHex = not pcall(color.fromHex, "#12"),
            immutable = not pcall(function() orange.R = 0 end),
        }
        "##,
    )
    .await;
    outcome.assert_clean();
    let results: Table = outcome.global("results");
    let text = |key: &str| results.get::<String>(key).unwrap();
    assert_eq!(text("fields"), "1,0,0,0.5");
    assert_eq!(results.get::<f64>("defaultAlpha").unwrap(), 1.0);
    assert_eq!(text("rgb"), "255,128,0,255");
    assert_eq!(text("hex"), "#ff8000");
    assert_eq!(text("hexAlpha"), "#ff000080");
    assert_eq!(text("fromHexAlpha"), "#ff000080");
    assert_eq!(text("hsv"), "0.3333333333333333,1,1");
    assert_eq!(text("fromHSV"), "#00ff00");
    assert_eq!(text("added"), "Color(0.5, 0.5, 0, 1)");
    assert_eq!(text("darkened"), "Color(0.5, 0.5, 0.5, 0.5)");
    assert_eq!(text("tinted"), "Color(0.5, 0.25, 0, 1)");
    assert_eq!(text("lerp"), "#808080");
    assert_eq!(text("constants"), "#ffffff#000000#00000000");
    assert_eq!(text("typeName"), "Color");
    for key in ["fromHex", "fromShortHex", "badHex", "immutable"] {
        assert!(results.get::<bool>(key).unwrap(), "{key}");
    }
}

#[tokio::test]
async fn datatypes_cross_threads_by_value() {
    let dir = main_script(
        r#"
local Messenger = import("Messenger")
local offset = udim.new(4, 5, 6)
local tint = color.fromRGB(10, 20, 30)

Messenger:Subscribe("Local", function(position, tone, nested)
    localResult = {
        position = position == udim.new(1, 2, 3),
        color = tone == color.new(1, 0, 0, 1),
        nested = nested.layout == udim.new(7, 8, 9) and nested.colors[1] == color.white,
    }
end)
Messenger:Fire("Local", udim.new(1, 2, 3), color.new(1, 0, 0, 1), { layout = udim.new(7, 8, 9), colors = { color.white } })

Messenger:Subscribe("Parallel", function(moved, shaded, thread)
    parallelResult = { moved = tostring(moved), shaded = shaded:ToHex(), thread = thread }
end)

EnterParallel()
local moved = offset + udim.new(1, 1, 1)
local shaded = tint * 2
Messenger:Fire("Parallel", moved, shaded, threadName())
ExitParallel()
"#,
    );
    for outcome in run_both(dir.path()).await {
        outcome.assert_clean();
        let local: Table = outcome.global("localResult");
        for key in ["position", "color", "nested"] {
            assert!(local.get::<bool>(key).unwrap(), "{key}");
        }
        let parallel: Table = outcome.global("parallelResult");
        assert_eq!(parallel.get::<String>("moved").unwrap(), "UDim(5, 6, 7)");
        assert_eq!(parallel.get::<String>("shaded").unwrap(), "#14283c");
        assert_eq!(parallel.get::<String>("thread").unwrap(), "parallel block #1 of src/main.luau");
    }
}

#[tokio::test]
async fn serde_encodes_datatypes_as_tables() {
    let outcome = run_script(
        r#"
        local Serde = import("Serde")
        encoded = Serde.Encode("json", { size = udim.new(1, 2, 3), tint = color.new(1, 0.5, 0, 1) })
        "#,
    )
    .await;
    outcome.assert_clean();
    assert_eq!(
        outcome.global::<String>("encoded"),
        r#"{"size":{"X":1,"Y":2,"Z":3},"tint":{"A":1,"B":0,"G":0.5,"R":1}}"#
    );
}
