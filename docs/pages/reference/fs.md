# FS

Reads and writes files on disk and reads the files of the game.

```luau
local FS = import("FS")
```

## Description

FS works like the Lua `io` library, with extra functions for files and folders. Most functions yield the calling coroutine while the work runs on another thread. The rest of the game keeps running. See [Yielding and coroutines](../manual/yielding.md).

Most functions throw an error when they fail. [open](#open), [read](#read), [write](#write) and [close](#close) are different. They return `nil` and a message instead, like the Lua `io` library.

For a guide, see [Files and saving](../manual/files.md).

## Path rules

The start of a path decides where it points.

| Path starts with | Kind | Example |
| --- | --- | --- |
| `./` or `../` | [Game path](#game-paths) | `"../data/config.json"` |
| `@` | [Alias path](#alias-paths) | `"@Data/config.json"` |
| anything else | [Disk path](#disk-paths) | `"C:/Games/notes.txt"` |

### Game paths

A path that starts with `./` or `../` points into the files of the game. It starts from the folder of the script that makes the call. `require` uses the same rule. `.\`, `..\`, `.` and `..` work too.

| Script | Path | Points at |
| --- | --- | --- |
| `src/main.luau` | `"./data.json"` | `src/data.json` |
| `src/main.luau` | `"../data/config.json"` | `data/config.json` |
| `src/lib/init.luau` | `"./"` | The `src` folder. |

A game path cannot go above the root of the game. That errors with `<path> points outside the game's files`.

In `luv test` the game files are the files of the project. In a packed game they are the files inside the package. Loaded containers add their files too. See [Container](container.md).

### Alias paths

A path that starts with `@` uses an alias from a `.luaurc` or `.config.luau` file, the same way `require` does. luv looks for that file in the folder of the calling script and then in each parent folder. Alias names are not case sensitive. See [Scripts and modules](../manual/scripts.md).

```json
{
	"aliases": {
		"Data": "./data"
	}
}
```

With this `.luaurc` in the project root, `"@Data/config.json"` points at `data/config.json`.

- An alias to a folder of the game gives a game path.
- An alias to an absolute path, like `"C:/Saves"`, gives a disk path.
- `@self` is the folder of an `init.luau` module.
- An unknown alias errors with `@<alias> is not a valid alias`.

### Disk paths

Every other path is a normal path on disk, like `"C:/Games/notes.txt"` or `"/home/me/notes.txt"`. Disk paths are not limited to any folder.

A path like `"notes.txt"` with no `./` in front is a disk path too. It starts from the working folder of the program, which is [Process.cwd](process.md#cwd). It does not start from the game. To save data, build full paths from [Process.dirs](process.md#dirs).

### Game files are read only

- Every function that changes a game path errors with `the game's files are read-only`. [open](#open) with a mode that writes returns `nil` and that message instead.
- You can copy game files to disk with [copy](#copy).
- Scripts show up in [readDir](#readdir) and [exists](#exists), but they cannot be read. Reading one fails with `scripts can only be loaded with require`. A script is a `.luau` or `.lua` file that does not end in `.d.luau`.
- Some files are hidden, like `build.toml` and `.d.luau` files. See [What gets packed](../start/project-layout.md#what-gets-packed).

## Fields

| Name | Type | Description |
| --- | --- | --- |
| `stdin` | [File](file.md) | The standard input of the program. |
| `stdout` | [File](file.md) | The standard output of the program. |
| `stderr` | [File](file.md) | The standard error output of the program. |

These three files are in text mode. They cannot be closed or seeked.

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [open](#open)(path, mode) | [File](file.md)`?`, `string?`, `number?` | yes |
| [lines](#lines)(path, ...) | `() -> ...any` | yes |
| [input](#input)(file) | [File](file.md) | yes |
| [output](#output)(file) | [File](file.md) | yes |
| [read](#read)(...) | `...any` | yes |
| [write](#write)(...) | [File](file.md)`?`, `string?`, `number?` | yes |
| [close](#close)(file) | `boolean?`, `string?`, `number?` | yes |
| [type](#type)(value) | `string?` | no |
| [readFile](#readfile)(path) | `string` | yes |
| [writeFile](#writefile)(path, contents) | none | yes |
| [appendFile](#appendfile)(path, contents) | none | yes |
| [readDir](#readdir)(path) | `{ string }` | yes |
| [makeDir](#makedir)(path) | none | yes |
| [remove](#remove)(path) | none | yes |
| [removeDir](#removedir)(path) | none | yes |
| [rename](#rename)(from, to) | none | yes |
| [copy](#copy)(from, to) | none | yes |
| [exists](#exists)(path) | `boolean` | yes |
| [isFile](#isfile)(path) | `boolean` | yes |
| [isDir](#isdir)(path) | `boolean` | yes |
| [metadata](#metadata)(path) | [Metadata](#metadata-2) | yes |
| [tmpname](#tmpname)() | `string` | yes |

## Function descriptions

### open

```luau
FS.open(path: string, mode: string?): (File?, string?, number?)
```

Opens a file and returns a [File](file.md). The default mode is `"r"`.

| Mode | Reads | Writes | Missing file | Old content |
| --- | --- | --- | --- | --- |
| `"r"` | yes | no | fails | kept |
| `"w"` | no | yes | created | removed |
| `"a"` | no | at the end | created | kept |
| `"r+"` | yes | yes | fails | kept |
| `"w+"` | yes | yes | created | removed |
| `"a+"` | yes | at the end | created | kept |

Add `b` for binary mode, like `"rb"`, `"wb"` or `"r+b"`. The `b` goes after the `+`. See [Text and binary mode](file.md#text-and-binary-mode).

When the file cannot be opened, `open` returns `nil`, a message and an error code. The message starts with the path. The code is the error number of the system, or `0`. This happens for a missing file, a folder, a script, or a game file opened with a mode that writes.

A bad mode is an error, like `bad argument #2 to 'open' (invalid mode 'rw')`. A bad path, like an unknown alias, is an error too.

```luau
local FS = import("FS")
local Process = import("Process")

local file, message = FS.open(Process.dirs.temp .. "/scores.txt", "w")
if not file then
	error(message)
end
file:write("top score ", 1200, "\n")
file:close()
```

### lines

```luau
FS.lines(path: string?, ...: ReadFormat): () -> ...any
```

Returns a function for a `for` loop. Each call reads with the given [formats](#readformat). With no formats it reads one line.

- With a path, it opens that file for reading. The file closes when the loop reaches the end. If the file cannot be opened, it errors with `<path>: <message>`.
- With a [File](file.md) in place of the path, it reads that file. The file stays open at the end.
- With `nil`, it reads the default input. See [input](#input).

Errors while reading are thrown inside the loop. Each step yields the calling coroutine.

```luau
local FS = import("FS")

for line in FS.lines("../data/names.txt") do
	print(line)
end
```

### input

```luau
FS.input(file: (File | string)?): File
```

Sets or gets the default input. [read](#read) and `FS.lines()` read from it. It starts as `FS.stdin`.

- With a path, it opens that file for reading and makes it the default input. It errors with `<path>: <message>` when the file cannot be opened.
- With a [File](file.md), it makes that file the default input.
- With nothing, it changes nothing.

It returns the default input.

### output

```luau
FS.output(file: (File | string)?): File
```

Sets or gets the default output. [write](#write) and [close](#close) use it. It starts as `FS.stdout`. It works like [input](#input), but a path is opened with the mode `"w"`. So a file that is already there gets emptied.

```luau
local FS = import("FS")
local Process = import("Process")

local path = Process.dirs.temp .. "/log.txt"
FS.output(path)
FS.write("hello ", 1, "\n")
FS.close()
FS.output(FS.stdout)
print(FS.readFile(path))
```

### read

```luau
FS.read(...: ReadFormat): ...any
```

Reads from the default input. It is the same as `FS.input():read(...)`. See [File:read](file.md#read).

### write

```luau
FS.write(...: string | number): (File?, string?, number?)
```

Writes to the default output. It is the same as `FS.output():write(...)`. See [File:write](file.md#write).

### close

```luau
FS.close(file: File?): (boolean?, string?, number?)
```

Closes a file. With no argument it closes the default output. It returns `true` when it worked. The standard files cannot be closed. For them it returns `nil` and `cannot close standard file`. Closing a file that is already closed is an error.

After you close the default output, set a new one with [output](#output) before you call [write](#write) again.

### type

```luau
FS.type(value: any): ("file" | "closed file")?
```

Returns `"file"` for an open [File](file.md), `"closed file"` for a closed or destroyed File and `nil` for anything else. It does not yield.

### readFile

```luau
FS.readFile(path: string): string
```

Returns all bytes of a file as a string. It errors with `cannot read <path>: <message>`.

```luau
local FS = import("FS")

local config = FS.readFile("../data/config.json")
print(#config)
```

### writeFile

```luau
FS.writeFile(path: string, contents: string | buffer)
```

Writes a string or a buffer to a file on disk. It creates the file or replaces all of its content. It does not create missing folders, so call [makeDir](#makedir) first. It errors with `cannot write <path>: <message>`. Contents that are not a string or a buffer are an error too.

### appendFile

```luau
FS.appendFile(path: string, contents: string | buffer)
```

Adds a string or a buffer to the end of a file on disk. It creates the file when it is missing. It errors with `cannot append to <path>: <message>`.

### readDir

```luau
FS.readDir(path: string): { string }
```

Returns the names of the files and folders inside a folder, sorted. The names do not include the folder path. It errors with `cannot read directory <path>: <message>`.

```luau
local FS = import("FS")

for _, name in FS.readDir("@Data/levels") do
	print(name)
end
```

### makeDir

```luau
FS.makeDir(path: string)
```

Creates a folder on disk, with every missing parent folder. It does nothing when the folder is already there. It errors with `cannot create directory <path>: <message>`.

### remove

```luau
FS.remove(path: string)
```

Removes a file or an empty folder on disk. It errors with `cannot remove <path>: <message>`. Use [removeDir](#removedir) for a folder with things in it.

### removeDir

```luau
FS.removeDir(path: string)
```

Removes a folder on disk and everything inside it. It errors with `cannot remove <path>: <message>`.

> [!WARNING]
> This cannot be undone. Check the path first.

### rename

```luau
FS.rename(from: string, to: string)
```

Renames or moves a file or a folder on disk. Both paths must point at the disk. It errors with `cannot rename <from>: <message>`.

### copy

```luau
FS.copy(from: string, to: string)
```

Copies a file or a whole folder. `from` can be a disk path or a game path. `to` must point at the disk.

- For a file, `to` is the path of the new file. A file that is already there is replaced.
- For a folder, it copies everything inside. Missing folders are created. Files that are already there are replaced.
- Scripts inside a game folder are skipped.

It errors with `cannot copy <from>: <message>`.

```luau
local FS = import("FS")
local Process = import("Process")

FS.copy("@Data/levels", Process.dirs.temp .. "/levels")
print(FS.readDir(Process.dirs.temp .. "/levels")[1])
```

### exists

```luau
FS.exists(path: string): boolean
```

Returns `true` when a file or folder is at the path. A missing path is not an error. Scripts of the game count as existing.

### isFile

```luau
FS.isFile(path: string): boolean
```

Returns `true` when the path is a file. On disk it follows symbolic links.

### isDir

```luau
FS.isDir(path: string): boolean
```

Returns `true` when the path is a folder. On disk it follows symbolic links.

### metadata

```luau
FS.metadata(path: string): Metadata
```

Returns a [Metadata](#metadata-2) table for a file or folder. On disk it does not follow symbolic links, so a link has the kind `"symlink"`. It errors with `cannot read metadata of <path>: <message>`.

```luau
local FS = import("FS")

local info = FS.metadata("@Data/config.json")
print(info.kind, info.size, info.readonly)
```

### tmpname

```luau
FS.tmpname(): string
```

Creates a new empty file in the temp folder of the system and returns its full path. The name looks like `luv-<pid>-<number>`. Remove the file with [remove](#remove) when you are done.

## Metadata

The table that [FS.metadata](#metadata) returns.

| Name | Type | Description |
| --- | --- | --- |
| `kind` | `"file" | "dir" | "symlink"` | What is at the path. Game paths are only `"file"` or `"dir"`. |
| `size` | `number` | The size in bytes. Game folders have the size `0`. |
| `readonly` | `boolean` | `true` when the file is read only. Always `true` for game paths. |
| `modified` | `number?` | When the file last changed, in seconds since 1 January 1970. It is `nil` for game paths and when the system does not know. |
| `created` | `number?` | When the file was created. Same format as `modified`. |
| `accessed` | `number?` | When the file was last read. Same format as `modified`. |

## ReadFormat

A read format says how much to read. [File:read](file.md#read), [File:lines](file.md#lines), [FS.read](#read) and [FS.lines](#lines) take any number of them.

| Name | Type | Description |
| --- | --- | --- |
| `"l"` | `string` | The next line without its line ending. This is the default. |
| `"L"` | `string` | The next line with its line ending. |
| `"n"` | `string` | A number. |
| `"a"` | `string` | Everything that is left. |
| a count | `number` | Up to that many bytes. |

Each name can also start with `*`, like `"*l"`. See [Read formats](file.md#read-formats) for what each format returns at the end of a file.
