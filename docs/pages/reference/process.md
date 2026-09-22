# Process

Information about the program, other programs and the life of the game.

```luau
local Process = import("Process")
```

## Description

Process has the command line arguments, the environment variables and the folders of the system. It runs other programs with [spawn](#spawn) and [start](#start). It also has the [Heartbeat](#heartbeat) signal, [BindToClose](#bindtoclose) and [exit](#exit).

The `args`, `env` and `dirs` tables are read only.

## Fields

| Name | Type | Description |
| --- | --- | --- |
| `args` | `{ string }` | The arguments after `--` in `luv test` and `luv run`. A packed game gets all arguments of its command line. See [Command line](../start/command-line.md). |
| `env` | `{ [string]: string }` | The environment variables from when the game started. Names keep the case of the system, like `Path` on Windows. |
| `os` | `string` | `"windows"` or `"linux"`. |
| `arch` | `string` | The CPU type, like `"x86_64"` or `"aarch64"`. |
| `pid` | `number` | The process id of the game. |
| `executable` | `string?` | The full path of the running program. This is `luv` for `luv test` and `luv run`, and the game program when the game is packed. |
| `gameName` | `string` | The `name` from [build.toml](../start/build-toml.md). |
| `dirs` | [Dirs](#dirs) | Folders of the system and the game, like the save folder. |
| `Heartbeat` | [Signal](signal.md)`<number>` | Fires 60 times a second. See [Heartbeat](#heartbeat). |

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [cwd](#cwd)() | `string` | no |
| [exit](#exit)(code) | never returns | yes |
| [spawn](#spawn)(program, args, options) | [ProcessResult](#processresult) | yes |
| [start](#start)(program, args, options) | [Child](child.md) | no |
| [BindToClose](#bindtoclose)(callback) | none | no |

## Function descriptions

### cwd

```luau
Process.cwd(): string
```

Returns the working folder of the program. Usually that is the folder you started `luv` or the game from. It is not the game folder. Disk paths like `"notes.txt"` start from here. See [Disk paths](fs.md#disk-paths).

### exit

```luau
Process.exit(code: (number | boolean)?): never
```

Starts closing the game and sets the exit code of the program.

| code | Exit code |
| --- | --- |
| `nil` or `true` | `0` |
| `false` | `1` |
| a number | That number without its fraction. |

The coroutine that calls `exit` never goes on. The other coroutines keep running while the [BindToClose](#bindtoclose) callbacks run. Then the game ends. See [Shutdown order](#shutdown-order).

- Only the first call sets the code. Later calls keep it.
- The code from `exit` is used even when errors happened before.
- Use codes from 0 to 255. Those work on every system.
- Other types are an error, like `bad argument #1 to 'exit' (number or boolean expected, got string)`.

```luau
local Process = import("Process")

if Process.args[1] == "--version" then
	print("1.0.0")
	Process.exit(0)
end
```

### spawn

```luau
Process.spawn(program: string, args: { string }?, options: ProcessOptions?): ProcessResult
```

Runs a program, waits until it ends and returns a [ProcessResult](#processresult). This yields the calling coroutine until the program ends. The options are in [ProcessOptions](#processoptions).

- luv runs `program` and passes each string in `args` as it is. It looks for the program in the folders of the `PATH` variable.
- With `shell = true`, luv runs the line through `cmd /d /s /c` on Windows and `sh -c` on Linux. It joins `program` and `args` with spaces and quotes nothing. So you can put a whole command line in `program`.
- The output is kept in `stdout` and `stderr`. With `stdio = "inherit"`, the output goes to the console of the game and both strings are empty.
- The program gets `stdin` from the options as its input. Without it the input is empty. With `stdio = "inherit"` and no `stdin`, the program reads the console of the game.

A program that ends with an error code is not an error. Check `ok` and `code`. `spawn` errors with `cannot start '<program>': <message>` when the program cannot start, for example when it does not exist.

```luau
local Process = import("Process")

local result = Process.spawn("git", { "rev-parse", "--short", "HEAD" }, { cwd = Process.dirs.game })
if result.ok then
	print("commit", result.stdout)
else
	print("git failed", result.code, result.stderr)
end
```

This one runs a command line through the shell and passes a variable:

```luau
local Process = import("Process")

local command = if Process.os == "windows" then "echo %NAME%" else "echo $NAME"
local result = Process.spawn(command, nil, { shell = true, env = { NAME = "luv" } })
print(result.stdout)
```

### start

```luau
Process.start(program: string, args: { string }?, options: ProcessOptions?): Child
```

Starts a program and returns a [Child](child.md) right away. It does not wait for the program to end. It takes the same arguments and options as [spawn](#spawn). It does not yield.

- `Stdin`, `Stdout` and `Stderr` of the Child are [File](file.md) objects connected to the program.
- With `stdin` in the options, luv writes that text to the program and then closes its input. `Stdin` is `nil` then.
- With `stdio = "inherit"`, the program uses the console of the game. `Stdin`, `Stdout` and `Stderr` are all `nil`.

It errors with `cannot start '<program>': <message>`.

The game does not wait for a started program, and the program keeps running when the game ends. Call [Kill](child.md#kill) to stop it.

```luau
local Process = import("Process")

local child = Process.start("sort", nil, { shell = true })
local input, output = child.Stdin, child.Stdout
if input and output then
	input:write("pear\n", "apple\n")
	input:close()
	print(output:read("a"))
end
print(child:Wait().code)
```

### BindToClose

```luau
Process.BindToClose(callback: () -> ())
```

Adds a function that runs once when the game closes. Use it to save data. It does not yield.

- Callbacks run in the order they were added. Each one runs in its own coroutine, so it can yield. For example, it can write a file.
- The game waits until every callback has returned, but no longer than 30 seconds.
- A callback added after the game started closing does not run.
- Do not call [exit](#exit) inside a callback. That callback never returns, so the game waits the full 30 seconds.

```luau
local FS = import("FS")
local Process = import("Process")
local Serde = import("Serde")

local state = { level = 1, coins = 0 }
Process.BindToClose(function()
	local folder = Process.dirs.save
	if folder then
		FS.makeDir(folder)
		FS.writeFile(folder .. "/state.json", Serde.Encode("json", state))
	end
end)
```

## Signals

### Heartbeat

```luau
Process.Heartbeat: Signal<number>
```

Fires 60 times a second. The argument is the time in seconds since the last beat. When the game falls behind, missed beats are skipped. The next argument then covers the whole gap.

Each handler runs in a new coroutine on each beat. While a handler is bound or a coroutine waits on it, Heartbeat keeps the game running. Unbind your handlers to let the game end.

Heartbeat is a timer. It is not tied to the frames of a window. For drawing, use the frame signals of [Window](window.md).

```luau
local Process = import("Process")

local elapsed = 0
Process.Heartbeat:BindHandler("clock", function(delta: number)
	elapsed += delta
	if elapsed >= 5 then
		Process.Heartbeat:UnBind("clock")
		print("five seconds passed")
	end
end)
```

## Shutdown order

The game starts to close when one of these happens:

- A script calls [exit](#exit).
- You press Ctrl+C. The exit code is `130`. A second Ctrl+C ends the program at once.
- Nothing keeps the game running anymore. Running coroutines, open sockets and servers and a bound [Heartbeat](#heartbeat) are some of the things that keep it running.

Then:

1. Every [BindToClose](#bindtoclose) callback starts in its own coroutine.
2. Other coroutines keep running. Heartbeat still fires and sockets still get data.
3. The game ends when every callback has returned, or 30 seconds after closing started.
4. Everything that is still running stops. Open sockets and servers are dropped.
5. The program exits. The code is the one from `exit`. Without `exit` it is `0`, or `1` when errors happened. See [Exit codes](../start/command-line.md#exit-codes).

## Dirs

The table in `Process.dirs`. Each value is a full path. On Windows the paths use `\`. FS accepts both `\` and `/`, so you can add `"/save.json"` to a path.

| Name | Type | Description |
| --- | --- | --- |
| `home` | `string?` | The home folder of the user. |
| `appData` | `string?` | The folder for app data. `AppData\Roaming` on Windows, `~/.local/share` on Linux. |
| `localAppData` | `string?` | The folder for local app data. `AppData\Local` on Windows, `~/.local/share` on Linux. |
| `config` | `string?` | The folder for settings. `AppData\Roaming` on Windows, `~/.config` on Linux. |
| `cache` | `string?` | The folder for cache files. `AppData\Local` on Windows, `~/.cache` on Linux. |
| `temp` | `string` | The temp folder of the system. It is always set. |
| `documents` | `string?` | The documents folder of the user. |
| `desktop` | `string?` | The desktop folder of the user. |
| `downloads` | `string?` | The downloads folder of the user. |
| `pictures` | `string?` | The pictures folder of the user. |
| `music` | `string?` | The music folder of the user. |
| `videos` | `string?` | The videos folder of the user. |
| `game` | `string?` | The folder of the game. See the table below. |
| `save` | `string?` | The save folder of the game. See below. |

A value is `nil` when the system has no such folder.

The `game` folder depends on how the game runs:

| How the game runs | `game` folder |
| --- | --- |
| `luv test` | The project folder. |
| `luv run` | The folder of the `.luvit` file. |
| Packed game | The folder of the game program. |

`save` is the `appData` folder plus the game name, like `C:\Users\you\AppData\Roaming\My Game`. The characters `< > : " / \ | ? *` in the name become `_`. So `My: Game` becomes `My_ Game`.

> [!NOTE]
> luv does not create the save folder. Call [FS.makeDir](fs.md#makedir) before you write to it.

Write save data to `save`, not to `game`. An installed game often cannot write to its own folder.

## ProcessOptions

The options table for [spawn](#spawn) and [start](#start). Every field is optional.

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `cwd` | `string?` | the working folder of the game | The working folder of the program. This is a normal disk path. Game paths and aliases do not work here. |
| `env` | `{ [string]: string }?` | none | Extra environment variables. They are added to the ones the game has. |
| `clearEnv` | `boolean?` | `false` | Start from no environment variables. The program gets only `env`. |
| `shell` | `boolean?` | `false` | Run the line through `cmd` on Windows or `sh` on Linux. |
| `stdin` | `string?` | none | Text sent to the input of the program. The input is closed after it. |
| `stdio` | `("capture" | "inherit")?` | `"capture"` | `"capture"` gives you the output. `"inherit"` connects the program to the console of the game. |

Another `stdio` value errors with `invalid stdio option 'x', expected "capture" or "inherit"`.

## ProcessStatus

How a program ended. [Child:Wait](child.md#wait) returns it.

| Name | Type | Description |
| --- | --- | --- |
| `ok` | `boolean` | `true` when the program ended with success. |
| `code` | `number` | The exit code of the program. It is `-1` when there is no exit code. |

## ProcessResult

Inherits: [ProcessStatus](#processstatus)

What [spawn](#spawn) returns. It has `ok` and `code` from ProcessStatus, plus these fields:

| Name | Type | Description |
| --- | --- | --- |
| `stdout` | `string` | Everything the program wrote to its output. |
| `stderr` | `string` | Everything the program wrote to its error output. |

The bytes are kept as they are. Both strings are empty with `stdio = "inherit"`.
