# ExternalModule

Inherits: [BaseGameObject](basegameobject.md)

A Luau file from outside the game, compiled and ready to run. [ECall](globals.md#ecall) makes one.

## Description

`ECall` reads a file from disk, compiles it and hands back an ExternalModule. Nothing runs yet. [Fetch](#fetch) runs it and gives you whatever it returned, and every Fetch after that gives the same thing back.

The file never joins the game. luv keeps the compiled code and the value the file returned, not the file itself, so the game folder is untouched and nothing is written anywhere.

Each module gets a name inside `mods`. A file called `greeter.luau` is `@mods/greeter`, and that is the name you see in error messages and stack traces.

```luau
local handle = ECall("mods/greeter.luau")
local greeter = handle:Fetch()
print(greeter.greet("world"))
```

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"ExternalModule"`. Read only. |
| `Name` | `string` | The file name without its extension. For `mods/greeter.luau` it is `greeter`. |
| `Path` | `string` | The full path of the file that was read. Read only. |
| `Module` | `string` | The name inside `mods`, like `@mods/greeter`. Read only. |
| `IsLoaded` | `boolean` | `true` once the file has run and its value is kept. Read only. |

## Methods

### Fetch

```luau
handle:Fetch(): any
```

Runs the file and returns what it returned. This yields the calling coroutine while it runs, so the file can use anything the rest of the game can.

luv keeps the value against the full path of the file. So the second Fetch does not run the file again, and an `ECall` of the same path anywhere else gets the same value.

An error inside the file comes back out of Fetch.

```luau
local handle = ECall("mods/greeter.luau")
local first = handle:Fetch()
local second = handle:Fetch()
print(first == second)
```

### Drop

```luau
handle:Drop(): boolean
```

Forgets the kept value. Returns `true` when there was one, and `false` when there was nothing to forget. The next Fetch runs the file again from the start.

Drop does not reach into anything that already has the value. See [Dropping is not unloading](#dropping-is-not-unloading).

```luau
local handle = ECall("mods/settings.luau")
print(handle:Fetch().volume)
handle:Drop()
print(handle:Fetch().volume)
```

## Dropping is not unloading

Drop only takes the value out of the cache. Whoever already holds it keeps it, and the module stays in memory until every one of them lets go.

That matters when modules lean on each other. Say `ui.luau` fetched `theme.luau` and kept it:

```tree
mods/
├── theme.luau
└── ui.luau
```

| File | What it holds |
| --- | --- |
| `theme.luau` | A table of colors. |
| `ui.luau` | The table from `theme.luau`, kept in a local. |

Dropping `theme` now leaves you with this:

- `ui` still reads the old table, because it has it.
- The next Fetch of `theme` runs the file again and makes a second table.
- Two tables are alive at once, and changing one does not change the other.

So drop a module only when nothing else is holding it. Drop the modules that depend on it first, then drop it. When you are not sure what holds what, leave it cached.

```luau
local theme = ECall("mods/theme.luau")
local ui = ECall("mods/ui.luau")

ui:Drop()
theme:Drop()
```

## Errors

| Message | Cause |
| --- | --- |
| `ECall needs the path of a Luau file` | The path is empty or only spaces. |
| `cannot read <path>: ...` | The file is missing or could not be read. |
| A syntax error with the path in front | The file does not compile. |
| `<path> was dropped and cannot run again` | The ExternalModule was destroyed with [Destroy](basegameobject.md#destroy). |
