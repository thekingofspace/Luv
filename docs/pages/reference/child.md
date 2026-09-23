# Child

Inherits: [BaseGameObject](basegameobject.md)

A program started with [Process.start](process.md#start).

## Description

A Child connects you to a running program. Write to its input with `Stdin`. Read its output with `Stdout` and `Stderr`. All three are [File](file.md) objects.

Close `Stdin` when you are done writing. Many programs wait for that before they finish.

Read the output of a program that writes a lot. The program stops and waits while its output is not read. You can also start it with `stdio = "inherit"`. See [ProcessOptions](process.md#processoptions).

The game does not wait for a Child. A coroutine that calls [Wait](#wait) keeps the game running until the program ends. When the game ends, the program keeps running. Call [Kill](#kill) to stop it, for example in [BindToClose](process.md#bindtoclose).

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"Child"`. Read only. |
| `Name` | `string` | The `program` passed to [Process.start](process.md#start). |
| `Pid` | `number?` | The process id of the program. Read only. |
| `Stdin` | [File](file.md)`?` | Writes to the input of the program. It is `nil` when the options have `stdin` or `stdio = "inherit"`. Read only. |
| `Stdout` | [File](file.md)`?` | Reads the output of the program. It is `nil` with `stdio = "inherit"`. Read only. |
| `Stderr` | [File](file.md)`?` | Reads the error output of the program. It is `nil` with `stdio = "inherit"`. Read only. |

## Methods

### Wait

```luau
child:Wait(): ProcessStatus
```

Waits until the program ends and returns a [ProcessStatus](process.md#processstatus). This yields the calling coroutine. You can call it more than once. After the program has ended, it returns the same result right away.

```luau
local Process = import("Process")

local child = Process.start("git", { "log", "--oneline" })
local output = child.Stdout
if output then
	for line in output:lines() do
		print(line)
	end
end
local status = child:Wait()
print(status.ok, status.code)
```

### Kill

```luau
child:Kill()
```

Stops the program. It does not wait for it. Call [Wait](#wait) after it to know when the program is gone. A killed program reports `ok` as `false`. It does not yield.

### Destroy

```luau
child:Destroy()
```

Stops the program like [Kill](#kill) and marks the Child as destroyed. See [BaseGameObject](basegameobject.md#destroy).

It also closes `Stdin`, `Stdout` and `Stderr`, so the pipes are handed back right away instead of when the Child is collected. The three fields still give you the same [File](file.md), and reading or writing one errors.
