# promise

Holds work that is not done yet.

```luau
local handle = promise.new(function(resolve, reject)
	task.wait(1)
	resolve("done")
end)

print(handle:Await())
```

## Description

`promise` is a global, so every script has it without an `import`.

A [Promise](#promise-object) is one value that is not there yet. It starts as pending and settles once, either resolved with some values or rejected with a reason. Once it has settled it never changes again.

You do not have to hold up your code to use one. Hook onto it with [AndThen](#andthen) and carry on, or call [Await](#await) to hold up the coroutine until it settles.

| Name | What it does |
| --- | --- |
| [promise.new](#new) | Runs a body that calls `resolve` or `reject`. |
| [promise.call](#call) | Runs a function and uses what it returns. |
| [promise.resolve](#resolve) | A promise that is already resolved. |
| [promise.reject](#reject) | A promise that is already rejected. |
| [promise.all](#all) | Waits for a whole list. |
| [promise.race](#race) | Waits for the first of a list. |
| [promise.delay](#delay) | Resolves after a while. |
| [promise.is](#is) | Whether a value is a promise. |

A promise runs on a coroutine, so the body can wait for time, for a file, for the network, for anything the rest of the game can wait for. See [Yielding and coroutines](../manual/yielding.md).

## Status

A promise is always in one of four states.

| Status | What it means |
| --- | --- |
| `pending` | It has not settled. |
| `resolved` | It worked, and it holds the values that were resolved. |
| `rejected` | It failed, and it holds the reason. |
| `cancelled` | You called [Cancel](#cancel) before it settled. |

Read it with the `Status` property. It only ever moves away from `pending` once.

## Functions

### new

```luau
promise.new(body: (resolve, reject, ...any) -> ...any, ...: any): Promise
```

Starts `body` on a coroutine and returns a promise for what it produces.

The body is handed three things.

| Name | What it is |
| --- | --- |
| `resolve` | A function. Call it with any values to resolve the promise with them. |
| `reject` | A function. Call it with a reason to reject the promise. |
| `...` | Whatever you passed to `promise.new` after the body. |

```luau
local sum = promise.new(function(resolve, reject, first: number, second: number)
	if second == 0 then
		reject("cannot divide by zero")
		return
	end
	task.wait(0.1)
	resolve(first / second, "extra")
end, 10, 2)

print(sum:Await())
```

The body begins at once, before `promise.new` returns, and runs until it waits for something. So a body that resolves without waiting gives you a promise that has already resolved.

The first call to `resolve` or `reject` wins. Later calls do nothing, so you do not have to guard them.

A promise does not settle when the body returns. It settles when `resolve` or `reject` is called. A body that returns without calling either leaves the promise pending for good, which is what you want when something else will settle it later.

An error inside the body rejects the promise, with the text of the error as the reason. You never lose an error to a coroutine this way.

```luau
local broken = promise.new(function()
	error("no good")
end)

broken:Catch(function(reason)
	print(reason)
end)
```

### call

```luau
promise.call(body: (...any) -> ...any, ...: any): Promise
```

Runs `body` on a coroutine and resolves with whatever it returns. An error rejects instead. Anything after `body` is passed to it.

Use this when the work already reads as a plain function and you do not need `resolve` and `reject`.

```luau
local FS = import("FS")

local reading = promise.call(function(path: string)
	return FS.readFile(path)
end, "save.json")

reading
	:AndThen(function(text: string)
		print(#text)
	end)
	:Catch(function(reason)
		print(`could not read it: {reason}`)
	end)
```

When the body returns a promise, this one follows it. So you can build one out of another without nesting.

### resolve

```luau
promise.resolve(...: any): Promise
```

A promise that has already resolved with the values you pass. Useful when a function has to hand back a promise but already has the answer.

```luau
local function loadLevel(name: string)
	local kept = cache[name]
	if kept then
		return promise.resolve(kept)
	end
	return promise.call(readLevel, name)
end
```

### reject

```luau
promise.reject(reason: any): Promise
```

A promise that has already rejected with `reason`.

### all

```luau
promise.all(list: { any }): Promise
```

Waits for every promise in the list and resolves with a table of their values, in the order of the list. Each place holds the first value that promise resolved with.

The first rejection rejects the whole thing, with that reason. The rest keep running, and nothing is read from them.

```luau
local Net = import("Net")

local function fetch(url: string)
	return promise.call(function()
		return Net.Request({ url = url }).body
	end)
end

local bodies = promise.all({
	fetch("https://example.com/one"),
	fetch("https://example.com/two"),
}):Await()

print(#bodies[1], #bodies[2])
```

Anything in the list that is not a promise is kept as it is, so you can mix values in. An empty list resolves with an empty table.

### race

```luau
promise.race(list: { any }): Promise
```

Settles with whichever promise in the list settles first, and takes its outcome. The rest keep running and are ignored.

```luau
local answer = promise.race({
	askTheServer(),
	promise.delay(5, "no reply"),
}):Await()
```

That is the way to put a time limit on something. The list cannot be empty.

### delay

```luau
promise.delay(seconds: number, ...: any): Promise
```

A promise that resolves after `seconds` with the values you pass. The game keeps running until it does.

```luau
promise.delay(3, "late"):AndThen(print)
```

### is

```luau
promise.is(value: any): boolean
```

Whether `value` is a promise.

## Promise object

What every function on this page returns.

### Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"Promise"`. Read only. |
| `Status` | `string` | `"pending"`, `"resolved"`, `"rejected"` or `"cancelled"`. See [Status](#status). Read only. |

### Methods

Each of [AndThen](#andthen), [Catch](#catch) and [Finally](#finally) returns a new promise, so they chain. None of them hold up the code that calls them. The handler runs later, on its own coroutine, so it can wait for things.

Hooking onto a promise that has already settled is fine. The handler runs at once instead of later.

### AndThen

```luau
handle:AndThen(handler: (...any) -> ...any): Promise
```

Runs `handler` with the resolved values once this promise resolves, and returns a promise for what the handler returns.

```luau
promise.resolve(2)
	:AndThen(function(number: number)
		return number * 3
	end)
	:AndThen(function(number: number)
		print(number)
	end)
```

That prints `6`.

When the handler returns a promise, the new one follows it instead of resolving with the promise itself. That is how you put slow steps one after another without nesting them.

```luau
promise.resolve("save.json")
	:AndThen(function(path: string)
		return promise.call(readFile, path)
	end)
	:AndThen(function(text: string)
		print(#text)
	end)
```

A rejection skips the handler and passes straight down the chain, so you only need one [Catch](#catch) at the end. An error inside the handler rejects the promise it returned.

### Catch

```luau
handle:Catch(handler: (reason: any) -> ...any): Promise
```

Runs `handler` with the reason once this promise rejects, and returns a promise for what the handler returns.

This is how you recover. The promise it returns is resolved with whatever the handler gave back, so the chain carries on as if nothing went wrong.

```luau
local text = promise.call(readFile, "save.json")
	:Catch(function(reason)
		print(`using the default: {reason}`)
		return "{}"
	end)
	:Await()
```

When the promise resolves instead, the handler is skipped and the value passes through. An error inside the handler rejects the promise it returned.

### Finally

```luau
handle:Finally(handler: (status: string) -> ...any): Promise
```

Runs `handler` however this promise settles, and passes the outcome through untouched. The handler is given the status as a string, so it can tell what happened.

Use it to put something back, close a file or hide a loading screen.

```luau
showSpinner()

promise.call(loadEverything)
	:Finally(function(status: string)
		hideSpinner()
		print(`finished as {status}`)
	end)
	:Catch(warn)
```

Whatever the handler returns is thrown away, because the outcome of the promise is what carries on. An error inside the handler is the one thing that changes it, and rejects the promise it returned.

### Await

```luau
handle:Await(): ...any
```

Holds up the coroutine that called it until the promise settles, then returns the resolved values. Comes straight back when it has already settled.

A rejected promise raises an error with the reason in it. A cancelled one raises `the promise was cancelled`. Wrap it in [pcall](https://luau.org/library#pcall) when you would rather handle that here, or use [AwaitStatus](#awaitstatus).

```luau
local ok, first = pcall(function()
	return promise.call(loadSave):Await()
end)

if not ok then
	print(`could not load: {first}`)
end
```

Only your coroutine waits. The rest of the game keeps going.

### AwaitStatus

```luau
handle:AwaitStatus(): (string, ...any)
```

The same wait as [Await](#await), but nothing is ever raised. Returns the status first and then the values, which is the reason when it was rejected.

```luau
local status, value = promise.call(loadSave):AwaitStatus()

if status == "resolved" then
	print(value)
else
	print(`{status}: {value}`)
end
```

Use this when a rejection is an ordinary outcome and not a fault.

### Cancel

```luau
handle:Cancel(): boolean
```

Settles the promise as cancelled. Returns `true` when it was pending, and `false` when it had already settled and nothing was done.

An [AndThen](#andthen) or [Catch](#catch) on a cancelled promise never runs, and everything further down the chain is cancelled too. A [Finally](#finally) still runs, with `"cancelled"` as the status.

```luau
local loading = promise.call(loadLevel, "forest")

onPlayerLeft(function()
	loading:Cancel()
end)
```

Cancelling does not stop the body. luv cannot reach inside a coroutine and end it, so a body that is waiting keeps waiting and then finishes as normal. What changes is that nobody is listening, and its `resolve` and `reject` do nothing.

So use `Cancel` to let go of a result you no longer want. When the work itself has to stop, have the body check something of your own and return early.

## Waiting for many things

`promise.all` is the short way to start several slow things and wait for them together.

```luau
local Asset = import("Asset")

local loaded = promise.all({
	promise.call(Asset.Load, "hero.png"),
	promise.call(Asset.Load, "level.png"),
	promise.call(Asset.Load, "music.ogg"),
}):Await()

print(`loaded {#loaded} assets`)
```

Each one is its own coroutine, so they all run at the same time. Waiting on them one by one would take as long as all of them added up.

## Errors

| Message | Cause |
| --- | --- |
| `the promise was cancelled` | [Await](#await) on a cancelled promise. |
| `the promise was rejected with a <type>` | [Await](#await) on a promise rejected with something other than a string. Use [Catch](#catch) or [AwaitStatus](#awaitstatus) to read the reason itself. |
| `promise.race needs at least one promise` | The list was empty. |
