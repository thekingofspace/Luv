# Parallel code

Parallel code runs on its own thread, next to the rest of your game. Use it for heavy work that would hold up everything else.

luv has three ways to do it:

| Way | What it runs | How long the thread lives |
| --- | --- | --- |
| [A parallel block](#writing-a-block) | The lines between `task.desynchronize()` and `task.synchronize()`. | Until its work is done. |
| [task.parallel](#task-parallel) | One function, with the values you pass it. | Until its work is done. |
| [BindParallel](#bindparallel) | A signal handler, every time the signal fires. | Until you unbind it. |

Each thread is a separate CPU thread with its own Luau state. Threads talk through [Messenger](../reference/messenger.md), [Thread](../reference/thread.md) and a safe [Registry](../reference/registry.md).

## Writing a block

Put the code between `task.desynchronize()` and `task.synchronize()`.

```luau
local Messenger = import("Messenger")

Messenger:Subscribe("Total", function(total: number)
	print("total", total)
end)

task.desynchronize()
local total = 0
for index = 1, 10000000 do
	total += index
end
Messenger:Fire("Total", total)
task.synchronize()

print("this prints first")
```

When the script reaches `task.desynchronize()`, luv starts a new thread and runs the block there. The script does not wait. It goes on after `task.synchronize()` right away. The block cannot return values. Send results back with [Messenger](../reference/messenger.md).

Each time the script reaches the block, luv starts another thread. A block in a loop that runs 4 times starts 4 threads. A block in a function starts a thread each time the function runs.

### The old names

`EnterParallel()` and `ExitParallel()` still work and do exactly the same thing. They are deprecated, so your editor marks them and suggests the new names. The new names match the rest of the [task](../reference/task.md) library.

A block can open with one pair and close with the other, but it reads better to keep to one.

## Rules

- `task.desynchronize()` and `task.synchronize()` must each be a statement of their own, with no arguments.
- Both must be in the same block of code. You cannot start a block outside an `if`, loop or `do` and end it inside.
- A block cannot hold another block, not even inside a function that is defined in the block.
- A block cannot use the `...` of the function around it. Store it in a local before `task.desynchronize()`.
- Blocks work in any script, modules included. A block can call a module function that has its own block.
- A local of your own named `task` hides the task library, and then `task.desynchronize()` is just a call to your table.

luv checks these rules when it loads the script. `luv build` stops with the error. See [Error messages](#error-messages).

## task.parallel

```luau
task.parallel(body: (...any) -> ...any, ...: any): Thread
```

Runs one function on a new thread and hands the values after it to the function. It returns a [Thread](../reference/thread.md) at once and does not wait.

```luau
local Messenger = import("Messenger")

local worker = task.parallel(function(first: number, last: number)
	local total = 0
	for index = first, last do
		total += index
	end
	Messenger:Fire("Total", total)
end, 1, 10000000)

print(worker.Name)
print("total", Messenger:Wait("Total"))
```

This is how you start a thread with arguments. A thread that runs your entities can be handed its level and its settings when it starts:

```luau
local Messenger = import("Messenger")
local Thread = import("Thread")

task.parallel(function(level: string, count: number)
	Thread.Set("Entities", { level = level })
	Messenger:Subscribe("Spawn", function(kind: string)
		print(`spawning a {kind} in {level}`)
	end)
end, "caves", 40)
```

The values you pass follow the same rules as [captured locals](#captured-locals).

## BindParallel

```luau
signal:BindParallel(id: string, handler: (...any) -> ...any): Thread
```

Binds a handler that runs on its own thread. luv starts one thread for the binding and keeps it. Every time the signal fires, the values of the fire travel to that thread and the handler runs there.

```luau
local Messenger = import("Messenger")
local Signal = import("Signal")

local moved = Signal.new()
local scale = 10

moved:BindParallel("mover", function(x: number, y: number)
	Messenger:Fire("Moved", (x + y) * scale)
end)

moved:Fire(1, 2)
print(Messenger:Wait("Moved"))
```

[UnBind](../reference/signal.md#unbind) ends the thread. So does destroying the signal. A bound thread with nothing to do does not keep the game running.

The values of a fire must be ones that can be copied, see [Captured locals](#captured-locals). A fire with a value that cannot be copied still reaches the normal handlers, and luv reports an error for the parallel one.

[Invoke](../reference/signal.md#invoke) does not work on a parallel handler, because it runs somewhere else and cannot hand a value back.

## Why the function must be written in the call

`task.parallel` and `BindParallel` only take a function written right there, inside the call. A function stored in a variable is refused:

```luau
local handler = function() end
task.parallel(handler)
```

This fails with `task.parallel needs the function written inside the call, like task.parallel(function(...) end), because luv can only move a function to another thread when it can see the code`.

A function cannot be copied to another Luau state once it exists. Luau keeps no way to read its code or the locals it holds, and two functions written on the same line look the same from the outside. Loops make it worse, since every function made in a loop comes from the same place but holds a different value.

A function written in the call is different. luv reads it when it compiles the script, the same way it reads a parallel block. It sees every line of the function and every local from outside that the function uses. So it can build the same function on the other thread with complete accuracy. The outside locals are copied, as described below.

## What the thread sees

The thread runs in a fresh Luau state. It has the standard libraries and every luv global, like `import`, `require`, `task`, `enum`, `udim` and `color`. It has its own copy of every library, its own [Process.Heartbeat](../reference/process.md#heartbeat) and its own Messenger object.

Globals that you set outside do not exist inside it. To give every thread the same globals, use a [boot script](scripts.md#boot-scripts). It runs on every new thread before the code of that thread.

`require` inside the thread works relative to the script, the same as outside. The thread has its own module cache, so modules run again inside it.

Errors inside the thread point at the right line of your script. They also reach [Exception.Raised](../reference/exception.md#raised) on the main thread.

## Captured locals

Parallel code can use locals from outside it. This includes function parameters, loop variables and `self` inside a method. luv copies each one when the thread starts. A change inside does not reach the outside, and a change outside does not reach the thread.

```luau
local Messenger = import("Messenger")

local settings = { speed = 5 }

Messenger:Subscribe("Speed", function(speed: number)
	print("thread saw", speed)
end)

task.desynchronize()
settings.speed = 99
Messenger:Fire("Speed", settings.speed)
task.synchronize()

print("script sees", settings.speed)
```

This prints:

```text
script sees	5
thread saw	99
```

Only locals declared before the parallel code are captured. A local declared after it is not.

These values can be copied into a thread:

| Value | Can be copied |
| --- | --- |
| `nil`, booleans, numbers and strings | Yes. |
| `vector` | Yes. |
| `buffer` | Yes, as a copy. |
| Tables | Yes, as a deep copy without metatables. |
| [UDim](../reference/udim.md) and [Color](../reference/color.md) | Yes. |
| Enum items | Yes. The thread gets the same item. |
| Anything from `import`, like `Messenger` or `Thread` | Yes. The thread gets its own copy. |
| A module from `require` | Yes. The thread loads the same module itself, once. |
| Functions | No. |
| Coroutines | No. |
| Engine objects, like a [Signal](../reference/signal.md) | No. |
| Tables that hold a value that cannot be copied | No. |
| Tables that contain themselves | No. |
| Tables nested deeper than 128 levels | No. |

So you can import and require at the top of your script and use them straight away inside parallel code:

```luau
local Messenger = import("Messenger")
local Maths = require("./Maths")

task.parallel(function(value: number)
	Messenger:Fire("Result", Maths.square(value))
end, 6)
```

The thread gets its own Messenger and loads `Maths` for itself. A module that holds functions is fine this way, because luv sends which module it is, not what is inside it.

These are the same rules as for [Messenger](../reference/messenger.md#values-you-can-send).

## Sending results back

Fire a message from the thread and subscribe to it outside. Here four blocks work at the same time:

```luau
local Messenger = import("Messenger")

Messenger:Subscribe("ChunkDone", function(index: number, solid: number)
	print(`chunk {index} has {solid} solid tiles`)
end)

for index = 1, 4 do
	task.desynchronize()
	local solid = 0
	for tile = 1, 4096 do
		solid += if math.noise(tile / 64, index) > 0 then 1 else 0
	end
	Messenger:Fire("ChunkDone", index, solid)
	task.synchronize()
end
```

To talk to one thread and not all of them, find it with [Thread](../reference/thread.md) and use [Send](../reference/thread.md#send).

## Workers

Each block run starts a new thread and a new Luau state. For many small jobs, start a few workers once, or bind one with [BindParallel](#bindparallel).

```luau
local Messenger = import("Messenger")

task.desynchronize()
Messenger:Subscribe("Square", function(job: number, value: number)
	Messenger:Fire("Squared", job, value * value)
end)
task.synchronize()

Messenger:Subscribe("Squared", function(job: number, result: number)
	print(job, result)
end)

for job = 1, 3 do
	Messenger:Fire("Square", job, job + 1)
end
```

A worker gets every message sent after its thread starts. Its own code runs before its first message, so subscribe at the top.

## Thread lifetime

A block or a `task.parallel` thread stays alive while any of these is true:

- Its code is still running.
- It has running coroutines or heartbeat handlers.
- It has Messenger subscriptions or waits.
- It has BindToClose callbacks.

After that the thread ends. A [BindParallel](#bindparallel) thread lives until you unbind it. When the whole game ends, every thread stops.

Work inside a thread keeps the game running, the same as work in the main script. Subscriptions alone do not, and neither does an idle BindParallel thread. See [When the game ends](yielding.md#when-the-game-ends).

When the game closes, every thread runs its own BindToClose callbacks. A callback in the main script can wait for their messages:

```luau
local Process = import("Process")
local Messenger = import("Messenger")

Process.BindToClose(function()
	for _ = 1, 4 do
		print("worker said", Messenger:Wait("Goodbye"))
	end
end)

for worker = 1, 4 do
	task.desynchronize()
	Process.BindToClose(function()
		Messenger:Fire("Goodbye", worker)
	end)
	task.synchronize()
end
```

## Error messages

luv reports these when it loads the script. The old names give the same messages with their own names.

| Message | Cause |
| --- | --- |
| `task.desynchronize() has no matching task.synchronize() in the same block` | The block never ends, or it ends inside another block of code. |
| `task.synchronize() has no matching task.desynchronize() in the same block` | There is no `task.desynchronize()` before it in the same block of code. |
| `task.desynchronize() cannot be nested, call task.synchronize() first` | Two openings without a closing between them. |
| `task.desynchronize() cannot be used inside another parallel block` | A block inside a function that is defined in a block. |
| `task.desynchronize() does not take any arguments` | Something was passed to it. `task.synchronize()` gives the same error with its own name. |
| `task.desynchronize() must be called on its own as a statement` | It was used as a value, like `local x = task.desynchronize()`. |
| `` `...` cannot be used directly inside a parallel block, store it in a local before task.desynchronize() `` | The block uses the `...` of the function around it. |

luv reports these when the script reaches the parallel code:

| Message | Cause |
| --- | --- |
| ``cannot pass `name` into parallel block #1: reason`` | A captured local of a block cannot be copied. |
| ``cannot pass `name` into the parallel function at src/main.luau:4: reason`` | A captured local of a parallel function cannot be copied. |
| `cannot pass the arguments of task.parallel at src/main.luau:4: reason` | A value passed to `task.parallel` cannot be copied. |
| `task.parallel needs the function written inside the call, ...` | The function was stored in a variable. See [Why the function must be written in the call](#why-the-function-must-be-written-in-the-call). |
| `BindParallel needs the function written inside the call, ...` | The same, for BindParallel. |

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

task.desynchronize()
changed:Fire()
task.synchronize()
```

This stops the script with ``cannot pass `changed` into parallel block #1: Signal objects cannot be sent between threads``.

Errors inside a running thread start with the name of the thread, like `[parallel block #1 of src/main.luau]` or `[task.parallel at src/main.luau:4]`. The number of a block counts the parallel code in that file from the top, starting at 1.
