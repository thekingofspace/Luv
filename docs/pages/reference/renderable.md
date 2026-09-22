# Renderable

Inherits: [BaseGameObject](basegameobject.md)

Inherited by: [RenderableShape](renderableshape.md), [RenderableImage](renderableimage.md), [RenderableText](renderabletext.md)

Something that a window draws.

## Description

You make renderables with [Renderable.new](renderable-api.md#new). There are four classes:

| Class | What it draws |
| --- | --- |
| `Renderable` | Only what your shaders draw. See [The Renderable class](#the-renderable-class). |
| [RenderableShape](renderableshape.md) | A shape, like a rectangle or a circle. |
| [RenderableImage](renderableimage.md) | An image or a part of one. |
| [RenderableText](renderabletext.md) | Text in one font. |

This page lists the members that all four share. The shape, image and text classes also have the [placement](#placement) members.

`typeof` returns `"Renderable"` for all four classes. Use `ClassName` to tell them apart. The default `Name` is the class name.

A renderable belongs to the window that made it. The window keeps it alive until you call `Destroy` or the window closes. Changes apply at once in Luau and show on screen on the next frame.

Using a member that the class does not have raises an error like `Font is not a valid member of RenderableShape 'Box'`. After `Destroy`, every member except `Name` and `ClassName` raises an error like `RenderableShape 'Box' has been destroyed`.

It also has every member of [BaseGameObject](basegameobject.md).

## Properties

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `SinkUpdates` | `boolean` | `true` | Set it to `false` to hide the renderable. See [Hiding](#hiding). |
| `ZIndex` | `number` | `0` | The draw order. Higher values draw on top. See [Drawing order](#drawing-order). |
| `BlendMode` | [BlendMode](enums.md#blendmode) | `enum.BlendMode.Alpha` | How it mixes with what is behind it. See [Blend modes](#blend-modes). |
| `RenderHook` | [Pointer](pointer.md)`?` | `nil` | A native function that luv calls before each frame is drawn. See [Render hooks](#render-hooks). |
| `RenderHookData` | [Pointer](pointer.md)`?` | `nil` | The address the render hook gets as `user_data`. |

## Drawing order

luv draws renderables from the lowest ZIndex to the highest. When two have the same ZIndex, the one made later draws on top. ZIndex can be any number, like `-10` or `2.5`. It must be finite, or it raises `ZIndex must be a finite number`.

ZIndex also sets the order of query results and the order that render hooks run in.

## Hiding

When `SinkUpdates` is `false`, the renderable leaves the frame:

- It is not drawn.
- Queries do not find it.
- Its render hook does not run.
- Shaders that point at it see `NO_OBJECT`. See [Referencing another renderable](../manual/shaders.md#referencing-another-renderable).

You can still read and change its properties. Set `SinkUpdates` back to `true` to show it again with all the changes. It stays in [GetRenderables](renderable-api.md#getrenderables).

A Color with an alpha of 0 also makes a renderable invisible, but queries still find it.

## Blend modes

| BlendMode | What it does |
| --- | --- |
| `Alpha` | The default. Draws over what is behind it, using the alpha of its colors. |
| `Additive` | Adds its color to what is behind it, scaled by alpha. It can only make things brighter. |
| `Multiply` | Multiplies what is behind it by its color. White changes nothing. Alpha fades the effect. |
| `Opaque` | Writes its pixels with no mixing. The whole box is written, so the corners around a Circle and the clear parts of an image come out black. Use it for things that fill a rectangle, like a background. |

```luau
local Window = import("Window")

local window = Window.new({ Title = "Glow" })
local Renderable = window:GetAPI("Renderable")

Renderable.new("RenderableShape", {
	Shape = enum.ShapeType.Circle,
	Position = udim.new(200, 150),
	Size = udim.new(120, 120),
	Color = color.new(0.1, 0.85, 1, 0.4),
	BlendMode = enum.BlendMode.Additive,
})
```

## Render hooks

`RenderHook` points at a function in a native plugin. luv calls it on the render thread before each frame is drawn. The hook can write shader data, upload textures and set the draw counts of a custom `Renderable`. See [Render hooks](../manual/render-hooks.md) and [LuvRenderContext](native-c.md#luvrendercontext).

- `RenderHook` takes a [Pointer](pointer.md), a [NativeFunction](nativefunction.md), a [Callback](callback.md) or `nil`. A null Pointer raises `RenderHook cannot be a null Pointer`.
- `RenderHookData` takes a Pointer, an object made by a native plugin, or `nil`. The hook gets its address as `user_data`.
- Other values raise an error like `RenderHook must be a Pointer, NativeFunction, Callback or nil, got number`.
- Reading either property gives back a Pointer.
- luv keeps the plugin, the memory or the object alive while it is set.

Hooks run in ZIndex order. They do not run while `SinkUpdates` is `false`.

> [!WARNING]
> The hook runs on the render thread, not in Luau. A Callback makes that thread wait for Luau every frame. Use a function from a native plugin.

## Methods

These methods load shaders and write their data. A [PostProcess](postprocess.md) has the same methods.

### LoadShader

```luau
renderable:LoadShader(shader: Shader)
```

Loads a compiled [Shader](shader.md) on this renderable. Loading a shader that is already loaded does nothing. The load order matters when two shaders have the same stage. See [How entry points are picked](../manual/shaders.md#how-entry-points-are-picked).

luv checks the bindings of the shader here, not when it compiles. It errors when:

- The value is not a Shader: `LoadShader expects a Shader`.
- The shader did not compile: `shader 'glow' did not compile: ...`.
- The shader uses push constants.
- A texture is not a sampled 2D texture, like ``glow: `tiles` must be a 2D texture``.
- A binding in group 0 does not match the engine bindings: ``glow: `data` uses @group(0) @binding(7), but group 0 is reserved for the engine (0 = frame, 1 = instances, 2 = image, 3 = image_sampler, 4 = objects, 5 = backdrop)``.
- A binding uses a group above 3: ``glow: `data` uses @group(4), but renderable data can only use groups 1 to 3``.

```luau
local Window = import("Window")
local Shader = import("Shader")
local Asset = import("Asset")

local window = Window.new({ Title = "Shaded" })
local Renderable = window:GetAPI("Renderable")
local glow = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/glow.wgsl") }, { Name = "glow" }))

local box = Renderable.new("RenderableShape", { Position = udim.new(200, 150) })
box:LoadShader(glow)
print(box:HasShader(glow))
```

### RemoveShader

```luau
renderable:RemoveShader(shader: Shader): boolean
```

Removes a shader. Returns `true` when it was loaded and `false` when it was not. Data that no other loaded shader uses is dropped. So when you load the shader again, its data starts at 0.

### ClearShaders

```luau
renderable:ClearShaders()
```

Removes every shader and drops all shader data. A RenderableShape, RenderableImage or RenderableText then draws the normal way again.

### HasShader

```luau
renderable:HasShader(shader: Shader): boolean
```

Returns `true` when the shader is loaded on this renderable.

### GetShaders

```luau
renderable:GetShaders(): { Shader }
```

Returns the loaded shaders in the order they were loaded.

### WriteShaderData

```luau
renderable:WriteShaderData(shader: Shader, name: string, value: ShaderValue)
renderable:WriteShaderData(shader: Shader, values: { [string]: ShaderValue })
```

Writes data for a shader that is loaded on this renderable. The second form writes many names at once, one after the other. When one of them fails, the ones before it stay written. It does not yield.

Each renderable keeps its own data, so two renderables with the same shader can look different. Data starts at 0. It stays until you write it again or remove the shader. When two loaded shaders use the same group and binding, they share that data.

The name picks what to write:

- The name of a binding in group 1, 2 or 3 writes the whole binding.
- The name of a member of a uniform or storage struct writes only that member.
- Add `.member` to go deeper, like `"panel.origin"`.

Group 0 belongs to the engine. It cannot be written.

| WGSL type | Luau value |
| --- | --- |
| `f32`, `f64` | A number. `true` is 1 and `false` is 0. An enum item gives its `Value`. |
| `i32`, `u32`, `i64`, `u64` | The same, but it must be a whole number that fits the type. |
| `u32`, `i32` | A renderable from the same window. See [Referencing another renderable](../manual/shaders.md#referencing-another-renderable). |
| `vec2`, `vec3`, `vec4` | A [UDim](udim.md) (X, Y, Z), a [Color](color.md) (R, G, B, A), a `vector`, a table of numbers, or one number for the first part. Missing parts are 0. |
| `mat2x2` to `mat4x4` | A table of numbers, one column after the other. A table of columns works too. |
| `array<T, N>` | A table of up to N items. Items you leave out become 0. |
| `array<T>` | A table of items. The array gets exactly that many items. |
| Array of `u32` or `i32` | A string. Each character becomes its code point. |
| Struct | A table of member names and values. Members you leave out keep their value. |
| Any of the above | A `buffer`. Its bytes are copied as they are. |
| `texture_2d<f32>` | An image [Asset](asset.md). The texture is white until the image has loaded. `nil` sets it back to a 1x1 white texture. |
| `sampler` | A [ResampleMode](enums.md#resamplemode) item, or `"Smooth"`, `"Linear"`, `"Pixelated"` or `"Nearest"`. The default is Smooth. Every sampler clamps at the edges. |

`f16` values cannot be written from Luau.

| Message | Cause |
| --- | --- |
| `the shader is not loaded on RenderableShape 'Box'` | Load the shader first. |
| `shader 'glow' has no data named 'size', it declares settings, pattern` | No binding or member has this name. |
| `'tint' is ambiguous in shader 'glow', qualify it with the binding name like 'binding.tint'` | Two bindings have a member with this name. |
| `'settings' in shader 'glow' has no member 'size'` | A part of a dotted name does not exist. |
| `cannot write 'strength' in shader 'glow': expected a number, got string` | The value does not fit the type. |
| `cannot write 'codes' in shader 'glow': the array holds 6 items but 7 were given` | Too many items for a fixed array. |
| `'pattern' in shader 'glow' is a texture, so it takes an image Asset, got number` | A texture needs an Asset. |

For a shader with this data:

```wgsl
struct Settings {
    tint: vec4<f32>,
    offset: vec2<f32>,
    strength: f32,
}
@group(1) @binding(0) var<uniform> settings: Settings;
```

you write and read it like this:

```luau
local Window = import("Window")
local Shader = import("Shader")
local Asset = import("Asset")

local window = Window.new({ Title = "Data" })
local Renderable = window:GetAPI("Renderable")
local glow = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/glow.wgsl") }, { Name = "glow" }))
local box = Renderable.new("RenderableShape", { Position = udim.new(200, 150), Shaders = { glow } })

box:WriteShaderData(glow, "strength", 0.5)
box:WriteShaderData(glow, { tint = color.new(1, 0.5, 0, 1), offset = udim.new(4, 4) })
print(box:ReadShaderData(glow, "tint"))
```

To write the same shader on many renderables in one call, use [Bulk.BulkWriteShaderData](bulk.md#bulkwriteshaderdata).

### ReadShaderData

```luau
renderable:ReadShaderData(shader: Shader, name: string): any
```

Reads back data that Luau wrote. Names work like in [WriteShaderData](#writeshaderdata). Data that was never written reads as 0. Data that a render hook writes is not visible here.

| WGSL type | Returns |
| --- | --- |
| `f32`, `f64`, `u64` | A number. |
| `i32`, `u32`, `i64` | A whole number. |
| `u32` or `i32` that holds a renderable | The renderable, or `nil` once it is destroyed. |
| `vec2<f32>` | A [UDim](udim.md) with a Z of 0. |
| `vec3<f32>` | A [UDim](udim.md). |
| `vec4<f32>` | A [Color](color.md). |
| Other vectors | A table of numbers. |
| Matrix | A table of numbers, one column after the other. |
| Array | A table of items. An `array<T>` gives the items that were written. |
| Struct | A table of member names and values. |
| `texture_2d<f32>` | The [Asset](asset.md), or `nil`. |
| `sampler` | A [ResampleMode](enums.md#resamplemode) item. |

## The Renderable class

The class named `"Renderable"` has no placement members. It draws nothing by itself. Load shaders with a vertex and a fragment entry point, and luv draws `VertexCount` vertices, `InstanceCount` times, with them. There are no vertex buffers. Your vertex shader builds each vertex from `@builtin(vertex_index)`, `@builtin(instance_index)` and your shader data.

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `VertexCount` | `number` | `6` | How many vertices to draw. A whole number of 0 or more. |
| `InstanceCount` | `number` | `1` | How many instances to draw. A whole number of 0 or more. |

- With no shaders loaded, it draws nothing.
- When the loaded shaders miss a stage, luv reports `a Renderable needs shaders with both a vertex and a fragment entry point, no vertex entry point is loaded` while the game runs.
- A render hook can set both counts for each frame. See [Render hooks](../manual/render-hooks.md).
- Queries never find it.
- A count that is not a whole number of 0 or more raises `VertexCount must be a whole number of at least 0`.

This shader draws a rectangle from its data:

```wgsl
struct Panel {
    tint: vec4<f32>,
    origin: vec2<f32>,
    extent: vec2<f32>,
}
@group(1) @binding(0) var<uniform> panel: Panel;

@vertex
fn panel_vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32((0x32u >> index) & 1u), f32((0x2cu >> index) & 1u));
    let world = panel.origin + corner * panel.extent;
    return vec4<f32>(world.x / frame.resolution.x * 2.0 - 1.0, 1.0 - world.y / frame.resolution.y * 2.0, 0.0, 1.0);
}

@fragment
fn panel_fragment() -> @location(0) vec4<f32> {
    return panel.tint;
}
```

This script draws it:

```luau
local Window = import("Window")
local Shader = import("Shader")
local Asset = import("Asset")

local window = Window.new({ Title = "Panel" })
local Renderable = window:GetAPI("Renderable")
local shader = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/panel.wgsl") }, { Name = "panel" }))

local panel = Renderable.new("Renderable", { Shaders = { shader }, VertexCount = 6 })
panel:WriteShaderData(shader, { tint = color.new(1, 0, 1, 1), origin = udim.new(10, 10), extent = udim.new(40, 30) })
```

## Placement

[RenderableShape](renderableshape.md), [RenderableImage](renderableimage.md) and [RenderableText](renderabletext.md) have these members. The `"Renderable"` class does not.

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `Position` | [UDim](udim.md) | `udim.new(0, 0)` | Where the anchor point sits in the window. |
| `Size` | [UDim](udim.md) | depends on the class | The width and the height. |
| `AnchorPoint` | [UDim](udim.md) | `udim.new(0.5, 0.5)` | Which point of the renderable sits at Position. |
| `Rotation` | `number` | `0` | Turns it clockwise around the anchor point, in degrees. |
| `Color` | [Color](color.md) | `color.white` | The fill of a shape, the tint of an image or the color of text. |

All of them use window pixels. These are the same units as the window size, so they do not change with the display scale of the screen. The point (0, 0) is the top left corner of the window. X grows to the right and Y grows down. Positions and sizes can have fractions. The Z of a UDim is not used.

AnchorPoint is a fraction of Size:

| AnchorPoint | The point that sits at Position |
| --- | --- |
| `udim.new(0, 0)` | The top left corner. |
| `udim.new(0.5, 0.5)` | The center. This is the default. |
| `udim.new(1, 0)` | The top right corner. |
| `udim.new(0.5, 1)` | The middle of the bottom edge. |
| `udim.new(1, 1)` | The bottom right corner. |

Rotation turns the renderable around its anchor point. With the default AnchorPoint, it spins around its center.

The default Size depends on the class:

| Class | Default Size |
| --- | --- |
| RenderableShape | `udim.new(100, 100)`. |
| RenderableImage | The size of the image, when the config has no Size. |
| RenderableText | `udim.new(0, 0)`, which fits the box to the text. |

A value that is not finite raises an error like `Position must only hold finite numbers` or `Rotation must be a finite number`.

This bar sits along the bottom edge of the window:

```luau
local Window = import("Window")

local window = Window.new({ Title = "Bar", Size = udim.new(800, 450) })
local Renderable = window:GetAPI("Renderable")

Renderable.new("RenderableShape", {
	Position = udim.new(0, 450),
	AnchorPoint = udim.new(0, 1),
	Size = udim.new(800, 40),
	Color = color.new(0.2, 0.2, 0.3, 1),
})
```

## Config

[Renderable.new](renderable-api.md#new) takes a config table. Any property you can set works as a key. These tables list the keys that more than one class shares. Each class page lists its own keys.

### Shared fields

Every class takes these.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Name` | `string` | the class name | The name. |
| `SinkUpdates` | `boolean` | `true` | `false` makes it start hidden. |
| `ZIndex` | `number` | `0` | The draw order. |
| `BlendMode` | [BlendMode](enums.md#blendmode) | `enum.BlendMode.Alpha` | How it mixes with what is behind it. |
| `Shaders` | `{ Shader }` | none | Shaders to load, in order. The same as calling [LoadShader](#loadshader) for each one. |
| `RenderHook` | [Pointer](pointer.md) | none | The render hook. |
| `RenderHookData` | [Pointer](pointer.md) | none | The address the hook gets as `user_data`. |

`Shaders` must be a list, or it raises `Shaders must be an array of Shader objects`.

### Custom class fields

Only the `"Renderable"` class takes these.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `VertexCount` | `number` | `6` | How many vertices to draw. |
| `InstanceCount` | `number` | `1` | How many instances to draw. |

### Placement fields

RenderableShape, RenderableImage and RenderableText take these.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Position` | [UDim](udim.md) | `udim.new(0, 0)` | Where the anchor point sits. |
| `Size` | [UDim](udim.md) | depends on the class | The width and the height. |
| `AnchorPoint` | [UDim](udim.md) | `udim.new(0.5, 0.5)` | Which point sits at Position. |
| `Rotation` | `number` | `0` | Degrees, clockwise. |
| `Color` | [Color](color.md) | `color.white` | The main color. |
