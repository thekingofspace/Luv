# ArrayType

A C array with a fixed length, made with [DLL.Array](dll.md#array).

## Description

Pass the element type and the length to [DLL.Array](dll.md#array). The length must be a whole number from 1 to 4294967296. The element can be any [type name](dll.md#type-names) except `"void"`. It can also be a [StructType](structtype.md) or another ArrayType.

```luau
local DLL = import("DLL")

local Numbers = DLL.Array("i32", 5)
local Grid = DLL.Array(DLL.Array("u8", 4), 4)
print(Numbers.Size, Numbers.Length, Grid.Size)
```

Arrays work in these places:

- As fields of a [StructType](structtype.md).
- In memory, with [New](#new), [DLL.New](dll.md#new), [Pointer:Read](pointer.md#read) and [Pointer:Write](pointer.md#write).
- With [DLL.SizeOf](dll.md#sizeof) and [DLL.AlignOf](dll.md#alignof).

Arrays cannot be arguments or return values. Put the array in memory and pass its Pointer.

This C function takes an array and a callback:

```c
#include "luv.h"

LUV_EXPORT int32_t reduce(const int32_t* values, size_t count, int32_t (*combine)(int32_t, int32_t)) {
    int32_t total = values[0];
    for (size_t index = 1; index < count; index++) {
        total = combine(total, values[index]);
    }
    return total;
}
```

Luau puts the array in memory and passes the Pointer:

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
local reduce = mathlib:GetFunction("reduce", "i32", { "pointer", "usize", "pointer" })
local combine = DLL.Callback("int", { "int", "int" }, function(a: number, b: number)
	return a + b
end)
local values = DLL.New(DLL.Array("i32", 5), { 1, 2, 3, 4, 5 })
print(reduce(values, 5, combine))
```

`DLL.Array` errors with `an array length must be a whole number of at least 1, got 0` for a bad length.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `Size` | `number` | The element size times the length, in bytes. Read only. |
| `Alignment` | `number` | The alignment of the element type. Read only. |
| `Length` | `number` | The number of elements. Read only. |

## Methods

### New

```luau
arrayType:New(values: { any }?): Pointer
```

Allocates owned memory for the array, fills it with zeros and writes `values` into it. Returns a [Pointer](pointer.md). This is the same as `DLL.New(arrayType, values)`. See [Values](#values) for what you can pass.

```luau
local DLL = import("DLL")

local Numbers = DLL.Array("i32", 5)
local numbers = Numbers:New({ 5, 3, 9 })
print(table.concat(numbers:Read(Numbers), ","))
```

## Operators

| Operator | Result |
| --- | --- |
| `tostring(a)` | The element type and the length, like `ArrayType(i32[5])`. |

`typeof(a)` is `"ArrayType"`. An ArrayType has no `==` operator. Two arrays made by different calls are never equal.

## Values

luv turns these Luau values into an array:

| Value | How it is used |
| --- | --- |
| Table | Elements are read from index 1 up to the length. The array is set to zeros first, so missing elements are 0. |
| `string` or `buffer` | Only when each element is 1 byte. luv copies the bytes up to the size of the array. The rest stays 0. |
| [Pointer](pointer.md) | luv copies the whole array from that address. |

Reading an array gives a table that starts at 1. A `char` array reads as numbers. To read it as text, use [Pointer:ReadString](pointer.md#readstring) with the offset of the array.

A wrong element value names the element, like `element #2: i32 expects a whole number, got string`.
