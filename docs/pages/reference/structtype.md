# StructType

The layout of a C struct, made with [DLL.Struct](dll.md#struct).

## Description

Pass a list of fields to [DLL.Struct](dll.md#struct). Each field is a [StructField](#structfield). It has two forms:

- `{ "name", type }`
- `{ Name = "name", Type = type }`

```luau
local DLL = import("DLL")

local Vec2 = DLL.Struct({ { "x", "f32" }, { "y", "f32" } })
local Record = DLL.Struct({
	{ "id", "i32" },
	{ "weight", "f64" },
	{ Name = "tag", Type = DLL.Array("char", 8) },
})
print(Record.Size, Record.Alignment, Record:Offset("tag"))
```

The matching C structs:

```c
typedef struct Vec2 {
    float x;
    float y;
} Vec2;

typedef struct Record {
    int32_t id;
    double weight;
    char tag[8];
} Record;
```

luv lays out the fields like a C compiler does:

- Fields keep the order of the list.
- Each field starts at the next multiple of its alignment.
- The alignment of the struct is the largest alignment of its fields.
- The size is rounded up to a multiple of that alignment.

So `Record` above is 24 bytes, and `tag` starts at byte 16. A field can be another StructType or an [ArrayType](arraytype.md). luv has no packed structs, bit fields or unions. For those, read and write at fixed offsets with a [Pointer](pointer.md).

Use a StructType anywhere a type goes. That includes argument and return types, [Pointer:Read](pointer.md#read), [Pointer:Write](pointer.md#write), [DLL.New](dll.md#new), [DLL.SizeOf](dll.md#sizeof) and [DLL.Callback](dll.md#callback). Structs pass to C functions by value.

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
local Vec2 = DLL.Struct({ { "x", "f32" }, { "y", "f32" } })
local add = mathlib:GetFunction("vec2_add", Vec2, { Vec2, Vec2 })
local sum = add({ x = 1, y = 2 }, udim.new(3, 4))
print(sum.x, sum.y)
```

`DLL.Struct` errors when:

| Message | Cause |
| --- | --- |
| `field #2 must be a table like { "x", "f32" }` | A field is not a table. |
| `field #2 needs a name` | A field has no name, or an empty one. |
| `the struct has more than one field named 'x'` | Two fields share a name. |
| `field 'x': void has no values, it can only be a return type` | A field has the type `"void"`. Other type errors look the same. |
| `a struct needs at least one field` | The list is empty. |

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `Size` | `number` | The size in bytes. Read only. |
| `Alignment` | `number` | The alignment in bytes. Read only. |
| `Fields` | `{ string }` | The field names in order. Each read gives a new table. Read only. |

## Methods

### Offset

```luau
structType:Offset(field: string): number
```

Returns the byte offset of a field from the start of the struct. It only knows the fields of this struct, not the fields of a nested struct. It errors with `the struct has no field named 'x'` for a missing field.

```luau
local DLL = import("DLL")

local Vec2 = DLL.Struct({ { "x", "f32" }, { "y", "f32" } })
local Settings = DLL.Struct({ { "center", Vec2 }, { "size", "f32" }, { "speed", "f32" } })
local settings = Settings:New({ center = udim.new(320, 240), size = 150, speed = 1 })
settings:Write(Vec2, udim.new(400, 300), Settings:Offset("center"))
```

### New

```luau
structType:New(values: { [string]: any }?): Pointer
```

Allocates owned memory for one struct, fills it with zeros and writes `values` into it. Returns a [Pointer](pointer.md). This is the same as `DLL.New(structType, values)`. See [Values](#values) for what you can pass.

```luau
local DLL = import("DLL")

local Record = DLL.Struct({ { "id", "i32" }, { "weight", "f64" }, { "tag", DLL.Array("char", 8) } })
local stored = Record:New({ id = 5, weight = 1, tag = "abc" })
print(stored:ReadString(nil, Record:Offset("tag")), stored:Read(Record).id)
```

## Operators

| Operator | Result |
| --- | --- |
| `a == b` | `true` when both have the same fields, types and layout. |
| `tostring(s)` | `StructType(24 bytes)` |

`typeof(s)` is `"StructType"`.

## Values

luv turns these Luau values into a struct:

| Value | How it is used |
| --- | --- |
| Table | Each field is read by its name. The struct is set to zeros first, so fields you leave out are 0. |
| [UDim](udim.md) | `X`, `Y` and `Z` fill the fields in order. Every field must be `f32` or `f64`, and there can be at most 3. |
| [Color](color.md) | `R`, `G`, `B` and `A` fill the fields in order. Every field must be `f32` or `f64`, and there can be at most 4. |
| [Pointer](pointer.md) | luv copies one whole struct from that address. |

Reading a struct gives a table with one key for each field. A nested struct gives a nested table, and an array gives an array table.

In a call argument, `string` and `pointer` fields can take Luau strings and buffers. They live until the call returns. In memory they cannot. Use [DLL.String](dll.md#string) there.

A wrong value errors with a message like `a struct expects a table of fields or a Pointer to copy from, got number` or `a UDim or Color only fills structs of up to 3 float fields`. A wrong field value names the field, like `field 'x': f32 expects a number, got string`.

## StructField

One entry in the list you pass to [DLL.Struct](dll.md#struct). Use the list form or the named form. When both are set, the list form wins.

| Name | Type | Description |
| --- | --- | --- |
| `[1]` or `Name` | `string` | The field name. It cannot be empty or the same as another field. |
| `[2]` or `Type` | `DLLType` | The type of the field. Any [type name](dll.md#type-names) except `"void"`, a StructType or an ArrayType. |
