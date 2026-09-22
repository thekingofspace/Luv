# ShaderCombo

Inherits: [BaseGameObject](basegameobject.md)

Shader code joined from several parts, ready to compile.

## Description

You get a ShaderCombo from [Shader.Combine](shader-library.md#combine). Pass it to [Shader.Compile](shader-library.md#compile) to get a [Shader](shader.md). The Shader takes the name, the language and the stage of the combo. A compile error names the part and the line inside that part.

A combo can also be a part of another combo. Its parts are then added one by one.

It also has every member of [BaseGameObject](basegameobject.md).

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"ShaderCombo"`. Read only. |
| `Name` | `string` | The name from the options of Combine, or `"combo"`. |
| `Language` | `string` | `"wgsl"` or `"glsl"`. Read only. |
| `Parts` | `{ string }` | The name of each part, in order. A string part is named `prelude` when it is [Shader.Prelude](shader-library.md#prelude), and `part 1`, `part 2` and so on otherwise. Read only. |
| `Source` | `string` | The joined code. Read only. |

## Methods

### Destroy

```luau
combo:Destroy()
```

After this, reading `Language`, `Parts` or `Source`, or compiling the combo, raises an error like `ShaderCombo 'neon' has been destroyed`. Shaders that were already compiled from it keep working. See [BaseGameObject](basegameobject.md#destroy).

## Example

```luau
local Shader = import("Shader")
local Asset = import("Asset")

local combo = Shader.Combine({ Shader.Prelude, Asset.Load("shaders/neon.wgsl") }, { Name = "neon" })
print(combo.Language, table.concat(combo.Parts, ", "))

local neon = Shader.Compile(combo)
if not neon.Compiled then
	print(neon.Error)
end
```
