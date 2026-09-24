# Globals

Names that every script can use without `import` or `require`.

## Description

luv keeps the standard Luau libraries and adds the globals on this page. It also changes `print`, `require`, `coroutine.resume` and `coroutine.wrap`.

The main script and every module it loads share one global table. A parallel block starts with a fresh global table. It has the standard libraries and the globals on this page, but none of your own globals. See [Parallel code](../manual/parallel.md).

luv has no `wait`, `task` or `warn` globals. To wait for time, see [Waiting for time](../manual/yielding.md#waiting-for-time).

## Globals

| Name | Type | Description |
| --- | --- | --- |
| [import](#import) | function | Returns an engine library. |
| [print](#print) | function | Writes values to the output. |
| [require](#require) | function | Loads a module. |
| [enum](#enum) | table | Holds every enum. |
| [udim](#udim) | table | Makes [UDim](udim.md) values. |
| [color](#color) | table | Makes [Color](color.md) values. |
| [EnterParallel](#enterparallel) | function | Starts a parallel block. |
| [ExitParallel](#exitparallel) | function | Ends a parallel block. |
| [coroutine.resume](#coroutine-resume) | function | Resumes a coroutine. luv changes it. |
| [coroutine.wrap](#coroutine-wrap) | function | Wraps a function in a coroutine. luv changes it. |

## Global descriptions

### import

```luau
import(name: string)
```

Returns the engine library called `name`. Your editor knows the exact type of each library from its name.

```luau
local Process = import("Process")
local Signal = import("Signal")

local changed = Signal.new()
print(Process.gameName, changed.ClassName)
```

| Name | What you get |
| --- | --- |
| [Asset](asset.md) | Loads files from the `assets` folder. |
| [Bulk](bulk.md) | Sets many properties in one call. |
| [Container](container.md) | Finds and loads containers. |
| [Crypto](crypto.md) | Hashing, encryption, signing and random bytes. |
| [DLL](dll.md) | Loads native libraries and calls their functions. |
| [FS](fs.md) | Reads and writes files and folders. |
| [Messenger](messenger.md) | The Messenger object. It sends messages between threads. |
| [Net](net.md) | HTTP, TCP, UDP and WebSockets. |
| [Process](process.md) | The heartbeat, closing, exit, arguments and child processes. |
| [Random](random.md) | Random number generators with a seed. |
| [Serde](serde.md) | Encodes and decodes JSON, TOML and YAML. |
| [Shader](shader-library.md) | Compiles shaders. |
| [Signal](signal.md) | Makes new signals. |
| [Viewport](viewport.md) | Information about the screens. |
| [Window](window.md) | Opens windows. |

Names are case sensitive. Every call with the same name returns the same value, so `import("Messenger") == import("Messenger")` is `true`.

`import("Messenger")` returns the Messenger object itself. Every other name returns a read only table.

Each parallel block has its own copy of every library. You can use a library inside a parallel block or send it with Messenger. The other thread gets its own copy.

An unknown name raises an error that lists every name:

```text
'Physics' cannot be imported, the available imports are Asset, Bulk, Container, Crypto, DLL, FS, Messenger, Net, Process, Random, Serde, Shader, Signal, Viewport, Window
```

A native plugin can add a name of its own, called a service. It joins the list when [DLL.Load](dll.md#load) finishes, so load the library first. See [Services](native-c.md#services).

### ecall

```luau
ecall(path: string)
```

Reads a folder from outside the game and returns an [ExternalModule](externalmodule.md). Every Luau file in it becomes bytecode, every other file is kept as it is, and the whole folder is mounted in the files of the game under `mods`. Nothing runs yet. This yields the calling coroutine while the folder is read.

Use it for mods and for anything a player drops in after the game shipped. The folder is not copied and nothing is written to disk. luv holds the bytecode and the bytes in memory.

An absolute path is used as it is. A relative path is looked for in the game folder. A folder needs an `init.luau`, and a single `.luau` file works on its own.

```luau
local hat = ecall("mods/hat")
print(hat.Folder, hat.Files)
local api = hat:Fetch()
```

Because the folder is mounted, a mod can `require` its own scripts with `@self`, read its own files with [FS](fs.md) and load its own pictures and sounds with [Asset.Load](asset.md#load). See [What a mod can reach](externalmodule.md#what-a-mod-can-reach).

Call it with the same path twice and both handles share one value, so a mod is only ever run once until you drop it. [Drop](externalmodule.md#drop) forgets that value, and it is worth reading [Dropping is not unloading](externalmodule.md#dropping-is-not-unloading) first, because anything already holding the mod keeps it.

### SetGlobal

```luau
SetGlobal(name: string, value: any)
```

Puts a value in the globals of Luau, where every script can read it by name. Use it to hand mods the functions and values of your game.

The name uses letters, digits and underscores and cannot start with a digit. The value can be anything, a function, a table, a number.

```luau
SetGlobal("modApi", {
	spawn = function(kind: string) end,
	version = 3,
})

ecall("mods/hat"):Fetch()
```

```luau title="mods/hat/init.luau"
print(modApi.version)
modApi.spawn("hat")
```

The names luv owns cannot be replaced. `SetGlobal("import", ...)` errors with `'import' belongs to luv and cannot be replaced`. Those names are `ecall`, `import`, `require`, `enum`, `udim`, `color`, `SetGlobal` and `_G`.

Each parallel block has its own globals, so a value set on the main thread is not there. Send it with [Messenger](messenger.md) or set it again inside the block.

### print

```luau
print(...: any)
```

Writes the values to standard output. A tab goes between the values and a new line goes at the end. Each value goes through `tostring` first, so engine objects print their `Name`. Parallel blocks write to the same output.

```luau
local Signal = import("Signal")

print("Score:", 10, udim.new(1, 2), enum.KeyCode.Space, Signal.new())
```

This prints:

```text
Score:	10	UDim(1, 2, 0)	enum.KeyCode.Space	Signal
```

### require

```luau
require(path: string): any
```

Loads a module and returns its value. The path starts with `./`, `../` or `@`. A module runs once. Later calls return the same value. Each parallel block loads its own copy. See [Scripts and modules](../manual/scripts.md).

### enum

```luau
local mode = enum.WindowType.Borderless
```

A read only table with every enum type. Each enum type holds its items. See [Enums](enums.md).

### udim

```luau
local size = udim.new(640, 360)
```

A read only table with [udim.new](udim.md#new) and [udim.zero](udim.md#zero). See [UDim](udim.md).

### color

```luau
local tint = color.fromHex("#ff8000")
```

A read only table with [color.new](color.md#new), [color.fromRGB](color.md#fromrgb), [color.fromHex](color.md#fromhex), [color.fromHSV](color.md#fromhsv) and the [constants](color.md#constants) `color.white`, `color.black` and `color.transparent`. See [Color](color.md).

### EnterParallel

```luau
EnterParallel()
```

Marks the start of a parallel block. Each time the script reaches it, the code up to the matching `ExitParallel()` runs on a new thread. The script goes on without waiting. It must be a statement of its own, with no arguments. See [Parallel code](../manual/parallel.md).

### ExitParallel

```luau
ExitParallel()
```

Marks the end of a parallel block. It must be in the same block of code as its `EnterParallel()`.

```luau
local Messenger = import("Messenger")

EnterParallel()
local total = 0
for index = 1, 1000000 do
	total += index
end
Messenger:Fire("Total", total)
ExitParallel()
```

### coroutine.resume

```luau
coroutine.resume(co: thread, ...: any): (boolean, ...any)
```

Works like the standard function, with one change for engine waits. When the coroutine starts an engine wait, like [Signal:Wait](signal.md#wait), `coroutine.resume` returns only `true`. luv runs the rest of the coroutine when the wait ends.

While luv waits, `coroutine.status` returns `"suspended"`. Resuming it at that time returns `false` and `cannot resume a coroutine that is waiting on the engine`. Once the coroutine calls `coroutine.yield`, you can resume it yourself again.

An error after an engine wait prints as an uncaught error, because no caller is left to get it.

### coroutine.wrap

```luau
coroutine.wrap(f: (...any) -> ...any): (...any) -> ...any
```

Works like the standard function, with the same change as `coroutine.resume`. A call returns as soon as the coroutine starts an engine wait. Errors are raised again with their original value, so a table error stays a table.

```luau
local Process = import("Process")

coroutine.wrap(function()
	Process.Heartbeat:Wait()
	print("one tick later")
end)()
print("this prints first")
```

See [Yielding and coroutines](../manual/yielding.md) for more.

## Type names

`typeof` gives these names for engine values:

| Value | `typeof` result |
| --- | --- |
| [UDim](udim.md) | `"UDim"` |
| [Color](color.md) | `"Color"` |
| An enum item | `"EnumItem"` |
| [Signal](signal.md) | `"Signal"` |
| [Messenger](messenger.md) | `"Messenger"` |
| A library from `import`, other than Messenger | `"table"` |
| An engine error caught with `pcall` | `"error"` |

To check the class of any engine object, read its [ClassName](basegameobject.md#properties).

## Errors from engine functions

When an engine function fails, it raises an error object, not a string. `typeof(err)` is `"error"`. `tostring(err)` starts with `runtime error:` and the message. A stack traceback follows on the next lines. Use `string.find` to check for a message.

```luau
local Signal = import("Signal")

local signal = Signal.new()
signal:Destroy()

local ok, err = pcall(signal.Fire, signal)
print(ok, typeof(err))
print(string.find(tostring(err), "has been destroyed", 1, true) ~= nil)
```

This prints `false` and `error`, and then `true`.

Errors you raise with `error` keep their value. A string message gets the script path and line in front, like `src/main.luau:3: boom`.

An error that no code catches prints as `error: <message>`, and the game keeps running. See [Errors in handlers](../manual/yielding.md#errors-in-handlers).
