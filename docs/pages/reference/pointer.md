# Pointer

An address in native memory.

## Description

You get a `Pointer` from many places:

- [DLL.Alloc](dll.md#alloc), [DLL.New](dll.md#new) and [DLL.String](dll.md#string).
- `New` on a [StructType](structtype.md#new) or an [ArrayType](arraytype.md#new).
- [Library:GetSymbol](library.md#getsymbol), and the `Pointer` of a [NativeFunction](nativefunction.md) or [Callback](callback.md).
- [DLL.Pointer](dll.md#pointer) and [DLL.Null](dll.md#null).
- C functions and callbacks that return or pass a `pointer`.

`typeof(pointer)` is `"Pointer"`. Pass a Pointer to any C argument of type `pointer`, `string` or `wstring`.

## Kinds of pointer

| Kind | Where it comes from | Size | Bounds checks | Free |
| --- | --- | --- | --- | --- |
| Owned | [DLL.Alloc](dll.md#alloc), [DLL.New](dll.md#new), [DLL.String](dll.md#string), and `New` on a StructType or ArrayType. | The bytes from this address to the end of the memory. | Yes | Yes, on the Pointer you got from the function. |
| Object | A plugin object. `RenderHookData` returns one when it holds a plugin object. | The bytes from this address to the end of the object. | Yes | No |
| Foreign | Everything else: symbols, function and callback pointers, DLL.Pointer, DLL.Null and pointers from C. | `nil` | No | No |

A Pointer made with [Offset](#offset) keeps the kind of the Pointer it came from.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `Address` | `number` | The address. Read only. |
| `IsNull` | `boolean` | `true` when the address is 0. Read only. |
| `Size` | `number?` | For owned and object pointers, the bytes from this address to the end. `nil` for foreign pointers and for freed memory. Read only. |

## Methods

### Read

```luau
pointer:Read(valueType: DLLType, offset: number?): any
```

Reads one value of `valueType` at `offset` bytes from the address. The value is converted like a return value. See [C to Luau](dll.md#c-to-luau). A struct gives a table with a key for each field. `offset` must be a whole number and can be negative.

```luau
local DLL = import("DLL")

local memory = DLL.Alloc(16)
memory:Write("u16", 513, 2)
print(memory:Read("u16", 2), memory:Read("u8", 2))
```

### Write

```luau
pointer:Write(valueType: DLLType, value: any, offset: number?)
```

Writes one value of `valueType` at `offset` bytes from the address. See [Luau to C](dll.md#luau-to-c).

Writing a struct or an array writes all of it. Fields and elements that you leave out become 0. A Luau string or buffer cannot go into a `string` or `pointer` value here. Store a Pointer from [DLL.String](dll.md#string) instead.

```luau
local DLL = import("DLL")

local Vec2 = DLL.Struct({ { "x", "f32" }, { "y", "f32" } })
local point = Vec2:New()
point:Write(Vec2, { x = 3, y = 4 })
point:Write("f32", 10, Vec2:Offset("y"))
print(point:Read(Vec2).y)
```

### ReadString

```luau
pointer:ReadString(length: number?, offset: number?): string
```

Reads text at `offset` bytes from the address. With `length`, it reads exactly that many bytes, zero bytes included. Without it, it reads up to the first zero byte. For owned and object pointers, it also stops at the end of the memory.

```luau
local DLL = import("DLL")

local memory = DLL.Alloc(16)
memory:WriteString("hi", 8)
print(memory:ReadString(nil, 8), #memory:ReadString(4))
```

### ReadWideString

```luau
pointer:ReadWideString(length: number?, offset: number?): string
```

Reads `wchar_t` text and returns it as UTF-8. `length` counts characters, not bytes. Without it, the text ends at the first zero character.

### WriteString

```luau
pointer:WriteString(text: string, offset: number?)
```

Writes the bytes of `text` and one zero byte after them. The memory needs room for `#text + 1` bytes.

### WriteWideString

```luau
pointer:WriteWideString(text: string, offset: number?)
```

Writes `text` as `wchar_t` characters with a zero character after them. `text` must be UTF-8.

```luau
local DLL = import("DLL")

local memory = DLL.Alloc(32)
memory:WriteWideString("héllo")
print(memory:ReadWideString())
```

### ReadBuffer

```luau
pointer:ReadBuffer(length: number, offset: number?): buffer
```

Copies `length` bytes into a new buffer.

### WriteBuffer

```luau
pointer:WriteBuffer(data: Bytes, offset: number?)
```

Copies the bytes of a string or buffer. It adds no zero byte. It errors with `the data must be a string or buffer, got number` for other values.

```luau
local DLL = import("DLL")

local bytes = buffer.create(8)
buffer.writeu32(bytes, 0, 0xFFFFFFFF)
buffer.writeu32(bytes, 4, 0x7FFFFFFF)
local big = DLL.Alloc(8)
big:WriteBuffer(bytes)
print(big:Read("u32"), big:Read("u32", 4))
```

### Offset

```luau
pointer:Offset(bytes: number): Pointer
```

Returns a new Pointer `bytes` further along. `bytes` must be a whole number and can be negative. The new Pointer shares the memory of the old one. It keeps that memory alive and uses the same bounds.

```luau
local DLL = import("DLL")

local memory = DLL.Alloc(16)
local tail = memory:Offset(12)
print(tail.Size, pcall(function()
	return tail:Read("i64")
end))
```

### Free

```luau
pointer:Free()
```

Frees owned memory right away. You rarely need this, because luv frees owned memory on its own. See [Ownership](#ownership).

It only works on the Pointer that [DLL.Alloc](dll.md#alloc), [DLL.New](dll.md#new), [DLL.String](dll.md#string) or `New` gave you. It errors when:

- The Pointer came from [Offset](#offset). The message is `only the Pointer returned by DLL.Alloc, DLL.New or DLL.String can free its memory`.
- The Pointer is not owned. The message is `only memory from DLL.Alloc, DLL.New or DLL.String can be freed`.
- The memory was already freed. The message is `the memory behind this Pointer has been freed`.

When a call or a render hook still uses the memory, it stays valid for them until they let go. Every read and write from Luau errors after `Free`.

## Operators

| Operator | Result |
| --- | --- |
| `a == b` | `true` when both have the same address. |
| `tostring(p)` | `Pointer(0x1f4a20)` with the address in hex, or `Pointer(null)`. |

In `types.d.luau` these two operators make up the `PointerMetatable` type.

## Bounds checks

luv checks every read and write through owned and object pointers. Reads and writes that go past the end, or before the start, error:

| Message | Cause |
| --- | --- |
| `accessing 8 bytes at offset 12 is outside the 16 bytes of this memory` | The access does not fit in owned memory. |
| `accessing 8 bytes at offset 0 is outside the 4 bytes of this Counter` | The access does not fit in a plugin object. The message names its class. |
| `cannot access memory through a null Pointer` | The address is 0. |
| `the memory behind this Pointer has been freed` | The memory was freed. |
| `the offset moves the Pointer outside of memory` | The offset goes past the lowest or highest address. |
| `an offset must be a whole number, got 1.5` | The offset has a fraction. |

Foreign pointers are not checked. A wrong address or size can crash the game.

## Ownership

- Owned memory is freed when no Pointer to it is left, no call uses it and no render hook holds it. Pointers made with [Offset](#offset) count too.
- [Free](#free) frees it earlier.
- luv never frees memory that C gives you. If the library wants you to free it, call its own free function.
- Writing a Pointer into memory stores only its address. It does not keep that memory alive. Keep the Pointer in a variable too.
- A Pointer from [Library:GetSymbol](library.md#getsymbol) or `NativeFunction.Pointer` keeps its library loaded.
- A Pointer to a plugin object keeps the object alive.
