# global

Adds your own globals, imports and window APIs.

```luau
global.new("Version", "1.4")
print(Version)
```

## Description

`global` is a global, so every script has it without an `import`. It is how a game hands its own tools to the rest of its scripts, and to mods loaded with [ecall](globals.md#ecall).

| Function | What it adds | Reached with |
| --- | --- | --- |
| [global.new](#new) | A global value. | Its name, anywhere. |
| [global.newImport](#newimport) | An import. | `import("Name")` |
| [global.newAPI](#newapi) | A window API. | `window:GetAPI("Name")` |

Each thread has its own globals, imports and window APIs. To set them up in every thread at once, do it in a [boot script](../manual/scripts.md#boot-scripts).

## Functions

### new

```luau
global.new(name: string, value: any)
```

Puts `value` in the globals under `name`. The value can be anything, a function, a table, an object.

```luau
global.new("spawnEnemy", function(kind: string)
	print("spawning", kind)
end)
```

The name uses letters, digits and underscores and cannot start with a digit. The names luv owns cannot be replaced, and trying errors with `'task' belongs to luv and cannot be replaced`. Those names are `ecall`, `import`, `require`, `enum`, `udim`, `color`, `promise`, `switch`, `task`, `global`, `epcall`, `SetGlobal`, `_G` and `print`.

`global.new` replaces [SetGlobal](globals.md#setglobal), which still works but is deprecated.

### newImport

```luau
global.newImport(name: string, value: any)
```

Makes `import(name)` give back `value`. Use it for a library of your own that every script imports the same way as the ones luv has.

```luau
local Items = {}
function Items.get(id: string)
	return nil
end

global.newImport("Items", Items)
```

```luau
local Items = import("Items")
```

The name cannot be one that luv already imports. `global.newImport("Asset", ...)` errors. Giving the same name again replaces the value.

### newAPI

```luau
global.newAPI(name: string, api: { [string]: any })
```

Makes `window:GetAPI(name)` give back your API for any window. Every function in it gets the window as its first value, so one API can serve every window.

```luau
local Hud = {}

function Hud.Show(window: any, text: string)
	print(`showing {text} in {window.Title}`)
end

global.newAPI("Hud", Hud)
```

```luau
local hud = window:GetAPI("Hud")
hud.Show("Game over")
hud:Show("Game over")
```

Both calls run `Hud.Show(window, "Game over")`. The call with `:` works the same as the one with `.`, so it reads like the APIs luv has.

What you get from `GetAPI` reads from your table as it is now, so functions you add later work too. You cannot write to it. `hud.Title = "x"` errors with `the Hud API cannot be changed through GetAPI`. Values that are not functions are given as they are.

The name cannot be one of the window APIs of luv, like `Sound` or `Renderable`.

## Types

Your editor does not know about these on its own. To type them, write a `.d.luau` file anywhere in your project. luv folds it into `types.d.luau`. See [Type files](../manual/native-plugins.md#type-files).

```luau title="src/Hud.d.luau"
export type Hud_API = {
	Show: (text: string) -> (),
}

export type WindowAPIs = {
	Hud: Hud_API,
}
```
