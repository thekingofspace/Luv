# Parallel code

A parallel block runs part of a script on its own thread. Use it for heavy work that would hold up the rest of your game.

## Writing a block

Put the code between `EnterParallel()` and `ExitParallel()`.

```luau
local Messenger = import("Messenger")

Messenger:Subscribe("Total", function(total: number)
	print("total", total)
end)

EnterParallel()
local total = 0
for index = 1, 10000000 do
	total += index
end
Messenger:Fire("Total", total)
ExitParallel()

print("this prints first")
```

When the script reaches `EnterParallel()`, luv starts a new thread and runs the block there. The script does not wait. It goes on after `ExitParallel()` right away. The block cannot return values. Send results back with [Messenger](../reference/messenger.md).

Each time the script reaches the block, luv starts another thread. A block in a loop that runs 4 times starts 4 threads. A block in a function starts a thread each time the function runs.

## Rules

- `EnterParallel()` and `ExitParallel()` must each be a statement of their own, with no arguments.
- Both must be in the same block of code. You cannot start a block outside an `if`, loop or `do` and end it inside.
- A block cannot hold another block, not even inside a function that is defined in the block.
- A block cannot use the `...` of the function around it. Store it in a local before `EnterParallel()`.
- Blocks work in any script, modules included. A block can call a module function that has its own block.

