# task

Starts coroutines and waits for time.

```luau
task.spawn(function()
	task.wait(1)
	print("one second later")
end)
```

## Description

`task` is a global, so every script has it without an `import`. It holds the five ways luv starts a coroutine or waits for time.

| Name | What it does |
| --- | --- |
| [task.wait](#wait) | Holds up this coroutine for a while. |
| [task.spawn](#spawn) | Starts a coroutine now. |
| [task.defer](#defer) | Starts a coroutine after the current code stops. |
| [task.delay](#delay) | Starts a coroutine after a while. |
| [task.create](#create) | Makes a [Task](#task-object), a coroutine you can start again and again. |
| [task.count](#count) | How many coroutines luv is running. |

Every one of these yields a coroutine and never a thread. The rest of the game keeps going while one waits. See [Yielding and coroutines](../manual/yielding.md).

`task.wait` and `task.delay` keep the game running until they are done. See [When the game ends](../manual/yielding.md#when-the-game-ends).

## Functions

### wait

```luau
task.wait(seconds: number?): number
```

Holds up the coroutine that called it for `seconds`, then returns how long it really waited. That is a little more than you asked for, because a coroutine starts again on the next turn of the engine.

Leave the seconds out, or pass `0`, to give every other coroutine a turn and come straight back.

```luau
local waited = task.wait(0.5)
print(`waited {waited} seconds`)

task.wait()
print("one turn later")
```

Only the coroutine that called it waits. Nothing else in the game stops.

```luau
task.spawn(function()
	task.wait(2)
	print("slow")
end)

task.wait(1)
print("quick")
```

That prints `quick` and then `slow`.

To wait for a set number of frames instead of a set time, use the frame signals of a [Window](window.md). To follow the engine tick, use [Process.Heartbeat](process.md#heartbeat).

### spawn

```luau
task.spawn(body: ((...any) -> ...any) | thread, ...: any): thread
```

Starts `body` as a coroutine right away and hands back the coroutine. Anything after `body` is passed to it.

The coroutine runs until it waits for something. Only then does `task.spawn` return, so the first part of the body happens before the next line of your script.

```luau
task.spawn(function(name: string)
	print(`hello {name}`)
	task.wait(1)
	print("goodbye")
end, "world")

print("after the spawn")
```

That prints `hello world`, then `after the spawn`, then `goodbye` a second later.

You can pass a coroutine you already made instead of a function. luv starts it with the values you pass.

```luau
local waiting = coroutine.create(function(count: number)
	print(count)
end)

task.spawn(waiting, 3)
```

A coroutine that luv is already running cannot be started again. That fails with `task.spawn was given a coroutine that luv is already running`.

### defer

```luau
task.defer(body: ((...any) -> ...any) | thread, ...: any): thread
```

The same as [task.spawn](#spawn), except the body does not begin until the code that called it stops or waits.

Use it when the body should see the end of what you are doing now.

```luau
local ready = false

task.defer(function()
	print(ready)
end)

ready = true
```

That prints `true`. With `task.spawn` it would print `false`, because the body would run before the last line.

### delay

```luau
task.delay(seconds: number, body: ((...any) -> ...any) | thread, ...: any): thread
```

Starts `body` as a coroutine after `seconds` and hands the coroutine back at once. Anything after `body` is passed to it.

```luau
task.delay(2, function(message: string)
	print(message)
end, "two seconds later")

print("right now")
```

The game keeps running until the body has had its turn, so a delay on its own is enough to hold a game open.

A delay of `0` runs on the next turn of the engine, the same as [task.defer](#defer).

### create

```luau
task.create(body: (...any) -> ...any, exclusive: boolean?): Task
```

Makes a [Task](#task-object) out of `body`. Think of it as a coroutine you can start again and again. Call the Task to start the body as a new coroutine, with whatever values you pass.

```luau
local greet = task.create(function(name: string)
	task.wait(0.5)
	print(`hello {name}`)
end)

greet("first")
greet("second")
```

Both of those run at the same time, each on its own coroutine.

Pass `true` for `exclusive` to let only one run happen at a time. While one is going, a call is turned away and returns `nil` instead of a coroutine.

```luau
local save = task.create(function()
	task.wait(1)
	print("saved")
end, true)

print(save() ~= nil)
print(save() ~= nil)
```

That prints `true` and then `false`, because the second call came while the first was still going.

An exclusive Task is the simple way to keep a slow job from piling up when a button can be pressed twice.

### count

```luau
task.count(): number
```

How many coroutines luv is running for this thread right now. Useful while you are looking for work that never ends.

```luau
print(`running {task.count()} coroutines`)
```

## Task object

What [task.create](#create) returns. Call it to start the body.

```luau
local worker = task.create(function(amount: number)
	task.wait(0.2)
	print(amount)
end)

worker(1)
worker(2)
worker:Wait()
print("both done")
```

### Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"Task"`. Read only. |
| `Exclusive` | `boolean` | `true` when only one run is allowed at a time. Read only. |
| `Running` | `number` | How many runs of this Task are going right now. Read only. |

### Calling it

```luau
worker(...: any): thread?
```

Starts the body as a new coroutine with the values you pass and returns that coroutine. An exclusive Task that is already running returns `nil` and starts nothing.

Like [task.spawn](#spawn), the body runs until it waits for something before the call comes back.

### Run

```luau
worker:Run(...: any): thread?
```

The same as calling it. Use whichever reads better.

### Wait

```luau
worker:Wait(): ()
```

Holds up the coroutine that called it until every run of this Task is done. Comes straight back when none are running.

```luau
local build = task.create(function(part: string)
	task.wait(0.1)
	print(part)
end)

for _, part in { "hull", "wing", "tail" } do
	build(part)
end

build:Wait()
print("all built")
```

A run that starts while you are waiting counts too, so `Wait` comes back once the Task is quiet.

## Errors

An error inside the body of a coroutine does not reach the code that started it, because that code has already moved on. luv prints it with the name of the thread. See [Errors in handlers](../manual/yielding.md#errors-in-handlers).

To catch it, use [pcall](https://luau.org/library#pcall) inside the body, or start the work with [promise.call](promise.md#call) instead and use [Catch](promise.md#catch).

| Message | Cause |
| --- | --- |
| `task.spawn needs a function or a coroutine, got a <type>` | The first value is something else. |
| `task.spawn was given a coroutine that luv is already running` | That coroutine is waiting on the engine. |
| `task.defer needs a function or a coroutine, got a <type>` | The same, for defer. |
| `task.delay needs a function or a coroutine, got a <type>` | The same, for delay. |
