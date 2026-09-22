# PostProcess

Inherits: [BaseGameObject](basegameobject.md)

A shader pass that runs over the whole window after everything else is drawn.

## Description

You get a `PostProcess` from [window:AddPostProcess](window.md#addpostprocess). Its `ClassName` is `"PostProcess"`.

Every frame, luv first draws the window. Then it runs each enabled pass, from the lowest `Order` to the highest. Each pass reads the picture from the pass before it. The first pass reads the drawn window. The last pass writes to the screen.

A pass with no shader is skipped.

- [window:GetPostProcesses](window.md#getpostprocesses) lists the passes in the order they run.
- [Destroy](basegameobject.md#destroy) removes one pass.
- [window:ClearPostProcesses](window.md#clearpostprocesses) removes every pass.
- When the window closes, all of its passes are destroyed.

A pass is not a renderable. It does not show up in `Renderable.GetRenderables()`, and queries never hit it. Renderable members like `Position` and `ZIndex` error on a pass, for example `Position is not a valid member of PostProcess 'PostProcess'`. Using a destroyed pass errors with `PostProcess 'PostProcess' has been destroyed`.

See [Post processing](../manual/post-processing.md) for a guide.

It also has every member of [BaseGameObject](basegameobject.md).

## Properties

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `Enabled` | `boolean` | `true` | When `false`, luv skips this pass. |
| `Order` | `number` | see below | Passes run from the lowest `Order` to the highest. Passes with the same `Order` run in the order they were added. It must be a finite number, or it errors with `Order must be a finite number`. |
| `RenderHook` | [Pointer](pointer.md)`?` | `nil` | A native function that luv calls every frame before it draws the window. See [Render hooks](../manual/render-hooks.md). |
| `RenderHookData` | [Pointer](pointer.md)`?` | `nil` | A pointer that luv passes to the render hook. |

luv gives each new pass a bigger `Order` than it gave the passes before it. So by default, passes run in the order you add them. To run a pass earlier, give it a lower `Order`.

`RenderHook` and `RenderHookData` take a [Pointer](pointer.md), a `NativeFunction`, a `Callback` or `nil`. `RenderHook` errors with `RenderHook cannot be a null Pointer` when you give it a null pointer.

## Methods

### LoadShader

```luau
post:LoadShader(shader: Shader)
```

Loads a [Shader](shader.md) on this pass. Loading a shader that is already there does nothing.

It errors with `LoadShader expects a Shader` for other values. It errors with `shader '<name>' did not compile: <error>` for a shader that failed to compile.

The pass uses the first fragment entry point of the shader. When more than one shader is loaded, the pass uses the last one that has a fragment entry point.

### RemoveShader

```luau
post:RemoveShader(shader: Shader): boolean
```

Removes a shader from this pass. Returns `true` if it was loaded. Data that no other loaded shader uses is cleared.

### ClearShaders

```luau
post:ClearShaders()
```

Removes every shader and all data from this pass. The pass is then skipped until you load a shader again.

### HasShader

```luau
post:HasShader(shader: Shader): boolean
```

Returns `true` if the shader is loaded on this pass.

### GetShaders

```luau
post:GetShaders(): { Shader }
```

Returns the loaded shaders in the order they were loaded.

### WriteShaderData

```luau
post:WriteShaderData(shader: Shader, name: string, value: ShaderValue)
post:WriteShaderData(shader: Shader, values: { [string]: ShaderValue })
```

Writes data for a loaded shader. The first form writes one value. The second form writes every name and value in the table. The data stays until you change it.

`name` is the name of a binding in the shader, or the name of a field inside a uniform or storage struct. Use a dot to reach deeper fields, like `"tint.color"`.

- A buffer takes a value that fits its type in the shader. See [Shaders](../manual/shaders.md).
- A texture takes an image [Asset](asset.md), or `nil` to clear it.
- A sampler takes a [ResampleMode](enums.md#resamplemode).

It errors when the shader is not loaded on this pass, when the shader has no data with that name, or when the value does not fit.

### ReadShaderData

```luau
post:ReadShaderData(shader: Shader, name: string): any
```

Returns the data stored for `name`. Data you never wrote reads as zero. A texture gives its [Asset](asset.md) or `nil`. A sampler gives a [ResampleMode](enums.md#resamplemode).

## Writing a post process shader

- Put [Shader.Prelude](shader-library.md) in front of your WGSL source. It declares `VertexOutput`, `image`, `image_sampler` and `frame`.
- Write a fragment entry point that takes `in: VertexOutput` and returns `@location(0) vec4<f32>`.
- Read the picture from the pass before with `textureSampleLevel(image, image_sampler, in.uv, 0.0)`.
- `in.uv` is `(0, 0)` at the top left of the window and `(1, 1)` at the bottom right.
- The color you return replaces the pixel. It does not blend with what was there.
- `frame.time`, `frame.delta` and `frame.resolution` work here too.
- Put your own data in `@group(1)` to `@group(3)`. Group 0 belongs to luv.

If the loaded shaders have no fragment entry point, luv prints `a PostProcess needs a shader with a fragment entry point`.

This pass turns the colors of the window around:

```luau
local Window = import("Window")
local Shader = import("Shader")

local window = Window.new({ Title = "Inverted" })
local invert = Shader.Compile(Shader.Prelude .. [[
@fragment
fn post_invert(in: VertexOutput) -> @location(0) vec4<f32> {
	let color = textureSampleLevel(image, image_sampler, in.uv, 0.0);
	return vec4<f32>(vec3<f32>(1.0) - color.rgb, 1.0);
}
]])
window:AddPostProcess(invert)
```

This pass tints the window with a color that the script writes:

```luau
local Window = import("Window")
local Shader = import("Shader")

local window = Window.new({ Title = "Tinted" })
local tintShader = Shader.Compile(Shader.Prelude .. [[
struct Tint { color: vec4<f32> }
@group(1) @binding(0) var<uniform> tint: Tint;
@fragment
fn post_tint(in: VertexOutput) -> @location(0) vec4<f32> {
	return textureSampleLevel(image, image_sampler, in.uv, 0.0) * tint.color;
}
]])
local pass = window:AddPostProcess(tintShader)
pass:WriteShaderData(tintShader, "color", color.new(1, 0.5, 0, 1))
```
