# Registry

Keeps named values in one place, like the items of a game or what mods add to it.

```luau
local Registry = import("Registry")

local items = Registry.new("Items")
items:Register("items.sword", { damage = 5 })
print(items:Get("items.sword").damage)
```

## Description

A registry is a list of values, each under an id. Ids are names split by dots, like `"sword"` or `"items.weapons.sword"`, so you can group them and ask for a whole group at once.

Values in a registry cannot be changed in place. Tables are frozen, all the way down, when you register them. To change one, register it again. That way every change goes through the registry, and [Changed](#changed) tells everyone who cares.

There are two kinds.

| Kind | Made with | Holds | Shared with |
| --- | --- | --- | --- |
| A registry | [Registry.new](#new) | Any value, functions and engine objects included. | This thread only. |
| A safe registry | [Registry.Safe](#safe) | Values that can travel between threads. | Every thread of the game. |

Use `Registry.new` for game objects and code. Use `Registry.Safe` for data that parallel threads also need, like the stats of every item.

## Functions

### new

```luau
Registry.new(name: string): Registry
```

The registry of this thread with this name. Asking again with the same name gives the same registry.

It holds any value. It is only seen by the thread that made it.

### Safe

```luau
Registry.Safe(name: string): Registry
```

The safe registry with this name. Every thread that asks for the same name gets the same values.

It only holds values that can travel between threads, see [Captured locals](../manual/parallel.md#captured-locals). Registering a function errors with `Registry.Safe cannot keep '<id>': function values cannot be sent between threads`.

When one thread registers a value, the [Changed](#changed) of the same registry fires in every other thread that asked for it.

Reading is fast. Each thread turns a value into Luau once and keeps it, and only reads it again after someone registers it again.

## Registry object

### Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"Registry"`. Read only. |
| `Name` | `string` | The name it was made with. Read only. |
| `IsSafe` | `boolean` | `true` for a registry from [Registry.Safe](#safe). Read only. |
| `Count` | `number` | How many ids it holds. Read only. |
| `Staged` | `number` | How many changes wait for [Commit](#commit). Read only. |
| `Changed` | [Signal](signal.md)`<string, any>` | See [Changed](#changed). Read only. |

### Register

```luau
registry:Register(id: string, value: any)
```

Puts `value` under `id`, replacing what was there, and fires [Changed](#changed). Tables are frozen. Registering `nil` removes the id.

### Stage

```luau
registry:Stage(id: string, value: any)
```

Keeps a change aside without applying it. Nothing can read it until [Commit](#commit). Use it to prepare many changes and apply them together, like every item of a mod.

### Commit

```luau
registry:Commit(): number
```

Applies every staged change in the order they were staged and fires [Changed](#changed) for each. Returns how many there were.

### Discard

```luau
registry:Discard(): number
```

Throws away every staged change. Returns how many there were.

### Get

```luau
registry:Get(id: string): any
```

The value under `id`, or `nil`.

### Has

```luau
registry:Has(id: string): boolean
```

Whether anything is under `id`.

### Remove

```luau
registry:Remove(id: string): boolean
```

Removes `id` and fires [Changed](#changed) with `nil`. Returns `false` when there was nothing to remove.

### List

```luau
registry:List(prefix: string?): { string }
```

Every id, sorted. With a `prefix`, only that id and the ids under it. `"items"` finds `"items"` and `"items.sword"`, but not `"itemsets"`.

```luau
for _, id in items:List("items.weapons") do
	print(id)
end
```

### GetAll

```luau
registry:GetAll(prefix: string?): { [string]: any }
```

The same ids as [List](#list), as a table of each id to its value.

### Clear

```luau
registry:Clear(): number
```

Removes every id and fires [Changed](#changed) for each. Returns how many there were.

## Changed

Fires with the id and its new value whenever something is registered or removed. The value is `nil` when the id was removed.

```luau
items.Changed:BindHandler("refresh", function(id: string, value: any)
	if value == nil then
		print(id, "was removed")
	else
		print(id, "is now", value.damage)
	end
end)
```

## For mods

A registry is a simple way to let mods add to a game. The game makes the registry and hands it out, and each mod stages what it adds and commits once.

```luau
local Registry = import("Registry")

global.new("Items", Registry.new("Items"))

for _, folder in { "mods/hats", "mods/swords" } do
	ecall(folder):Fetch()
end
```

```luau title="mods/swords/init.luau"
Items:Stage("items.sword.fire", { damage = 12, element = "fire" })
Items:Stage("items.sword.ice", { damage = 10, element = "ice" })
Items:Commit()
```
