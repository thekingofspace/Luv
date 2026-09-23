# Command line

Everything you do with luv goes through the `luv` command. Add `--help` after any command to see its options.

```shell
luv init my-game --name "My Game"
luv test my-game
luv build my-game
luv run my-game
luv package my-game
luv luaurc my-game
```

## luv init

```shell
luv init [path] [--name <name>]
```

Makes a new project in `path`. The default path is the current folder. The default name is the folder name.

Running it on an existing project updates `types.d.luau`, `native/luv.h` and `native/luv.rs` to match your version of luv. It never changes `build.toml`, your scripts or your assets. Each line of output shows what happened to a file:

| Mark | Meaning |
| --- | --- |
| `+` | The file was created. |
| `~` | The file was updated. |
| `=` | The file already exists and was left alone. |

## luv test

```shell
luv test [path] [-- args...]
```

Runs the game straight from its source files. This is the command you use while working. It first builds anything in `native/` and any containers, then runs the main script.

Anything after `--` goes to the game in `Process.args`.

```shell
luv test -- --level 2
```

```luau
local Process = import("Process")

print(Process.args[1], Process.args[2])
```

## luv build

```shell
luv build [path]
```

Packs the game into `build/<name>.luvit`. Scripts turn into bytecode, so no source code ships. It also builds native plugins and containers.

## luv run

```shell
luv run [path] [-- args...]
```

Runs a packed game. `path` can be a `.luvit` file or a project folder. For a folder it runs the `.luvit` file in the build folder. It does not build anything first, so run `luv build` before it.

## luv package

```shell
luv package [path] [--console]
```

Does everything `luv build` does, then makes the finished game in `build/package/`. The folder holds one program plus any native plugins and containers. See [Shipping your game](../manual/shipping.md).

On Windows the game opens without a console window. Add `--console` to keep one.

## luv luaurc

```shell
luv luaurc [path]
```

Writes the container aliases into `.luaurc` without building anything. `luv aliases` does the same. `luv test`, `luv build` and `luv package` already do this, so you only need the command when you want the file updated on its own, such as right after you add a container and want your editor to find it.

It prints one line per change:

```shell
Updated C:\my-game\.luaurc
  + Expansion
```

| Mark | Meaning |
| --- | --- |
| `+` | The alias was added. |
| `~` | The alias moved, so luv pointed it at the new folder. |
| `-` | The container is gone, so luv took the alias out. |

When nothing changed it says the file is already up to date. See [Scripts and modules](../manual/scripts.md#aliases-luv-writes-for-you).

## luv --version

Prints the installed version, for example `luv 0.1.0`.

## Exit codes

| Code | When |
| --- | --- |
| `0` | The game ended normally. |
| `1` | The project could not be built, or the game stopped because of an error. |
| `2` | The command line was wrong. |
| `130` | You pressed Ctrl+C. |

A game can pick its own code with [Process.exit](../reference/process.md#exit).

## Errors in the game

Each error that no script catches prints as `error: <message>`. The game keeps running other code. When it ends, luv prints how many errors happened and exits with code `1`.

A packed game on Windows has no console, so it also shows a message box with the errors.
