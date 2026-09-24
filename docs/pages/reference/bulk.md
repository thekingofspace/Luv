# Bulk

Sets many properties or shader values in one call.

```luau
local Bulk = import("Bulk")
```

## Description

Bulk takes a table that maps objects to their changes. It works with any object that has properties you can set. That includes [renderables](renderable.md), sound nodes like [ToSpeaker](tospeaker.md) and [SoundNode](soundnode.md), and plain Luau tables.

Changes are applied one at a time, in `pairs` order. When one fails, Bulk stops and raises an error. The changes made before the error stay.

In error messages, an object shows up as its `Name`. A plain table shows up as `a table`.

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [BulkUpdate](#bulkupdate)(updates) | nothing | no |
| [BulkWriteShaderData](#bulkwriteshaderdata)(shader, updates) | nothing | no |

## Function descriptions

### BulkUpdate

```luau
Bulk.BulkUpdate(updates: { [any]: { [string]: any } })
```

Sets properties on many objects. Each key is an object or a table. Each value is a table of property names and new values. It does the same as setting each property yourself, in one call instead of one call per property.

```luau
local Window = import("Window")
local Bulk = import("Bulk")

local window = Window.new({ Title = "Arena" })
local Renderable = window:GetAPI("Renderable")
local player = Renderable.new("RenderableShape", { Shape = enum.ShapeType.Circle })
local enemy = Renderable.new("RenderableShape", { Shape = enum.ShapeType.Diamond })

Bulk.BulkUpdate({
	[player] = { Position = udim.new(120, 180), Color = color.fromHex("#4caf50") },
	[enemy] = { Position = udim.new(520, 180), Color = color.fromHex("#e53935") },
})
```

It errors when:

| Message | Cause |
| --- | --- |
| `the update for <object> must be a table of properties, got <type>` | The value for an object is not a table. |
| `property names must be strings, got <type> for <object>` | A property name is not a string. |
| `cannot set <property> on <object>: <reason>` | The key is not an object or a table, the object has no such property, or it refused the value. |

```luau
local Bulk = import("Bulk")
local Signal = import("Signal")

local touched = Signal.new()
touched.Name = "Touched"

local ok, err = pcall(Bulk.BulkUpdate, { [touched] = { Missing = 1 } })
print(ok, string.find(tostring(err), "cannot set Missing on Touched", 1, true) ~= nil)
```

This prints `false` and `true`.

### BulkWriteShaderData

```luau
Bulk.BulkWriteShaderData(shader: Shader, updates: { [Renderable]: { [string]: ShaderValue } })
```

Writes shader data for many renderables at once. Each key is a renderable. Each value is a table of data names and values. It does the same as calling `WriteShaderData` with a table on each [Renderable](renderable.md). A [PostProcess](postprocess.md) works as a key too.

Each renderable must already have the [Shader](shader.md) loaded. The names must match the data that the shader declares.

```luau
local Bulk = import("Bulk")

local function pulse(shader: Shader, targets: { Renderable }, time: number)
	local updates: { [Renderable]: { [string]: ShaderValue } } = {}
	for index, target in targets do
		updates[target] = {
			Strength = math.sin(time * 4 + index) * 0.5 + 0.5,
			Tint = color.fromHSV(index / #targets, 0.8, 1),
		}
	end
	Bulk.BulkWriteShaderData(shader, updates)
end
```

It errors when:

| Message | Cause |
| --- | --- |
| `BulkWriteShaderData expects a Shader as its first argument` | The first argument is not a Shader. |
| `the keys of BulkWriteShaderData must be renderables, got <object>` | A key is not a renderable. |
| `cannot write shader data for <object>: <reason>` | Writing failed. The reason says why. |

The reasons include:

| Reason | Cause |
| --- | --- |
| `the shader is not loaded on <class> '<name>'` | The renderable does not have this shader loaded. |
| `shader data must be a table of names and values, got <type>` | The value for a renderable is not a table. |
| `shader data names must be strings, got <type>` | A data name is not a string. |
