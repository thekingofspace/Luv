# Exception

Finds every error in the game and shows where it came from.

```luau
local Exception = import("Exception")

Exception.Raised:BindHandler("log", function(exception: Exception)
	print(exception.Traceback)
end)
```

## Description

Errors can go missing. A packed game has no console, an error in a parallel thread happens far from the main script, and code inside `pcall` swallows its errors on purpose. Exception gathers them in one place.

[Raised](#raised) fires for every error, with the file and the line it came from:

| Error | Reaches Raised |
| --- | --- |
| An error nobody caught, in a script, a handler or a coroutine | Yes. |
| An error luv reports for you, like a render problem | Yes. |
| An error in a parallel thread | Yes, in that thread and on the main thread. |
| An error caught with [epcall](globals.md#epcall) | Yes, marked as `Caught`. |
| An error caught with `pcall` | No. Use `epcall` to have it reported. |

Uncaught errors are still printed and still counted in the error box of a packed game. Exception adds to that and takes nothing away.

The tools below also read the stack of the running code, so you can log where a call came from without an error at all.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `Raised` | [Signal](signal.md)`<Exception>` | Fires for every error. See [Raised](#raised). Read only. |
| `Count` | `number` | How many errors this thread has seen. Read only. |

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [Traceback](#traceback)(message, level) | `string` | no |
| [GetStack](#getstack)(level) | `{ StackFrame }` | no |
| [Caller](#caller)(level) | `StackFrame?` | no |
| [Try](#try)(body, ...) | `boolean, ...any` | when `body` yields |
| [GetRecent](#getrecent)() | `{ Exception }` | no |
| [ClearRecent](#clearrecent)() | none | no |

## Raised

Fires once for every error, with an [Exception](#exception-object). It fires just after the error, on the next turn of the engine, never in the middle of the code that failed.

```luau
local Exception = import("Exception")

Exception.Raised:BindHandler("report", function(exception: Exception)
	print(`{exception.Source}:{exception.Line} in {exception.Thread}`)
	print(exception.Message)
end)
```

On the main thread it also fires for errors from every parallel thread. Inside a parallel thread it only fires for that thread.

A handler that errors is reported like any other error, but it does not fire Raised again. So a broken handler cannot loop.

## Function descriptions

### Traceback

```luau
Exception.Traceback(message: any?, level: number?): string
```

The message followed by one line for each call on the stack, starting at the function that called Traceback.

```luau
local Exception = import("Exception")

local parts = {}
function parts.load()
	print(Exception.Traceback("loading"))
end
parts.load()
```

```text
loading
  at src/main.luau:5 in load
  at src/main.luau:7
```

`level` skips that many calls first. `1`, the default, starts at the function that called Traceback. `2` starts at the function that called that one.

### GetStack

```luau
Exception.GetStack(level: number?): { StackFrame }
```

The same stack as [Traceback](#traceback), as a list of [StackFrame](#stackframe) tables, from the innermost call out.

```luau
for _, frame in Exception.GetStack() do
	print(frame.Source, frame.Line, frame.Name)
end
```

### Caller

```luau
Exception.Caller(level: number?): StackFrame?
```

The [StackFrame](#stackframe) of whoever called the function you are in. `nil` when there is nobody, at the top of a script.

```luau
local Exception = import("Exception")

local Save = {}
function Save.write(data: any)
	local from = Exception.Caller()
	print(`Save.write called from {from.Source}:{from.Line}`)
end
```

`level` goes further out. `Caller(2)` is the caller of the caller.

### Try

```luau
Exception.Try(body: (...any) -> ...any, ...: any): (boolean, ...any)
```

Runs `body` with the values after it, like `pcall`. When it works, you get `true` and what it returned. When it fails, you get `false` and an [Exception](#exception-object) with the stack from the moment of the error.

```luau
local ok, result = Exception.Try(loadLevel, "caves")
if not ok then
	print(result.Traceback)
end
```

Try is for handling an error yourself, so it does not fire [Raised](#raised) and does not count. To catch an error and still have it reported, use [epcall](globals.md#epcall).

`body` can wait for things, the same as with `pcall`.

### GetRecent

```luau
Exception.GetRecent(): { Exception }
```

The last 50 errors this thread saw, oldest first. Useful to look back after something went wrong, even before a handler was bound.

### ClearRecent

```luau
Exception.ClearRecent()
```

Forgets the errors that [GetRecent](#getrecent) keeps. `Count` is not reset.

## Exception object

A frozen table that describes one error.

| Name | Type | Description |
| --- | --- | --- |
| `Message` | `string` | The text of the error, as Luau shows it, like `src/main.luau:10: boom`. |
| `Value` | `any` | The value that was raised. For `error({ code = 7 })` it is that table. For an error from luv it is the message. |
| `Source` | `string?` | The script the error came from, like `src/main.luau`. |
| `Line` | `number?` | The line in that script. |
| `Stack` | `{ StackFrame }` | Every call on the stack when the error happened, innermost first. See [StackFrame](#stackframe). |
| `Traceback` | `string` | `Message` followed by the stack, one call on each line. |
| `Thread` | `string` | `main`, or the name of the thread it happened in, like `parallel block #1 of src/main.luau`. |
| `Caught` | `boolean` | `true` when [epcall](globals.md#epcall) or [Try](#try) caught it. |
| `Time` | `number` | Seconds since the game started. |

## StackFrame

A frozen table for one call on the stack.

| Name | Type | Description |
| --- | --- | --- |
| `Source` | `string` | The script, like `src/Modules/Hud.luau`, or `[C]` for a function built into luv or Luau. |
| `Line` | `number?` | The line running in that call. `nil` for a built in function. |
| `Name` | `string?` | The name of the function, when it has one. |
| `IsNative` | `boolean` | `true` for a function built into luv or Luau. |

## Folded functions

luv compiles your scripts with every speed setting on. A small `local function` can be folded into the function that calls it, and then it has no line of its own in the stack. The line numbers stay right, only the name is missing.

Functions stored in tables are never folded, so `function Module.load()` always shows up by name.
