# NativeFunction

Inherits: [BaseGameObject](basegameobject.md)

A C function that you call from Luau.

## Description

You get a `NativeFunction` from [Library:GetFunction](library.md#getfunction) or [DLL.Function](dll.md#function). Call it like a Luau function, or with its [Call](#call) method. Both do the same thing.

```luau
local DLL = import("DLL")

local mathlib = DLL.Load("./mathlib")
local add = mathlib:GetFunction("add", "int", { "int", "int" })
print(add(40, 2))
print(add:Call(-5, 3))
```

A call converts each argument to its C type, runs the function on another thread and converts the result back. See [Converting values](dll.md#converting-values).

Every call yields the calling coroutine until the C function returns. Other coroutines, signals and frames keep running while it waits. See [Yielding and coroutines](../manual/yielding.md).

How arguments work:

- More arguments than argument types is an error.
- A missing argument counts as `nil`. `nil` becomes NULL for `pointer`, `string` and `wstring`, and `false` for `bool`. For number types and structs it is an error.
- A struct argument is passed by value. A struct return value comes back as a table.
- Arrays cannot be arguments or return values. Pass a pointer instead.
- Variadic C functions like `printf` are not supported.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"NativeFunction"`. Read only. |
| `Name` | `string` | The symbol name for a function from GetFunction. `function at 0x` and the address for a function from DLL.Function. |
| `Pointer` | [Pointer](pointer.md) | The address of the function. It keeps the library loaded. Read only. |

## Methods

### Call

```luau
fn:Call(...: any): ...any
```

Calls the function. This is the same as `fn(...)`. It returns the converted result. With [ErrorCode](#nativefunctionoptions) it also returns the error code as a second value. This yields the calling coroutine.

### Destroy

```luau
fn:Destroy()
```

Marks the function as destroyed. Calling it after that errors with `NativeFunction 'add' has been destroyed`. The library is not affected. See [BaseGameObject](basegameobject.md#destroy).

## Which thread runs a call

| How the function was made | Where calls run |
| --- | --- |
| From a library symbol, with GetFunction or DLL.Function | The thread of that library. Its name is `dll` followed by the file name. |
| From any other pointer | One shared thread named `dll calls`. |
| With `Parallel = true` | A thread from a pool. |

Calls to one library run one at a time, in the order they were made. They always run on the same system thread. This suits C libraries that must be used from one thread.

A slow call makes the next calls to the same library wait. It never stops Luau.

With `Parallel = true`, calls run at the same time as other calls. The C function must be thread safe.

## NativeFunctionOptions

The options table of [Library:GetFunction](library.md#getfunction) and [DLL.Function](dll.md#function).

| Name | Type | Description |
| --- | --- | --- |
| `ErrorCode` | `boolean?` | When `true`, luv clears the last system error right before the call and reads it right after, on the same thread. The call then returns two values: the result and the code. The code is `GetLastError()` on Windows and `errno` on Linux. Default `false`. |
| `Parallel` | `boolean?` | When `true`, calls run on pool threads and can overlap. Default `false`. |

Any other key errors with `unknown function option 'Nope', the options are ErrorCode and Parallel`.

A `void` function returns `nil` as its first value, so skip it when you only want the code.

```luau
local DLL = import("DLL")

local kernel32 = DLL.Load("kernel32")
local deleteFile = kernel32:GetFunction("DeleteFileW", "int", { "wstring" }, { ErrorCode = true })
local deleted, code = deleteFile("C:/missing/file.txt")
print(deleted, code)
```

## Errors

| Message | Cause |
| --- | --- |
| `'int32' is not a DLL type, the types are ...` | A type name is wrong. |
| `argument type #1: void has no values, it can only be a return type` | `"void"` is in the argument types. |
| `argument type #1: arrays cannot be passed by value, pass a pointer instead` | An ArrayType is in the argument types. |
| `arrays cannot be returned by value, return a pointer instead` | The return type is an ArrayType. |
| `add takes 2 arguments, got 3` | The call has too many arguments. |
| `argument #1 of add: i32 expects a whole number, got 1.5` | An argument does not fit its type. |
| `cannot call add because its library was unloaded` | The library was destroyed. |
| `NativeFunction 'add' has been destroyed` | The function was destroyed. |
