# Shader

Inherits: [BaseGameObject](basegameobject.md)

Compiled shader code that renderables and post processes can load.

## Description

You get a Shader from [Shader.Compile](shader-library.md#compile). Compile always returns a Shader, even when the code has errors. Check `Compiled` before you use it.

Load it on a renderable with [LoadShader](renderable.md#loadshader), or pass it to [window:AddPostProcess](window.md#addpostprocess). One Shader can be loaded on many renderables, in any window. Each renderable keeps its own data for it.

luv reads the bindings of the shader when it is loaded. So a shader can compile fine and still fail to load. See [LoadShader](renderable.md#loadshader).

Renderables use the first vertex entry point and the first fragment entry point of a shader. Compute entry points compile, but renderables do not use them. See [How entry points are picked](../manual/shaders.md#how-entry-points-are-picked).

It also has every member of [BaseGameObject](basegameobject.md).

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"Shader"`. Read only. |
| `Name` | `string` | The name used in error messages. It comes from the source. See [ShaderSource](shader-library.md#shadersource). |
| `Language` | `string` | `"wgsl"`, `"glsl"` or `"spirv"`. Read only. |
| `Compiled` | `boolean` | `true` when the code compiled. Read only. |
| `Error` | `string?` | The compile error, or `nil`. Read only. |
| `Size` | `number` | The size of the compiled SPIR-V code in bytes. 0 when it did not compile. Read only. |
| `EntryPoints` | `{ ShaderEntryPoint }` | The entry points in the code, in order. Empty when it did not compile. See [ShaderEntryPoint](#shaderentrypoint). Read only. |

## Methods

### GetSpirv

```luau
shader:GetSpirv(): buffer
```

Returns the compiled SPIR-V code as a buffer. You can pass it back to [Shader.Compile](shader-library.md#compile) later. It errors when the shader did not compile, with `shader 'glow' did not compile: ...`.

```luau
local Shader = import("Shader")

local original = Shader.Compile(Shader.Prelude .. [[
@fragment
fn white() -> @location(0) vec4<f32> {
	return vec4<f32>(1.0);
}
]])
local spirv = original:GetSpirv()
local copy = Shader.Compile(spirv)
print(copy.Language, buffer.len(spirv))
```

### Destroy

```luau
shader:Destroy()
```

Frees the compiled code. After this, `Compiled` is `false` and the shader cannot be loaded anymore. Renderables that already loaded it keep drawing with it. See [BaseGameObject](basegameobject.md#destroy).

## ShaderEntryPoint

One entry of `EntryPoints`.

| Name | Type | Description |
| --- | --- | --- |
| `Name` | `string` | The name of the function. |
| `Stage` | `string` | The stage, like `"vertex"`, `"fragment"` or `"compute"`. |
| `WorkgroupSize` | [UDim](udim.md) | The workgroup size of a compute entry point, as X, Y and Z. It is 0 for other stages. |

```luau
local Shader = import("Shader")
local Asset = import("Asset")

local shader = Shader.Compile(Asset.Load("shaders/triangle.wgsl"))
for _, entry in shader.EntryPoints do
	print(entry.Name, entry.Stage, entry.WorkgroupSize)
end
```
