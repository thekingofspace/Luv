# Callback

Inherits: [BaseGameObject](basegameobject.md)

A Luau function that C code can call.

## Description

You make a `Callback` with [DLL.Callback](dll.md#callback). Pass the Callback, or its `Pointer`, to a C function that takes a function pointer. Use the `"pointer"` type for that argument.

This C function calls the function pointer it gets:

```c
#include "luv.h"

LUV_EXPORT int32_t apply(int32_t (*callback)(int32_t), int32_t value) {
    return callback(value) + 1;
}
```

Luau passes a Callback to it:

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
local apply = mathlib:GetFunction("apply", "i32", { "pointer", "i32" })
local double = DLL.Callback("i32", { "i32" }, function(value: number)
	return value * 2
end)
print(apply(double, 20))
```

When C calls the callback:

1. luv converts the arguments to Luau values. See [C to Luau](dll.md#c-to-luau). Strings are copied right away.
2. The handler runs on the thread that runs Luau, in a new coroutine. It can yield, for example to call other native functions.
3. The C thread waits until the handler returns.
4. luv converts the first return value to the return type. See [Luau to C](dll.md#luau-to-c). Other return values are ignored.

A handler cannot return a Luau string for a `string` or `wstring` return type. Return a Pointer from [DLL.String](dll.md#string) instead, and keep that Pointer alive. A plain Luau string errors with `a callback cannot return a Luau string as a C string, return a Pointer from DLL.String instead`.

When the handler errors, luv reports it like any other uncaught error, and C gets 0.

Callbacks work with struct arguments and results too. This C function passes a struct to its callback:

```c
typedef struct Vec2 {
    float x;
    float y;
} Vec2;

LUV_EXPORT double average(Vec2 (*callback)(Vec2), Vec2 value) {
    Vec2 result = callback(value);
    return (result.x + result.y) / 2.0;
}
```

The handler gets a table and returns a table:

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
local Vec2 = DLL.Struct({ { "x", "f32" }, { "y", "f32" } })
local average = mathlib:GetFunction("average", "double", { "pointer", Vec2 })
local swap = DLL.Callback(Vec2, { Vec2 }, function(value)
	return { x = value.y, y = value.x }
end)
print(average(swap, { x = 1, y = 3 }))
```

## Keep it referenced

C only holds the address. Keep the Callback object in a variable or a table for as long as C may call it.

If the Callback is garbage collected, calls from C still return, but they return 0 without running the handler. A Pointer from `callback.Pointer` does not keep the handler alive.

Each Callback uses a small piece of native memory that luv keeps until the game ends. Make callbacks once and reuse them.

## Threads

C can call the callback from any thread, including threads that C starts itself.

When the calling thread is the thread of a library, or the shared `dll calls` thread, it keeps running other waiting calls while it waits for the handler. So the handler can call functions of the same library. Your C code must be ready for those calls while it waits.

Calling the callback on the thread that runs Luau does not work. It returns 0 right away without running the handler. This happens when a plugin function with `LUV_INLINE` calls it.

## What it returns when disabled

C gets 0, a null pointer, or a struct filled with zeros when:

- The Callback was destroyed.
- The Callback was garbage collected.
- The handler errored, or its result did not fit the return type.
- It was called on the thread that runs Luau.
- The game is closing.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"Callback"`. Read only. |
| `Name` | `string` | `"Callback"` unless you change it. |
| `Pointer` | [Pointer](pointer.md) | The address that C calls. Read only. It errors with `Callback 'Callback' has been destroyed` after [Destroy](#destroy). |

## Methods

### Destroy

```luau
callback:Destroy()
```

Turns the callback off and lets go of the handler function. Calls from C return 0 after this, and reading `Pointer` errors. See [BaseGameObject](basegameobject.md#destroy).

```luau
local DLL = import("DLL")

local increment = DLL.Callback("i32", { "i32" }, function(value: number)
	return value + 1
end)
print(increment.Pointer)
increment:Destroy()
print(pcall(function()
	return increment.Pointer
end))
```

The last line prints `false` and an error that says `Callback 'Callback' has been destroyed`.
