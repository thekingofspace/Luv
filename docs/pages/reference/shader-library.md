# Shader

Compiles shaders for renderables and post processing.

```luau
local Shader = import("Shader")
```

## Description

The Shader library turns WGSL, GLSL or SPIR-V code into [Shader](shader.md) objects. You load a Shader on a renderable with [LoadShader](renderable.md#loadshader), or you pass it to [window:AddPostProcess](window.md#addpostprocess).

Compiling runs on a worker thread. It yields only the calling coroutine. It does not need a window or a GPU.

luv does not add anything to your code. To use the engine bindings, structs and helpers, put [Prelude](#prelude) in front of your WGSL code with [Combine](#combine) or with `..`.

For a guide, see [Shaders](../manual/shaders.md).

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [Compile](#compile)(source) | [Shader](shader.md) | yes |
| [Compile](#compile)(sources) | `{ Shader }` | yes |
| [Compile](#compile)(sources, callback) | nothing | no |
| [Combine](#combine)(parts, options) | [ShaderCombo](shadercombo.md) | yes |

The library also has one value, [Prelude](#prelude). It is a string of WGSL code.

## Function descriptions

### Compile

```luau
Shader.Compile(source: ShaderSource): Shader
Shader.Compile(sources: { ShaderSource }): { Shader }
Shader.Compile(sources: ShaderSource | { ShaderSource }, callback: (shader: Shader, index: number) -> ())
```

Compiles shader code. See [ShaderSource](#shadersource) for what you can pass.

- With one source, it returns one [Shader](shader.md).
- With a list, it compiles them all at the same time and returns a list in the same order. A table counts as a list when it has no `Source` key.
- With a callback, it returns at once. It calls the callback for each shader when that shader is ready. `index` is the place of the source in the list, starting at 1. The calls can come in any order.

A compile error does not raise an error. The Shader comes back with `Compiled` set to `false` and the message in `Error`. The message names the source and the line. Always check `Compiled`.

It raises an error when an argument is wrong:

| Message | Cause |
| --- | --- |
| `bad shader source (expected string, buffer, Asset, File, ShaderCombo or options table, got number)` | The source is not a kind of source that luv knows. |
| `shader options need a Source` | An options table has no `Source`. |
| `unknown shader language 'hlsl', expected "wgsl", "glsl" or "spirv"` | `Language` is not a known language. |
| `unknown shader stage 'geometry', expected "vertex", "fragment" or "compute"` | `Stage` is not a known stage. |

One source:

```luau
local Shader = import("Shader")
local Asset = import("Asset")

local glow = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/glow.wgsl") }, { Name = "glow" }))
if not glow.Compiled then
	print(glow.Error)
end
```

A list:

```luau
local Shader = import("Shader")
local Asset = import("Asset")

local sources: { ShaderSource } = { Asset.Load("shaders/sky.wgsl"), Asset.Load("shaders/solid.frag") }
for _, shader in Shader.Compile(sources) do
	print(shader.Name, shader.Compiled)
end
```

A callback:

```luau
local Shader = import("Shader")
local Asset = import("Asset")

local sources: { ShaderSource } = { Asset.Load("shaders/sky.wgsl"), Asset.Load("shaders/water.wgsl") }
Shader.Compile(sources, function(shader: Shader, index: number)
	print(index, shader.Name, shader.Compiled)
end)
print("This prints first")
```

### Combine

```luau
Shader.Combine(parts: { ShaderSource }, options: ShaderComboOptions?): ShaderCombo
```

Joins several parts into one [ShaderCombo](shadercombo.md). Pass the combo to [Compile](#compile). A combo can also be a part of another combo. It yields the calling coroutine while it reads files.

Each part can be:

- A string of code. A string that equals [Prelude](#prelude) gets the name `prelude`. Other strings get the names `part 1`, `part 2` and so on, by their place in the list.
- An [Asset](asset.md), a [File](file.md) or an options table. luv reads its code and names the part after the file.
- A [ShaderCombo](shadercombo.md). Its parts are added one by one.

luv joins the parts in order. It also:

- Drops a part when its code is the same as an earlier part. So adding the prelude twice is fine.
- Moves WGSL `enable`, `requires` and `diagnostic` lines to the top, once each.
- Moves GLSL `#version` and `#extension` lines to the top. Only one `#version` is kept.

A compile error in a combo names the part and the line inside that part. The message starts like `shaders/broken.wgsl:3:12:`, followed by the error and the line of code.

The language of the combo comes from `options.Language`, then from the first part that is a file, then it is WGSL. Strings do not set the language.

| Message | Cause |
| --- | --- |
| `combo 'combo' needs at least one shader part` | The list is empty. |
| `'shaders/tint.frag' is glsl but the combo is wgsl, every part of a combo must use the same language` | The parts mix languages. |
| `'shader' is SPIR-V, which cannot be combined, combine WGSL or GLSL sources instead` | A part is SPIR-V. |

```luau
local Shader = import("Shader")
local Asset = import("Asset")

local combo = Shader.Combine({ Shader.Prelude, Asset.Load("shaders/neon.wgsl") }, { Name = "neon" })
print(combo.Parts[1], combo.Parts[2])
local neon = Shader.Compile(combo)
print(neon.Name, neon.Compiled)
```

The parts are named `prelude` and `shaders/neon.wgsl`. The compiled Shader is named `neon`.

## Prelude

```luau
Shader.Prelude: string
```

WGSL code that declares the engine bindings, structs, constants and helper functions. Put it in front of your WGSL code. It only works with WGSL.

```luau
local Shader = import("Shader")

local red = Shader.Compile(Shader.Prelude .. [[
@fragment
fn red(in: VertexOutput) -> @location(0) vec4<f32> {
	return vec4<f32>(1.0, 0.0, 0.0, in.color.a);
}
]])
print(red.Compiled)
```

### Frame

The `frame` binding. It is the same for everything drawn in one frame.

| Field | Type | Description |
| --- | --- | --- |
| `resolution` | `vec2<f32>` | The size of the window in window pixels. |
| `scale` | `f32` | Screen pixels per window pixel. |
| `time` | `f32` | Seconds since the window opened. |
| `delta` | `f32` | Seconds since the last frame. |
| `frame` | `u32` | The frame number. It wraps around to 0. |
| `padding` | `vec2<f32>` | Not used. |

### Instance

One entry of the `instances` binding. luv makes one instance for each box it draws for a shape, an image or text. Text makes one instance for each letter, and more for its background, outline and lines.

| Field | Type | Description |
| --- | --- | --- |
| `position` | `vec2<f32>` | Where the anchor point of the box sits, in window pixels. |
| `size` | `vec2<f32>` | The size of the box in window pixels. |
| `anchor` | `vec2<f32>` | The anchor point of the box. |
| `rotation` | `f32` | The rotation in radians. |
| `kind` | `u32` | `0` for a shape box, `1` for an image, `2` for a letter. The background and the lines of text are shape boxes. |
| `color` | `vec4<f32>` | The color of the box. For text it is Color for letters, StrokeColor for outlines and BackgroundColor for the background. |
| `stroke_color` | `vec4<f32>` | The StrokeColor of a shape. |
| `uv` | `vec4<f32>` | The part of the texture to draw, as left, top, right and bottom. |
| `shape` | `u32` | The shape as a `SHAPE_` constant. |
| `stroke` | `f32` | The StrokeThickness of a shape. |
| `flags` | `u32` | `flags & 1u` is not 0 for a Pixelated image. |
| `object` | `u32` | The index of the renderable in `objects`. |

### Object

One entry of the `objects` binding. There is one for each renderable that is not hidden.

| Field | Type | Description |
| --- | --- | --- |
| `position` | `vec2<f32>` | The Position. |
| `size` | `vec2<f32>` | The size. For text this is the size of its box. |
| `anchor` | `vec2<f32>` | The AnchorPoint. |
| `rotation` | `f32` | The Rotation in radians. |
| `shape` | `u32` | The `SHAPE_` constant. Images and text use `SHAPE_RECTANGLE`. A custom Renderable uses `NO_SHAPE`. |
| `color` | `vec4<f32>` | The Color. It is 0 for a custom Renderable. |
| `kind` | `u32` | A `KIND_` constant. |
| `z_index` | `f32` | The ZIndex. |
| `id_low`, `id_high` | `u32` | The id of the renderable in two halves. It matches `renderable` in a render hook. |

### VertexOutput

What the built in vertex stage gives your fragment entry point.

| Field | Type | Description |
| --- | --- | --- |
| `position` | `vec4<f32>` | `@builtin(position)`. In the fragment stage, it is in screen pixels. |
| `uv` | `vec2<f32>` | `@location(0)`. The texture position. It goes from 0 to 1 across a shape. |
| `color` | `vec4<f32>` | `@location(1)`. The color of the box. |
| `local` | `vec2<f32>` | `@location(2)`. The point inside the box in window pixels. The center is 0 and Rotation is not applied. |
| `size` | `vec2<f32>` | `@location(3)`. The size of the box in window pixels. |
| `instance` | `u32` | `@location(4)`, flat. The index of the box in `instances`. |

In a post process, `uv` goes from (0, 0) at the top left of the window to (1, 1) at the bottom right. `color` is white. `local` is the point in window pixels from the top left corner. `size` is `frame.resolution` and `instance` is 0.

### Constants

| Name | Value | Meaning |
| --- | --- | --- |
| `NO_OBJECT` | `0xffffffffu` | An object index that points at nothing. |
| `NO_SHAPE` | `0xffffffffu` | The shape of something that has no outline. |
| `SHAPE_RECTANGLE` | `0u` | Rectangle. |
| `SHAPE_CIRCLE` | `1u` | Circle. |
| `SHAPE_TRIANGLE` | `2u` | Triangle. |
| `SHAPE_RIGHT_TRIANGLE` | `3u` | RightTriangle. |
| `SHAPE_DIAMOND` | `4u` | Diamond. |
| `SHAPE_PENTAGON` | `5u` | Pentagon. |
| `SHAPE_HEXAGON` | `6u` | Hexagon. |
| `SHAPE_OCTAGON` | `7u` | Octagon. |
| `KIND_NONE` | `0u` | An empty entry in `objects`. |
| `KIND_RENDERABLE` | `1u` | A custom Renderable. |
| `KIND_SHAPE` | `2u` | A RenderableShape. |
| `KIND_IMAGE` | `3u` | A RenderableImage. |
| `KIND_TEXT` | `4u` | A RenderableText. |

The `SHAPE_` values match the `Value` of each [ShapeType](enums.md#shapetype) item.

### Bindings

The prelude declares these bindings in `@group(0)`. Group 0 belongs to luv. Put your own data in groups 1 to 3.

| Binding | Declaration | What it holds |
| --- | --- | --- |
| 0 | `var<uniform> frame: Frame` | The [Frame](#frame) info. |
| 1 | `var<storage, read> instances: array<Instance>` | Every box luv draws for shapes, images and text in this frame. |
| 2 | `var image: texture_2d<f32>` | The image of a RenderableImage. For text, the texture of the letters, with the coverage in `.r`. A 1x1 white texture for shapes and custom renderables. The picture from the pass before in a post process. |
| 3 | `var image_sampler: sampler` | A smooth sampler that clamps at the edges. |
| 4 | `var<storage, read> objects: array<Object>` | Every renderable that is not hidden. |
| 5 | `var backdrop: texture_2d<f32>` | What was drawn before this renderable. See [Reading the backdrop](../manual/shaders.md#reading-the-backdrop). In a post process it is the same as `image`. |

`SHAPE_POINTS` is a private array with the outline points of every shape except Circle, for a size of 1.

### Helper functions

| Function | Returns | Description |
| --- | --- | --- |
| `shape_range(shape: u32)` | `vec2<u32>` | The first index and the number of outline points of a shape in `SHAPE_POINTS`. Circle has none. |
| `shape_point(shape: u32, index: u32, size: vec2<f32>)` | `vec2<f32>` | Outline point `index` of a shape, scaled to `size`. The center is 0. |
| `rotate2d(value: vec2<f32>, angle: f32)` | `vec2<f32>` | Turns a point around 0 by `angle` radians. |
| `world_position(fragment: vec4<f32>)` | `vec2<f32>` | Turns `@builtin(position)` of the fragment stage into window pixels. |
| `object_local(object: Object, point: vec2<f32>)` | `vec2<f32>` | Turns a window point into the space of an object. The center of the object is 0 and its rotation is removed. |
| `object_world(object: Object, local: vec2<f32>)` | `vec2<f32>` | The reverse of `object_local`. |
| `shape_distance(shape: u32, size: vec2<f32>, point: vec2<f32>)` | `f32` | The distance from a point to the edge of a shape. The point is centered like `local`. The distance is below 0 inside the shape. `NO_SHAPE` gives `1e30`. |
| `object_exists(index: u32)` | `bool` | `true` when `objects[index]` holds a renderable. |
| `object_distance(index: u32, point: vec2<f32>)` | `f32` | The distance from a window point to the edge of `objects[index]`. It is below 0 inside and `1e30` when the object does not exist. |
| `object_contains(index: u32, point: vec2<f32>)` | `bool` | `true` when a window point is inside `objects[index]`. |

## ShaderSource

Anything [Compile](#compile) and [Combine](#combine) accept as a source.

| Source | Language | Name |
| --- | --- | --- |
| `string` | WGSL | `"shader"` |
| `buffer` | SPIR-V | `"shader"` |
| [Asset](asset.md) | From the extension | The asset path, like `"shaders/glow.wgsl"` |
| [File](file.md) | From the extension | The name of the file |
| [ShaderCombo](shadercombo.md) | The language of the combo | The name of the combo |
| Options table | See below | See below |

The extension of an Asset or a File sets the language and the stage:

| Extension | Language | Stage |
| --- | --- | --- |
| `.wgsl` | WGSL | none |
| `.glsl` | GLSL | none. Set `Stage` in an options table. |
| `.vert` | GLSL | vertex |
| `.frag` | GLSL | fragment |
| `.comp` | GLSL | compute |
| `.spv` | SPIR-V | none |
| Anything else | WGSL | none |

An options table wraps a source and changes how it is read:

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Source` | `string`, `buffer`, [Asset](asset.md), [File](file.md) or [ShaderCombo](shadercombo.md) | required | The code. |
| `Name` | `string` | from the source | The name used in error messages. |
| `Language` | `string` | from the source | `"wgsl"`, `"glsl"` or `"spirv"`. `"spv"` and `"spir-v"` work too. |
| `Stage` | `string` | from the source | `"vertex"`, `"fragment"` or `"compute"`. Only GLSL uses it. |

GLSL code needs a stage. Without one, the Shader gets an `Error` like `solid.glsl is GLSL, so it needs a Stage of "vertex", "fragment" or "compute"`. A `.frag` file has its stage in the extension. A `.glsl` file gets it from an options table, or from the [options of Combine](#shadercombooptions):

```luau
local Shader = import("Shader")
local Asset = import("Asset")

local frag = Shader.Compile(Asset.Load("shaders/solid.frag"))
local glsl = Shader.Compile(Shader.Combine({ Asset.Load("shaders/solid.glsl") }, { Language = "glsl", Stage = "fragment", Name = "solid" }))
print(frag.Language, glsl.Name)
```

## ShaderComboOptions

The options of [Combine](#combine).

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Name` | `string?` | `"combo"` | The name of the combo. The compiled Shader gets this name. |
| `Language` | `string?` | from the parts, or `"wgsl"` | `"wgsl"` or `"glsl"`. Every file part must match it. |
| `Stage` | `string?` | from the first part that has one | The stage for GLSL, like `"fragment"`. |

## ShaderValue

Any value that [WriteShaderData](renderable.md#writeshaderdata) accepts. Which one fits depends on the type in the shader. The full list is in [WriteShaderData](renderable.md#writeshaderdata).

| Luau value | Used for |
| --- | --- |
| `number` | Number fields. |
| `boolean` | Number fields. `true` is 1 and `false` is 0. |
| `string` | Arrays of `u32` or `i32`, and sampler modes. |
| `buffer` | Raw bytes for any buffer field. |
| `vector`, [UDim](udim.md), [Color](color.md) | Vector fields. |
| [EnumItem](enums.md#enumitem) | Number fields get its `Value`. Samplers take a [ResampleMode](enums.md#resamplemode) item. |
| [Asset](asset.md) | Texture bindings. |
| [Renderable](renderable.md) | `u32` and `i32` fields. See [Referencing another renderable](../manual/shaders.md#referencing-another-renderable). |
| table | Arrays, structs and matrices. |
