# Signal

Inherits: [BaseGameObject](basegameobject.md)

Runs handlers when something happens.

```luau
local Signal = import("Signal")

local damaged = Signal.new()
```

## Description

Make your own signal with [Signal.new](#new). Engine objects have signals too, like [Process.Heartbeat](process.md#heartbeat) and the signals of a [Window](window.md).

A signal keeps a list of handlers. Each handler has a string id that you pick. [Fire](#fire) runs every handler. [Invoke](#invoke) runs one handler and returns its result. [Wait](#wait) pauses the calling coroutine until the next fire.

A signal only works in the thread that made it. You cannot send it with [Messenger](messenger.md) or use it inside a parallel block. Use Messenger to talk between threads.

It also has every member of [BaseGameObject](basegameobject.md). Its `ClassName` is `"Signal"`.

## The Signal type

`Signal<T...>` is the type of a signal that passes values of the types `T...`. Engine signals have fixed types. For example `Process.Heartbeat` is a `Signal<number>`. Give your own signals a type, and your editor checks the handlers and the values you fire.

```luau
local Signal = import("Signal")

local damaged: Signal<number, string> = Signal.new()
damaged:BindHandler("log", function(amount, source)
	print(amount, source)
end)
damaged:Fire(10, "lava")
```

## Constructor

### new

```luau
Signal.new(): Signal
```

Makes a new signal. Its `Name` is `"Signal"`.

## Methods

### BindHandler

```luau
signal:BindHandler(id: string, handler: (T...) -> ...any)
```

Adds a handler under the id `id`. Handlers run in the order you bind them. The same function can be bound under more than one id.

Each id can only be bound once. Binding it again raises `handler '<id>' is already bound to <Name>, call UnBind("<id>") before binding it again`.

### UnBind

```luau
signal:UnBind(id: string): boolean
```

Removes the handler with this id. Returns `true` when a handler was removed and `false` when the id was not bound.

### IsBound

```luau
signal:IsBound(id: string): boolean
```

Returns `true` when a handler is bound under this id.

### Fire

```luau
signal:Fire(...: T...)
```

Runs every handler with these values. Fire never yields. It works in this order:

1. Fire takes a list of the bound ids and of the coroutines that wait in [Wait](#wait).
2. It starts each handler on its own new coroutine, in bind order. A handler runs until it ends or yields. Then the next handler starts.
3. Fire returns after the last handler has started.
4. The waiting coroutines get the values. They resume after the code that called Fire yields or ends.

Changes made by handlers during a fire follow these rules:

- An id that is unbound is skipped.
- An id that is bound again runs with its new function.
- A new id runs from the next fire on.
- A Wait that starts during the fire waits for the next fire.

Values are passed as they are. Tables are not copied.

An error in a handler prints as an uncaught error. The other handlers still run, and the code that called Fire does not see the error. The handler stays bound.

```luau
local Process = import("Process")
local Signal = import("Signal")

local signal = Signal.new()
signal:BindHandler("slow", function()
	print("slow start")
	Process.Heartbeat:Wait()
	print("slow end")
end)
signal:BindHandler("fast", function()
	print("fast")
end)
signal:Fire()
print("fired")
```

This prints `slow start`, `fast`, `fired` and then `slow end`.

### Invoke

```luau
signal:Invoke(id: string, ...: T...): ...any
```

Calls the handler with this id and returns what it returns. No other handler runs and no waiting coroutine wakes. When the handler yields, the calling coroutine waits for it. An error in the handler is raised in the caller.

When no handler has this id, Invoke raises `no handler is bound to '<id>' on <Name>`.

```luau
local Signal = import("Signal")

local prices: Signal<string> = Signal.new()
prices:BindHandler("lookup", function(item)
	return if item == "sword" then 150 else 10
end)
print(prices:Invoke("lookup", "sword"))
```

This prints `150`.

### Wait

```luau
signal:Wait(): T...
```

Yields the calling coroutine until the next [Fire](#fire). Returns the values given to Fire.

When the signal is destroyed during the wait, Wait raises `<Name> was destroyed while it was being waited on`.

```luau
local Signal = import("Signal")

local opened: Signal<string> = Signal.new()
coroutine.wrap(function()
	local who = opened:Wait()
	print(`{who} opened the door`)
end)()
opened:Fire("Player1")
print("fired")
```

This prints `fired` and then `Player1 opened the door`.

> [!NOTE]
> A coroutine that waits on a signal does not keep the game running. When nothing else keeps the game open, the game ends and the wait never returns. [Process.Heartbeat](process.md#heartbeat) is different. It keeps the game running while it has a handler or a waiting coroutine. See [When the game ends](../manual/yielding.md#when-the-game-ends).

### Destroy

```luau
signal:Destroy()
```

Removes every handler. Each waiting coroutine gets the error `<Name> was destroyed while it was being waited on`. After this, BindHandler, Fire, Invoke and Wait raise `Signal '<Name>' has been destroyed`. UnBind and IsBound return `false`. See [BaseGameObject](basegameobject.md#destroy).

Destroying [Process.Heartbeat](process.md#heartbeat) stops it for good in that thread.