luv checks these rules when it loads the script. `luv build` stops with the error. See [Error messages](#error-messages).

## What the block sees

The block runs in a fresh Luau state on its own thread. It has the standard libraries and every luv global, like `import`, `require`, `enum`, `udim` and `color`. It has its own copy of every library, its own [Process.Heartbeat](../reference/process.md#heartbeat) and its own Messenger object.

Globals that you set outside the block do not exist inside it.

`require` inside the block works relative to the script, the same as outside. The block has its own module cache, so modules run again inside it.

Errors inside the block point at the right line of your script.

## Captured locals

The block can use locals from outside it. This includes function parameters and loop variables. luv copies each one when the block starts. A change inside the block does not reach the outside, and a change outside does not reach the block.

```luau
local Messenger = import("Messenger")

local settings = { speed = 5 }

Messenger:Subscribe("Speed", function(speed: number)
	print("block saw", speed)
end)

EnterParallel()
settings.speed = 99
Messenger:Fire("Speed", settings.speed)
ExitParallel()

print("script sees", settings.speed)
```

This prints:

```text
script sees	5
block saw	99
```

Only locals declared before the block are captured. A local declared after it is not.

These values can be copied into a block:

| Value | Can be copied |
| --- | --- |
| `nil`, booleans, numbers and strings | Yes. |
| `vector` | Yes. |
| `buffer` | Yes, as a copy. |
| Tables | Yes, as a deep copy without metatables. |
| [UDim](../reference/udim.md) and [Color](../reference/color.md) | Yes. |
| Enum items | Yes. The block gets the same item. |
| Anything from `import`, like `Messenger` or `Process` | Yes. The block gets its own copy. |
| Functions | No. |
| Coroutines | No. |
| Engine objects, like a [Signal](../reference/signal.md) | No. |
| Tables that hold a value that cannot be copied | No. |
| Tables that contain themselves | No. |
| Tables nested deeper than 128 levels | No. |

These are the same rules as for [Messenger](../reference/messenger.md#values-you-can-send). Make helper functions inside the block, or `require` them there.

## Sending results back

Fire a message from the block and subscribe to it outside. Here four blocks work at the same time:

```luau
local Messenger = import("Messenger")

Messenger:Subscribe("ChunkDone", function(index: number, solid: number)
	print(`chunk {index} has {solid} solid tiles`)
end)

for index = 1, 4 do
	EnterParallel()
	local solid = 0
	for tile = 1, 4096 do
		solid += if math.noise(tile / 64, index) > 0 then 1 else 0
	end
	Messenger:Fire("ChunkDone", index, solid)
	ExitParallel()
end
```

A block can also wait for messages:

```luau
local Messenger = import("Messenger")

EnterParallel()
local first = Messenger:Wait("Numbers")
local second = Messenger:Wait("Numbers")
Messenger:Fire("Sum", first + second)
ExitParallel()

Messenger:Fire("Numbers", 20)
Messenger:Fire("Numbers", 22)
print("sum", Messenger:Wait("Sum"))
```

## Workers

Each block run starts a new thread and a new Luau state. For many small jobs, start a few workers once. Each worker subscribes to a job topic.

```luau
local Messenger = import("Messenger")

EnterParallel()
Messenger:Subscribe("Square", function(job: number, value: number)
	Messenger:Fire("Squared", job, value * value)
end)
ExitParallel()

Messenger:Subscribe("Squared", function(job: number, result: number)
	print(job, result)
end)

for job = 1, 3 do
	Messenger:Fire("Square", job, job + 1)
end
```

This prints:

```text
1	4
2	9
3	16
```

A worker gets every message sent after its block starts. Its own code runs before its first message, so subscribe at the top of the block.

## Thread lifetime

The thread of a block stays alive while any of these is true:

- Its code is still running.
- It has running coroutines or heartbeat handlers.
- It has Messenger subscriptions or waits.
- It has BindToClose callbacks.

After that the thread ends. When the whole game ends, every block stops.

Work inside a block keeps the game running, the same as work in the main script. Subscriptions alone do not. See [When the game ends](yielding.md#when-the-game-ends).

When the game closes, every block runs its own BindToClose callbacks. A callback in the main script can wait for their messages:

```luau
local Process = import("Process")
local Messenger = import("Messenger")

Process.BindToClose(function()
	for _ = 1, 4 do
		print("worker said", Messenger:Wait("Goodbye"))
	end
end)

for worker = 1, 4 do
	EnterParallel()
	Process.BindToClose(function()
		Messenger:Fire("Goodbye", worker)
	end)
	ExitParallel()
end
```

## Error messages

luv reports these when it loads the script:

| Message | Cause |
| --- | --- |
| `EnterParallel() has no matching ExitParallel() in the same block` | The block never ends, or it ends inside another block of code. |
| `ExitParallel() has no matching EnterParallel() in the same block` | There is no `EnterParallel()` before it in the same block of code. |
| `EnterParallel() cannot be nested, call ExitParallel() first` | Two `EnterParallel()` calls without an `ExitParallel()` between them. |
| `EnterParallel() cannot be used inside another parallel block` | A block inside a function that is defined in a block. |
| `EnterParallel() does not take any arguments` | Something was passed to it. `ExitParallel()` gives the same error with its own name. |
| `EnterParallel() must be called on its own as a statement` | It was used as a value, like `local x = EnterParallel()`. `ExitParallel()` gives the same error with its own name. |
| `` `...` cannot be used directly inside a parallel block, store it in a local before EnterParallel() `` | The block uses the `...` of the function around it. |

luv reports this one when the script reaches the block:

| Message | Cause |
| --- | --- |
| ``cannot pass `name` into parallel block #1: reason`` | A captured local cannot be copied. The reason says why. |

The reason can be:

| Reason | Cause |
| --- | --- |
| `function values cannot be sent between threads` | The value is a function. |
| `thread values cannot be sent between threads` | The value is a coroutine. |
| `Signal objects cannot be sent between threads` | The value is an engine object. Its `typeof` name comes first. |
| `tables that contain themselves cannot be sent between threads` | A table holds itself. |
| `tables nested deeper than 128 levels cannot be sent between threads` | Tables are nested too deep. |

```luau
local Signal = import("Signal")

local changed = Signal.new()

EnterParallel()
changed:Fire()
ExitParallel()
```

This stops the script with ``cannot pass `changed` into parallel block #1: Signal objects cannot be sent between threads``.

Errors inside a running block start with the block name, like `[parallel block #1 of src/main.luau]`. The number counts the blocks in that file from the top, starting at 1.
