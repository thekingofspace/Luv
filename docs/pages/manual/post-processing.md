# Post processing

A post process runs a fragment shader over the whole window after everything else is drawn. Use it for screen effects like color grading, a flash or a shake. Each pass is a [PostProcess](../reference/postprocess.md) object.

## Adding a pass

Compile a shader and pass it to [window:AddPostProcess](../reference/window.md#addpostprocess):

```luau
local Window = import("Window")
local Shader = import("Shader")

local window = Window.new({ Title = "Inverted" })
local invert = Shader.Compile(Shader.Prelude .. [[
@fragment
fn invert(in: VertexOutput) -> @location(0) vec4<f32> {
	let color = textureSampleLevel(image, image_sampler, in.uv, 0.0);
	return vec4<f32>(vec3<f32>(1.0) - color.rgb, 1.0);
}
]])
local pass = window:AddPostProcess(invert)
```

The pass runs every frame until you destroy it or the window closes. A pass needs a fragment entry point. If its shaders have none, luv reports `a PostProcess needs a shader with a fragment entry point`.

You can also call `AddPostProcess` with no shader and load one later with [LoadShader](../reference/postprocess.md#loadshader). A pass with no shader is skipped.

## What a pass reads

- `image` holds the picture so far. Sample it with `image_sampler` at `in.uv`. The sampler is smooth and clamps at the edges.
- `in.uv` goes from (0, 0) at the top left of the window to (1, 1) at the bottom right.
- `in.local` is the same point in window pixels from the top left. `frame.resolution` is the window size in window pixels.
- `textureDimensions(image)` is the size of the picture in screen pixels.
- `frame.time` and `frame.delta` work like they do in other shaders.
- The color you return replaces the pixel. Blend modes do not apply. Return 1.0 as the alpha.

A pass can have its own vertex entry point, but it does not need one. Without one, luv covers the window for you. With one, luv draws 6 vertices.

## Chaining passes

Each pass reads the picture from the pass before it. The first pass reads the drawn window, and the last pass writes to the screen. Passes run from the lowest `Order` to the highest.

luv gives each new pass a higher `Order` than the ones before it. So by default, passes run in the order you add them. Give a pass a lower `Order` to run it earlier:

```luau
local Window = import("Window")
local Shader = import("Shader")
local Asset = import("Asset")

local window = Window.new({ Title = "Chain" })
local blur = window:AddPostProcess(Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/blur.wgsl") }, { Name = "blur" })))
local grade = window:AddPostProcess(Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/grade.wgsl") }, { Name = "grade" })))

grade.Order = blur.Order - 1
print(window:GetPostProcesses()[1] == grade)
```

[window:GetPostProcesses](../reference/window.md#getpostprocesses) lists the passes in the order they run.

## Turning passes on and off

Set `Enabled` to `false` to skip a pass. Set it back to `true` to run it again. The pass keeps its shaders and its data.

To remove a pass for good, call `Destroy` on it. [window:ClearPostProcesses](../reference/window.md#clearpostprocesses) removes every pass of the window.

```luau
local Window = import("Window")
local Shader = import("Shader")
local Asset = import("Asset")

local window = Window.new({ Title = "Toggle" })
local Input = window:GetAPI("Input")
local crt = window:AddPostProcess(Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/crt.wgsl") }, { Name = "crt" })))

Input.KeyDown:BindHandler("crt", function(key: KeyCodeEnum)
	if key == enum.KeyCode.F2 then
		crt.Enabled = not crt.Enabled
	end
end)
```

## Writing data to a pass

A pass takes shader data like a renderable does. Put your data in `@group(1)` to `@group(3)` and write it with [WriteShaderData](../reference/postprocess.md#writeshaderdata). See [Shader data](shaders.md#shader-data).

## A full example

This pass adds a dark edge, a white flash and a shake. The script starts the flash and the shake when you press Space.

```wgsl
struct Screen {
    flash: f32,
    vignette: f32,
    shake: vec2<f32>,
}
@group(1) @binding(0) var<uniform> screen: Screen;

@fragment
fn screen_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv + screen.shake / frame.resolution;
    let color = textureSampleLevel(image, image_sampler, uv, 0.0).rgb;
    let centered = in.uv * 2.0 - vec2<f32>(1.0);
    let shade = 1.0 - screen.vignette * dot(centered, centered) * 0.5;
    return vec4<f32>(color * shade + vec3<f32>(screen.flash), 1.0);
}
```

Save it as `assets/shaders/screen.wgsl`. The script writes the data every frame:

```luau
local Window = import("Window")
local Shader = import("Shader")
local Asset = import("Asset")

local window = Window.new({ Title = "Hit", Size = udim.new(800, 450) })
local Input = window:GetAPI("Input")
local shader = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/screen.wgsl") }, { Name = "screen" }))
local screen = window:AddPostProcess(shader)
screen:WriteShaderData(shader, "vignette", 0.6)

local flash = 0
Input.KeyDown:BindHandler("hit", function(key: KeyCodeEnum)
	if key == enum.KeyCode.Space then
		flash = 0.4
	end
end)

window.PreFrame:BindHandler("screen", function(dt: number)
	flash *= math.exp(-dt * 8)
	local angle = math.random() * math.pi * 2
	screen:WriteShaderData(shader, { flash = flash, shake = udim.new(math.cos(angle), math.sin(angle)) * flash * 20 })
end)
```
