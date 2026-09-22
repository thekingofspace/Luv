# Shaders

luv draws shapes, images and text with its own built in shaders. You can load your own shaders on any renderable to change how it looks, or to draw something new. This page uses WGSL. luv also takes GLSL and SPIR-V.

## Compiling a shader

Keep shader files in your assets, for example `assets/shaders/pulse.wgsl`. Compile them with the [Shader](../reference/shader-library.md) library:

```luau
local Shader = import("Shader")
local Asset = import("Asset")

local pulse = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/pulse.wgsl") }, { Name = "pulse" }))
if not pulse.Compiled then
	print(pulse.Error)
end
```

`Shader.Compile` yields the calling coroutine while it works. A mistake in the code does not raise an error. The [Shader](../reference/shader.md) comes back with `Compiled` set to `false` and the message in `Error`.

## The prelude

`Shader.Prelude` is WGSL code that declares what luv gives every shader:

- The `frame` binding with the window size, the time and the frame delta.
- The `instances` and `objects` bindings with the boxes and renderables luv draws.
- The `image`, `image_sampler` and `backdrop` textures.
- The `VertexOutput` struct that the built in vertex stage fills in.
- Helper functions like `shape_distance`, `world_position` and `object_contains`.

luv does not add the prelude for you. Put it first with [Shader.Combine](../reference/shader-library.md#combine), like above, or with `Shader.Prelude .. code`. With Combine, error messages name the file and the line inside that file. See [Prelude](../reference/shader-library.md#prelude) for every struct, binding and helper.

## A fragment shader on a shape

When a shader has only a fragment entry point, luv keeps its own vertex stage. Your fragment entry point then gets a `VertexOutput` for each pixel of the box. This shader makes a shape pulse:

```wgsl
struct Pulse {
    speed: f32,
    amount: f32,
}
@group(1) @binding(0) var<uniform> pulse: Pulse;

@fragment
fn pulse_fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let item = instances[in.instance];
    let pixel = max(length(fwidth(in.local)) * 0.70710678, 1e-4);
    let coverage = clamp(0.5 - shape_distance(item.shape, in.size, in.local) / pixel, 0.0, 1.0);
    let wave = 0.5 + 0.5 * sin(frame.time * pulse.speed);
    return vec4<f32>(mix(in.color.rgb, vec3<f32>(1.0), wave * pulse.amount), in.color.a * coverage);
}
```

Load it with the `Shaders` config key:

```luau
local Window = import("Window")
local Shader = import("Shader")
local Asset = import("Asset")

local window = Window.new({ Title = "Pulse" })
local Renderable = window:GetAPI("Renderable")
local pulse = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/pulse.wgsl") }, { Name = "pulse" }))

local button = Renderable.new("RenderableShape", {
	Shape = enum.ShapeType.Octagon,
	Position = udim.new(200, 150),
	Color = color.new(0.55, 0.3, 1, 1),
	Shaders = { pulse },
})
button:WriteShaderData(pulse, { speed = 4, amount = 0.5 })
```

Some things to know:

- The fragment stage runs for the whole box around the shape. Use `shape_distance` to keep the outline, like above.
- Your shader replaces the stroke too. Draw it yourself when you need it.
- `in.local` is the pixel inside the box in window pixels, with 0 at the center. `in.size` is the size of the box. `in.color` is the Color of the renderable.
- luv blends what you return with the BlendMode of the renderable. You do not need to multiply the color by its alpha.
- On a RenderableImage, sample `image` with `image_sampler` at `in.uv`.
- On a RenderableText, the fragment stage runs for the box of each letter, and for the boxes of the background, the outline and the lines. The coverage of a letter is in the `.r` of `image`.

## Shader data

Put your own data in `@group(1)`, `@group(2)` or `@group(3)`. Group 0 belongs to luv. Write the data from Luau with [WriteShaderData](../reference/renderable.md#writeshaderdata). Each renderable keeps its own copy, and everything starts at 0.

### A uniform struct

```wgsl
struct Settings {
    tint: vec4<f32>,
    offset: vec2<f32>,
    strength: f32,
}
@group(1) @binding(0) var<uniform> settings: Settings;
```

You can write one member, a path to a member, or many members at once:

```luau
local Window = import("Window")
local Shader = import("Shader")
local Asset = import("Asset")

local window = Window.new({ Title = "Data" })
local Renderable = window:GetAPI("Renderable")
local glow = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/glow.wgsl") }, { Name = "glow" }))
local box = Renderable.new("RenderableShape", { Position = udim.new(200, 150), Shaders = { glow } })

box:WriteShaderData(glow, "strength", 0.5)
box:WriteShaderData(glow, "settings.offset", udim.new(4, 4))
box:WriteShaderData(glow, { tint = color.new(1, 0.5, 0, 1), strength = 1 })
```

A `vec2` takes a UDim and a `vec4` takes a Color. Numbers, booleans, tables and buffers work too. See [WriteShaderData](../reference/renderable.md#writeshaderdata) for every type. To write many renderables in one call, use [Bulk.BulkWriteShaderData](../reference/bulk.md#bulkwriteshaderdata).

### A storage array

An array without a length grows or shrinks to the number of items you write. This shader lights a floor with any number of lights:

```wgsl
struct Light {
    position: vec2<f32>,
    radius: f32,
    color: vec4<f32>,
}
@group(1) @binding(0) var<storage, read> lights: array<Light>;

@fragment
fn lit(in: VertexOutput) -> @location(0) vec4<f32> {
    let point = world_position(in.position);
    var light = vec3<f32>(0.1);
    for (var index = 0u; index < arrayLength(&lights); index++) {
        let item = lights[index];
        let fade = clamp(1.0 - distance(point, item.position) / item.radius, 0.0, 1.0);
        light += item.color.rgb * fade;
    }
    return vec4<f32>(in.color.rgb * light, in.color.a);
}
```

Each item of the array is a table:

```luau
local Window = import("Window")
local Shader = import("Shader")
local Asset = import("Asset")

local window = Window.new({ Title = "Lights", Size = udim.new(800, 450) })
local Renderable = window:GetAPI("Renderable")
local lighting = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/lights.wgsl") }, { Name = "lights" }))
local floor = Renderable.new("RenderableShape", { Position = udim.new(400, 225), Size = window.Size, Shaders = { lighting } })

floor:WriteShaderData(lighting, {
	lights = {
		{ position = udim.new(200, 200), radius = 180, color = color.new(1, 0.8, 0.5, 1) },
		{ position = udim.new(600, 250), radius = 120, color = color.new(0.3, 0.5, 1, 1) },
	},
})
```

An array with a length, like `array<u32, 32>`, takes up to that many items. The rest become 0. A string fills an array of `u32` with the code of each character.

## Textures and samplers

A `texture_2d<f32>` binding takes an image [Asset](../reference/asset.md). A `sampler` binding takes a [ResampleMode](../reference/enums.md#resamplemode) item, or `"Smooth"` or `"Pixelated"`.

```wgsl
@group(2) @binding(0) var pattern: texture_2d<f32>;
@group(2) @binding(1) var pattern_sampler: sampler;

@fragment
fn patterned(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(pattern, pattern_sampler, fract(in.uv * 4.0));
    return vec4<f32>(texel.rgb * in.color.rgb, in.color.a);
}
```

Write the texture and the sampler like any other data:

```luau
local Window = import("Window")
local Shader = import("Shader")
local Asset = import("Asset")

local window = Window.new({ Title = "Tiles" })
local Renderable = window:GetAPI("Renderable")
local tiles = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/tiles.wgsl") }, { Name = "tiles" }))
local wall = Renderable.new("RenderableShape", { Position = udim.new(200, 150), Size = udim.new(256, 256), Shaders = { tiles } })

wall:WriteShaderData(tiles, "pattern", Asset.Load("textures/bricks.png"))
wall:WriteShaderData(tiles, "pattern_sampler", "Pixelated")
```

- A texture that was never written, or was set to `nil`, is a 1x1 white texture.
- The image loads in the background. The texture is white until it is ready.
- A sampler that was never written is Smooth. Every sampler clamps at the edges.

## Referencing another renderable

Write a renderable into a `u32` field. The shader gets the index of that renderable in the `objects` binding. Read `objects[index]` for its position, size, rotation and color, or use the helpers `object_contains` and `object_distance`.

This shader hides the part of a renderable that is under a lens:

```wgsl
struct Hide {
    lens: u32,
}
@group(1) @binding(0) var<uniform> hide: Hide;

@fragment
fn hidden(in: VertexOutput) -> @location(0) vec4<f32> {
    if object_contains(hide.lens, world_position(in.position)) {
        discard;
    }
    return in.color;
}
```

The shader follows the lens as it moves:

```luau
local Window = import("Window")
local Shader = import("Shader")
local Asset = import("Asset")

local window = Window.new({ Title = "Lens" })
local Renderable = window:GetAPI("Renderable")
local hider = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/hide.wgsl") }, { Name = "hide" }))

local wall = Renderable.new("RenderableShape", { Position = udim.new(200, 150), Size = udim.new(400, 300), Shaders = { hider } })
local lens = Renderable.new("RenderableShape", { Shape = enum.ShapeType.Circle, Position = udim.new(200, 150), Color = color.new(1, 1, 1, 0) })
wall:WriteShaderData(hider, "lens", lens)
print(wall:ReadShaderData(hider, "lens") == lens)
```

- The renderable must be from the same window.
- When it is hidden or destroyed, the shader sees `NO_OBJECT` and `object_contains` returns `false`.
- `ReadShaderData` gives back the renderable, or `nil` once it is destroyed.

## Reading the backdrop

The `backdrop` texture holds what was drawn before this renderable. When the entry point you use reads `backdrop`, luv copies the frame so far into it right before it draws this renderable. Read it at `in.position.xy`, which is in screen pixels:

```wgsl
@fragment
fn invert_behind(in: VertexOutput) -> @location(0) vec4<f32> {
    let behind = textureLoad(backdrop, vec2<i32>(in.position.xy), 0);
    return vec4<f32>(1.0 - behind.rgb, 1.0);
}
```

Each renderable that reads the backdrop costs one extra copy of the frame. Renderables that do not read it cause no copy.

## A custom Renderable

The class named `"Renderable"` has no position or size. It draws only what your shaders draw. Give it a shader with both a vertex and a fragment entry point. luv draws `VertexCount` vertices, `InstanceCount` times. There are no vertex buffers, so build each vertex from `@builtin(vertex_index)`, `@builtin(instance_index)` and your data.

To turn window pixels into the position the vertex stage returns, use `frame.resolution`:

```wgsl
struct Dot {
    position: vec2<f32>,
    radius: f32,
    fade: f32,
    color: vec4<f32>,
}
@group(1) @binding(0) var<storage, read> dots: array<Dot>;

struct DotOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) offset: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn dot_vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> DotOutput {
    let item = dots[instance];
    let corner = vec2<f32>(f32((0x32u >> vertex) & 1u), f32((0x2cu >> vertex) & 1u)) * 2.0 - vec2<f32>(1.0);
    let world = item.position + corner * item.radius;
    var out: DotOutput;
    out.position = vec4<f32>(world.x / frame.resolution.x * 2.0 - 1.0, 1.0 - world.y / frame.resolution.y * 2.0, 0.0, 1.0);
    out.offset = corner;
    out.color = vec4<f32>(item.color.rgb, item.color.a * item.fade);
    return out;
}

@fragment
fn dot_fragment(in: DotOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color.rgb, in.color.a * smoothstep(1.0, 0.8, length(in.offset)));
}
```

Six vertices make one square, and each instance is one dot:

```luau
local Window = import("Window")
local Shader = import("Shader")
local Asset = import("Asset")

local window = Window.new({ Title = "Dots" })
local Renderable = window:GetAPI("Renderable")
local dotShader = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/dots.wgsl") }, { Name = "dots" }))

local dots = Renderable.new("Renderable", { Shaders = { dotShader }, VertexCount = 6, InstanceCount = 2 })
dots:WriteShaderData(dotShader, {
	dots = {
		{ position = udim.new(100, 100), radius = 12, fade = 1, color = color.new(1, 0, 0, 1) },
		{ position = udim.new(160, 100), radius = 8, fade = 0.5, color = color.new(0, 1, 0, 1) },
	},
})
```

A [render hook](render-hooks.md) can write the data and the draw counts from native code every frame instead.

## How entry points are picked

- In one shader, luv uses the first `@vertex` entry point and the first `@fragment` entry point. Their names do not matter.
- With more than one shader loaded, the last loaded shader that has a vertex entry point gives the vertex stage. The same goes for the fragment stage.
- A RenderableShape, RenderableImage or RenderableText uses the built in stage for any stage your shaders do not have.
- A custom `"Renderable"` needs both stages.
- A post process needs a fragment stage. See [Post processing](post-processing.md).
- Compute entry points are not used.

## Where errors show up

| Problem | Where you see it |
| --- | --- |
| The code does not compile. | `Compiled` is `false` and `Error` has the message. `Shader.Compile` does not raise an error. |
| A binding breaks the rules, like a binding in group 0 or group 4. | [LoadShader](../reference/renderable.md#loadshader) raises an error. |
| A data name or a value is wrong. | [WriteShaderData](../reference/renderable.md#writeshaderdata) raises an error. |
| A stage is missing, or the stages do not fit together. | luv reports an error while the game runs, like `cannot draw with shader 'glow': ...`. |
| A texture image cannot be read. | luv reports `cannot draw the image 'bricks.png': ...` while the game runs. |

luv reports each drawing error once for each window.

## GLSL and SPIR-V

- GLSL needs a stage. Name the file `.vert`, `.frag` or `.comp`, or set `Stage` in an options table. See [ShaderSource](../reference/shader-library.md#shadersource).
- The prelude is WGSL, so it only works with WGSL code.
- A `buffer` or a `.spv` asset is read as SPIR-V. [GetSpirv](../reference/shader.md#getspirv) gives you the SPIR-V of a compiled shader.
