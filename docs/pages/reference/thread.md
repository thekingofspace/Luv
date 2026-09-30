# Thread

Finds the threads of the game, describes them and talks to one of them directly.

```luau
local Thread = import("Thread")

Thread.Set("Entities")
print(Thread.Running().Name)
```

## Description

Every piece of [parallel code](../manual/parallel.md) runs on its own thread, and the main script runs on the main thread. Thread lets each one say what it is for, find the others, and send a message to just one of them.

A thread describes itself with [Set](#set). The `State` is a short word for its job, like `"Entities"` or `"Pathfinding"`, and `Data` is anything else worth knowing. Others find it with [Get](#get) or wait for it with [WaitFor](#waitfor).

```luau
local Messenger = import("Messenger")
local Thread = import("Thread")

task.parallel(function()
	Thread.Set("Entities", { capacity = 500 })
	Messenger:Subscribe("Spawn", function(kind: string)
		print("spawning", kind)
	end)
	Thread.Running():MarkReady()
end)

local entities = Thread.WaitFor("Entities")
entities:WaitReady()
entities:Send("Spawn", "goblin")
```

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [Running](#running)() | [Thread](#thread-object) | no |
| [Get](#get)(state) | `{ Thread }` | no |
| [Set](#set)(state, data) | none | no |
| [WaitFor](#waitfor)(state, timeout) | [Thread](#thread-object)`?` | yes |

### Running

```luau
Thread.Running(): Thread
```

The thread this code is running on.

### Get

```luau
Thread.Get(state: string?): { Thread }
```

Every thread of the game that is still running, the main thread first. Pass a `state` to get only the threads that set it.

```luau
for _, thread in Thread.Get() do
	print(thread.Name, thread.State)
end
```

### Set

```luau
Thread.Set(state: string?, data: any?)
```

Describes the thread this code runs on. Both replace what was set before.

`data` must be a value that can travel between threads. See [Captured locals](../manual/parallel.md#captured-locals).

### WaitFor

```luau
Thread.WaitFor(state: string, timeout: number?): Thread?
```

Holds up the coroutine until a thread with this `State` exists, then returns it. With a `timeout` in seconds it gives up and returns `nil`.

```luau
local pathing = Thread.WaitFor("Pathfinding", 5)
if pathing == nil then
	print("pathfinding never started")
end
```

## Thread object

A handle to one thread. You can keep it and pass it around. When the thread ends, the handle stays but reads as ended.

### Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"Thread"`. Read only. |
| `Id` | `number` | A number that names this thread for as long as the game runs. Read only. |
| `Name` | `string?` | `main`, the script of an [@startasync](../manual/scripts.md#boot-scripts) thread, or where the parallel code is, like `task.parallel at src/main.luau:4`. `nil` once it ended. Read only. |
| `State` | `string?` | What [Set](#set) gave it. `nil` once it ended. Read only. |
| `Data` | `any` | A copy of what [Set](#set) gave it. Read only. |
| `IsMain` | `boolean` | `true` for the main thread. Read only. |
| `IsReady` | `boolean` | `true` once it called [MarkReady](#markready). Read only. |
| `IsAlive` | `boolean` | `false` once it ended. Read only. |
| `IsCurrent` | `boolean` | `true` when this is the thread your code runs on. Read only. |

Two handles to the same thread are equal with `==`.

### Send

```luau
thread:Send(topic: string, ...: any): boolean
```

Sends a message to this thread only. It arrives in its [Messenger](messenger.md) subscriptions and waits for `topic`, the same as a fire, but no other thread gets it. Returns `false` when the thread has ended.

The values follow the rules of [Captured locals](../manual/parallel.md#captured-locals).

### MarkReady

```luau
thread:MarkReady(): boolean
```

Tells the game this thread has set itself up. It runs every [@bootready](../manual/scripts.md#boot-scripts) script on this thread and wakes everything waiting in [WaitReady](#waitready). Returns `true` the first time and `false` after that.

Only a thread can mark itself, so call it on `Thread.Running()`. Calling it on another thread errors with `only a thread can mark itself ready, call it on Thread.Running()`.

### WaitReady

```luau
thread:WaitReady(timeout: number?): boolean
```

Holds up the coroutine until this thread calls [MarkReady](#markready). Returns `true` once it is ready, and `false` when the timeout ran out or the thread ended first.

## Errors of other threads

An error in any thread reaches [Exception.Raised](exception.md#raised) on the main thread too, with `Thread` set to the name of the thread.
