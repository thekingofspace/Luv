# switch

Picks a function by name and runs it on a coroutine.

```luau
local handle = switch.new({
	jump = function(height: number)
		print(`jumping {height}`)
	end,
	sit = function()
		print("sitting")
	end,
})

handle("jump", 12)
```

## Description

`switch` is a global, so every script has it without an `import`.

A switch is a table of names to functions, built once and called many times. Calling it looks up the name, runs that function on a new coroutine, and hands you back what the function returned.

Luau has no switch statement, and a chain of `if` tests gets long and slow once there are more than a few names. A switch is one lookup however many names there are, and the lookup happens in Rust, not in Luau.

| Name | What it does |
| --- | --- |
| [switch.new](#new) | Builds a switch out of a table of cases. |

## Functions

### new

```luau
switch.new(cases: { [string]: (...any) -> ...any }, fallback: ((key: string, ...any) -> ...any)?): (key: string, ...any) -> ...any
```

Builds a switch and returns it as a function.

`cases` is a table of names to functions. luv reads it once, here, and keeps the names in Rust. Changing the table afterwards changes nothing, so a switch is always the set of cases it was built with.

`fallback` is what runs when a name is not in the table. It is given the name first and then the rest of the values. Leave it out and an unknown name raises an error instead.

```luau
local command = switch.new({
	move = function(x: number, y: number)
		return `moving to {x}, {y}`
	end,
	stop = function()
		return "stopping"
	end,
}, function(name: string)
	return `no idea what {name} means`
end)

print(command("move", 4, 9))
print(command("stop"))
print(command("dance"))
```

## Calling a switch

```luau
handle(key: string, ...: any): ...any
```

The first value is the name of the case. Everything after it is passed to the case function, and everything the case returns comes back to you.

```luau
local math = switch.new({
	add = function(a: number, b: number)
		return a + b
	end,
	swap = function(a: number, b: number)
		return b, a
	end,
})

print(math("add", 2, 3))

local left, right = math("swap", 1, 2)
print(left, right)
```

## It runs on a new coroutine

The case does not run on the coroutine that called the switch. luv starts a new one for it and holds up the caller until it is done, then passes the results back.

That means a case can wait for anything the rest of the game can wait for, and the call still reads like a plain function call.

```luau
local Net = import("Net")

local action = switch.new({
	fetch = function(url: string)
		return Net.Request({ url = url }).body
	end,
	pause = function(seconds: number)
		task.wait(seconds)
		return "awake"
	end,
})

print(#action("fetch", "https://example.com"))
print(action("pause", 1))
```

Only the coroutine that called the switch waits. The rest of the game keeps going. See [Yielding and coroutines](../manual/yielding.md).

Because the case is on its own coroutine, one case can safely wait on the same signal another case is waiting on, and a case cannot leave the caller in a half finished state.

To start a case and not wait for it, put the call in [task.spawn](task.md#spawn).

```luau
task.spawn(action, "pause", 5)
print("not waiting")
```

## Errors in a case

An error inside a case reaches the code that called the switch, the same as if you had called the function yourself. Wrap the call in [pcall](https://luau.org/library#pcall) to handle it.

```luau
local run = switch.new({
	risky = function()
		error("it broke")
	end,
})

local ok, problem = pcall(run, "risky")
print(ok, problem)
```

## When to use one

A switch fits wherever a name arrives at runtime and something has to happen.

| Where the name comes from | What the cases do |
| --- | --- |
| A message over the network | One case for each kind of message. |
| A console or chat command | One case for each command. |
| A save file or a data table | One case for each kind of entry a level can hold. |
| A mod loaded with [ecall](globals.md#ecall) | One case for each thing a mod is allowed to ask for. |

That last one is worth a look. A switch is a closed list of names, so a mod can only reach the cases you put in it.

```luau
local api = switch.new({
	spawn = function(kind: string)
		return world:add(kind)
	end,
	sound = function(name: string)
		return playSound(name)
	end,
}, function(name: string)
	error(`mods cannot call {name}`)
end)

SetGlobal("modApi", api)
```

See [SetGlobal](globals.md#setglobal) and [ExternalModule](externalmodule.md).

## Errors

| Message | Cause |
| --- | --- |
| `switch.new takes a table of strings to functions, got a <type> key` | A key of the table is not a string. |
| `the case '<name>' must be a function, got a <type>` | A value of the table is not a function. |
| `switch.new needs at least one case` | The table was empty and there was no fallback. |
| `a switch is called with a string` | The switch was called with nothing. |
| `a switch is called with a string, got a <type>` | The first value was not a string. |
| `no case for '<name>'` | The name is not a case and there is no fallback. |
