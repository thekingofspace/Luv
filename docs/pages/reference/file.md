# File

Inherits: [BaseGameObject](basegameobject.md)

An open file, a standard stream or a pipe to another program.

## Description

You get a File from:

- [FS.open](fs.md#open).
- The `stdin`, `stdout` and `stderr` [fields of FS](fs.md#fields).
- [Asset:Open](asset.md#open).
- The `Stdin`, `Stdout` and `Stderr` properties of a [Child](child.md).

The methods work like the file methods of the Lua `io` library. Their names are lowercase.

Most methods yield the calling coroutine while the work runs. Many coroutines can use the same File at once. Their calls run one after another.

Each write reaches the system right away. A File that is never closed gets closed when it is garbage collected. It is still good to close files yourself.

## Failures

When the work of a method fails, the method returns `nil`, a message and an error code. It does not throw. The code is the error number of the system, or `0`.

```luau
local FS = import("FS")

local file = FS.open("../data/config.json")
if file then
	local ok, message = file:write("x")
	print(ok, message)
	file:close()
end
```

This prints `nil` and `the game's files are read-only`, because game files cannot be written.

Using a closed file is different. That throws the error `attempt to use a closed file`.

## Text and binary mode

A File opened without `b` in its mode is in text mode. The standard files, pipes and asset files are in text mode too.

The mode changes only one thing. In text mode the `"l"` format drops a `\r` that comes right before the `\n` at the end of a line. In binary mode the `\r` stays. Nothing else is changed.

## Read formats

| Format | Reads | At the end of the file |
| --- | --- | --- |
| `"l"` | The next line without its line ending. This is the default. | `nil` |
| `"L"` | The next line with its line ending. | `nil` |
| `"n"` | A number. Spaces and new lines before it are skipped. Decimal numbers and hex numbers that start with `0x` work. | `nil` |
| `"a"` | Everything that is left. | `""` |
| a count | Up to that many bytes. A count of `0` returns `""`. | `nil` |

- The last line of a file can end without a line ending. `"l"` and `"L"` still return it.
- `"n"` returns `nil` when there is no valid number. The characters it looked at are gone.
- A format can start with `*`, like `"*a"`.
- An unknown format errors with `bad argument #<n> to 'read' (invalid format)`.

The formats are also listed in [ReadFormat](fs.md#readformat).

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"File"`. Read only. |
| `Name` | `string` | The path given to [FS.open](fs.md#open), `"stdin"`, `"stdout"`, `"stderr"`, the path of an asset or a pipe name like `"stdout of sort"`. |

## Methods

### read

```luau
file:read(...: ReadFormat): ...any
```

Reads once for each format and returns one value for each. With no format it reads one line. See [Read formats](#read-formats).

When a format reaches the end of the file, `read` returns `nil` for it and stops. Formats after it are not read. This yields the calling coroutine.

```luau
local FS = import("FS")
local Process = import("Process")

local path = Process.dirs.temp .. "/scores.txt"
FS.writeFile(path, "scores\n42 3.5\n")
local file = FS.open(path)
if file then
	local title = file:read("l")
	local first, second = file:read("n", "n")
	print(title, first, second)
	file:close()
end
```

### write

```luau
file:write(...: string | number): (File?, string?, number?)
```

Writes each value in order and returns the file. Numbers are written the way `tostring` shows them. Other types are errors, buffers too. Use [FS.writeFile](fs.md#writefile) or [FS.appendFile](fs.md#appendfile) for a buffer.

Writing to a file opened only for reading fails. Writing to an asset or a game file fails with `the game's files are read-only`. This yields the calling coroutine.

### lines

```luau
file:lines(...: ReadFormat): () -> ...any
```

Returns a function for a `for` loop. Each call reads with the given formats, or one line when there are none. The loop ends at the end of the file. The file stays open. Errors while reading are thrown. Each step yields the calling coroutine.

```luau
local FS = import("FS")

local file = FS.open("../data/names.txt")
if file then
	for name in file:lines() do
		print(name)
	end
	file:close()
end
```

### seek

```luau
file:seek(whence: ("set" | "cur" | "end")?, offset: number?): (number?, string?, number?)
```

Moves the position of the file and returns the new position. The position counts bytes from the start.

| whence | Moves to |
| --- | --- |
| `"set"` | `offset` bytes from the start. `offset` cannot be negative. |
| `"cur"` | `offset` bytes from the current position. This is the default. |
| `"end"` | `offset` bytes from the end. |

`offset` defaults to `0`. So `file:seek()` returns the current position and `file:seek("end")` returns the size. The standard files and pipes cannot seek. For them it returns `nil` and `cannot seek on this file`. This yields the calling coroutine.

```luau
local FS = import("FS")
local Process = import("Process")

local path = Process.dirs.temp .. "/letters.txt"
FS.writeFile(path, "abcdef")
local file = FS.open(path, "r+")
if file then
	file:seek("set", 1)
	file:write("X")
	file:close()
end
print(FS.readFile(path))
```

This prints `aXcdef`.

### flush

```luau
file:flush(): (File?, string?, number?)
```

Makes sure every write reached the system and returns the file. Each write already does this, so you rarely need it. This yields the calling coroutine.

### close

```luau
file:close(): (boolean?, string?, number?)
```

Closes the file and returns `true`. The standard files cannot be closed. For them it returns `nil` and `cannot close standard file`. Closing a file twice is an error. This yields the calling coroutine.

Closing the `Stdin` of a [Child](child.md) tells the program that no more input comes.

### setvbuf

```luau
file:setvbuf(mode: "no" | "full" | "line", size: number?): boolean
```

Kept so code written for Lua keeps working. It does nothing and returns `true`. Other modes are errors. It does not yield.

### Destroy

```luau
file:Destroy()
```

Closes the file. Every later call errors with `attempt to use a closed file`. Do not destroy `FS.stdin`, `FS.stdout` or `FS.stderr`, because they cannot be used again after that. See [BaseGameObject](basegameobject.md#destroy).
