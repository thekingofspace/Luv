# Library

Inherits: [BaseGameObject](basegameobject.md)

A native library loaded with [DLL.Load](dll.md#load).

## Description

You get a `Library` from [DLL.Load](dll.md#load). It finds exported symbols and makes [NativeFunction](nativefunction.md) objects from them.

Each library gets its own thread, named `dll` followed by the file name. luv loads the library on that thread and runs `luv_register` there. Calls to the library run there too, one at a time. See [Which thread runs a call](nativefunction.md#which-thread-runs-a-call).

The library stays loaded while anything still uses it. That includes the Library object, its functions, Pointers from [GetSymbol](#getsymbol), render hooks and plugin objects. A library that defines plugin classes stays loaded until the game ends.

Loading the same file twice gives two Library objects for one loaded library. They share its global variables.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"Library"`. Read only. |
| `Name` | `string` | The last part of the path you gave to `DLL.Load`. For `"./cube"` it is `"cube"`. |
| `Path` | `string` | The full path of the file that was loaded. For a library that the system found, it is the name that worked, like `kernel32.dll`. Read only. |
| `Exports` | `{ [string]: any }` | The classes and functions the library adds with the plugin API. Read only. See [Exports](#exports). |

## Exports

`Exports` is a read only table. It is empty when the library has no `luv_register` function. A plugin library fills it with:

- One entry for each class. It is a read only table with the static functions and static properties of the class.
- One entry for each exported function. It is a Luau function.

```luau
local DLL = import("DLL")

local fx = DLL.Load("./particles")
local ParticleSystem = fx.Exports.ParticleSystem
local particles = ParticleSystem.new()
particles.Drag = 2.4
local bytes = fx.Exports.blip(760, 0.08, 600, 0.4, 0.2)
```

See [How exports look in Luau](native-c.md#how-exports-look-in-luau) for methods, properties and operators. After [Destroy](#destroy), reading `Exports` errors.

## Methods

### HasSymbol

```luau
library:HasSymbol(name: string): boolean
```

Returns `true` when the library exports a function or variable with this name.

It errors with `'<name>' is not a valid symbol name` when the name is empty or has a zero byte in it. [GetSymbol](#getsymbol) and [GetFunction](#getfunction) check names the same way.

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
if mathlib:HasSymbol("add") then
	print("add is there")
end
```

### GetSymbol

```luau
library:GetSymbol(name: string): Pointer
```

Returns a foreign [Pointer](pointer.md) to an exported function or variable. The Pointer keeps the library loaded.

Use it to read and write exported variables, to make a function with [DLL.Function](dll.md#function), or to set a `RenderHook`. See [Render hooks](../manual/render-hooks.md).

It errors with `<path> has no exported symbol named 'x'` when the symbol is missing.

This C file exports a variable:

```c
#include "luv.h"

LUV_EXPORT int32_t counter = 7;
```

Luau reads and writes it through the Pointer:

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
local counter = mathlib:GetSymbol("counter")
print(counter:Read("i32"))
counter:Write("i32", 9)
```

### GetFunction

```luau
library:GetFunction(name: string, returnType: DLLType, argumentTypes: { DLLType }?, options: NativeFunctionOptions?): NativeFunction
```

Finds the symbol `name` and makes a [NativeFunction](nativefunction.md) for it. The function is named `name`. Calls run on the thread of this library.

`returnType` and `argumentTypes` use the [type names](dll.md#type-names). `options` is a [NativeFunctionOptions](nativefunction.md#nativefunctionoptions) table. Leave `argumentTypes` out when the function takes no arguments.

It errors when the symbol is missing or a type is not valid.

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
local add = mathlib:GetFunction("add", "int", { "int", "int" })
local greeting = mathlib:GetFunction("greeting", "string")
print(add(40, 2), greeting())
```

### Destroy

```luau
library:Destroy()
```

Stops the thread of the library and lets go of the `Exports` table, so its functions and classes are collected instead of living until the Library itself is. Calls that already wait still finish. After this:

- `Exports`, `HasSymbol`, `GetSymbol` and `GetFunction` error with `Library 'mathlib' has been destroyed`.
- Its functions error with `cannot call add because its library was unloaded`. Functions made with `Parallel = true` keep working.
- Plugin members with `LUV_WORKER` error the same way. Members with `LUV_INLINE` and `LUV_PARALLEL` keep working.

The library stays in memory while other things still use it. See [BaseGameObject](basegameobject.md#destroy).
