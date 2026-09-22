# Yielding and coroutines

luv never blocks the main thread. When a script has to wait, only its coroutine pauses. Everything else keeps running. This page shows what yields, how handlers start and when the game ends.

## How scripts run

Your main script runs as a coroutine, and so does every handler. So any script can call a yielding function, even at its top level.

Luau code in a thread runs until it ends or yields. Nothing stops it in the middle. A long loop that never yields holds up the handlers, messages and heartbeat of that thread. Move heavy work into a [parallel block](parallel.md).

## What yields

These functions yield the calling coroutine:

| Function | Waits until |
| --- | --- |
| [Signal:Wait](../reference/signal.md#wait) | The next fire. |
| [Signal:Invoke](../reference/signal.md#invoke) | The handler returns. It only yields when the handler yields. |
| [Messenger:Wait](../reference/messenger.md#wait) | The next message on the topic. |
| [Process.Heartbeat:Wait](../reference/process.md#heartbeat) | The next heartbeat tick. |
| [Process.exit](../reference/process.md#exit) | Never. It does not return. |
| `require` | The module finishes. It only yields when the module yields. |

The reference pages say when other functions yield. Library pages have a Yields column for this.

These never yield:

- `Fire` on a [Signal](../reference/signal.md) or on [Messenger](../reference/messenger.md).
- `BindHandler`, `UnBind`, `IsBound`, `Subscribe` and `Unsubscribe`.
- `import`, `print` and the [Bulk](../reference/bulk.md) functions.
- Everything in `udim`, `color` and `enum`.

You can call a yielding function anywhere a coroutine can yield. That includes the top level of a script, handlers, modules, code inside `pcall`, and your own coroutines.

## Handlers start inline

When luv runs a handler, it starts it right away on a new coroutine. The handler runs until it ends or yields. Only then does the code that fired go on.

[Signal:Fire](../reference/signal.md#fire) starts its handlers this way before it returns. Messenger handlers, heartbeat handlers and [BindToClose](../reference/process.md#bindtoclose) callbacks start the same way when their event comes in.

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

## Running code on the side

Wrap code in `coroutine.wrap` to run it next to the rest of your script. The call returns as soon as the coroutine starts an engine wait.

```luau
local Process = import("Process")
local Signal = import("Signal")

local doorOpened: Signal<string> = Signal.new()

coroutine.wrap(function()
	local who = doorOpened:Wait()
	print(`{who} opened the door`)
end)()

print("waiting for the door")
Process.Heartbeat:Wait()
doorOpened:Fire("Player1")
```

## coroutine.resume and coroutine.wrap

luv changes these two functions. With `coroutine.yield` they work as usual. The change is for engine waits:

- When the coroutine starts an engine wait, `coroutine.resume` returns only `true`. luv runs the rest of the coroutine when the wait ends.
- While luv waits, `coroutine.status` returns `"suspended"`. Resuming it then returns `false` and `cannot resume a coroutine that is waiting on the engine`.
- When the coroutine calls `coroutine.yield` after the engine wait, it is yours again. You can resume it as usual.
- `coroutine.wrap` works the same way. It raises errors again with their original value.

```luau
local Process = import("Process")

local co = coroutine.create(function()
	Process.Heartbeat:Wait()
	print("one tick later")
end)

print(coroutine.resume(co))
print(coroutine.status(co))
print(coroutine.resume(co))
```

This prints:

```text
true
suspended
false	cannot resume a coroutine that is waiting on the engine
one tick later
```

## Errors in handlers

An error that no code catches prints as `error: <message>`. Only the coroutine with the error stops. The game keeps running.

- An error in a Signal handler does not stop Fire or the other handlers. The handler stays bound. A broken heartbeat handler errors on every tick.
- An error inside `coroutine.resume` or `coroutine.wrap` goes to the caller when it happens before the first engine wait. After that wait there is no caller, so it prints as an uncaught error.
- An error in a parallel block starts with the block name, like `[parallel block #1 of src/main.luau]`.

When the game ends, luv prints how many errors happened and exits with code 1. See [Errors in the game](../start/command-line.md#errors-in-the-game).

```luau
local Signal = import("Signal")

local hit: Signal<number> = Signal.new()
hit:BindHandler("check", function(amount)
	if amount < 0 then
		error("negative damage")
	end
end)
hit:BindHandler("apply", function(amount)
	print("applied", amount)
end)

hit:Fire(-5)
print("Fire returned")
```

The error prints first. Then the `apply` handler and the rest of the script run as usual.

## Waiting for time

luv has no `wait` function. Wait on [Process.Heartbeat](../reference/process.md#heartbeat) instead. It fires 60 times a second and passes the seconds since the last tick.

```luau
local Process = import("Process")

local function waitSeconds(seconds: number)
	local elapsed = 0
	while elapsed < seconds do
		elapsed += Process.Heartbeat:Wait()
	end
end

waitSeconds(2)
print("two seconds later")
```

The heartbeat only fires while something listens to it. When a thread is busy, luv skips the ticks it missed, so the next tick passes a larger number. Each parallel block has its own heartbeat.

The heartbeat runs on its own timer. It does not follow the frames of a window. For work on every frame, use the frame signals of a [Window](../reference/window.md). See [Windows and frames](windows.md).

## When the game ends

The game keeps running while any of these is true, in any thread:

- A coroutine is running, or it waits on an engine call other than `Signal:Wait` or `Messenger:Wait`. For example [Process.spawn](../reference/process.md) waits for a program to end.
- `Process.Heartbeat` has a handler or a waiting coroutine.
- A window is open.
- A Messenger message has not reached every thread yet.

```luau
local Process = import("Process")

local ticks = 0
Process.Heartbeat:BindHandler("count", function()
	ticks += 1
	if ticks == 60 then
		Process.Heartbeat:UnBind("count")
	end
end)
```

This game runs for about one second. It ends when the handler unbinds, because nothing else is left.

These do not keep the game running on their own:

- A coroutine that waits in `Signal:Wait` or `Messenger:Wait`.
- Messenger subscriptions.
- BindToClose callbacks.
- A coroutine paused with `coroutine.yield`.

```luau
local Messenger = import("Messenger")

coroutine.wrap(function()
	Messenger:Wait("Never")
	print("this never prints")
end)()
print("the game ends after this line")
```

When nothing is left, the game closes:

1. Every thread runs its [BindToClose](../reference/process.md#bindtoclose) callbacks.
2. Messages still arrive, so a callback can use `Messenger:Wait`.
3. The game ends when every callback has finished, or after 30 seconds.
4. Coroutines that still wait never resume.

## BindToClose and Process.exit

BindToClose callbacks run in the order you bind them. Each one starts on its own coroutine and runs until it ends or yields. Then the next one starts.

```luau
local Process = import("Process")

Process.BindToClose(function()
	Process.Heartbeat:Wait()
	print("first")
end)
Process.BindToClose(function()
	print("second")
end)
print("main")
```

This prints `main`, `second` and then `first`.

[Process.exit](../reference/process.md#exit) starts the close right away, even when work is left. The coroutine that calls it never resumes. Other code keeps running while the BindToClose callbacks run. Then luv exits with your code.

```luau
local Process = import("Process")

Process.BindToClose(function()
	print("saving")
end)

print("quitting")
Process.exit(0)
print("never printed")
```

Pressing Ctrl+C starts the same close with exit code 130. A second Ctrl+C stops the game at once.

> [!WARNING]
> A callback that never finishes makes the game wait the full 30 seconds. Calling `Process.exit` inside a callback does this, because `exit` never returns.
