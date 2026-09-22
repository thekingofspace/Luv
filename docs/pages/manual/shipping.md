# Shipping your game

When your game is ready, `luv package` turns your project into a folder that players can run without luv. This page shows what goes into that folder and what to check before you share it.

## luv build and luv package

| Command | Makes | Use it for |
| --- | --- | --- |
| `luv build` | `build/<name>.luvit` | Trying the packed game with `luv run`. |
| `luv package` | `build/package/` | The finished game for players. |

`luv package` does everything `luv build` does, then makes the package folder. A `.luvit` file needs luv to run. The package folder does not. See [Command line](../start/command-line.md).

```shell
luv build
luv run
luv package
```

## The package folder

```tree
build/package/
├── Expansion.cont
├── Neon-Pong.exe
└── particles.dll
```

| Path | What it is |
| --- | --- |
| `Expansion.cont` | A container. See [Containers and DLC](containers.md). |
| `Neon-Pong.exe` | The game program. It holds the engine and the packed game. On Linux it is named `Neon-Pong`, with no extension. |
| `particles.dll` | A native plugin built from `native/`. On Linux it is named `libparticles.so`. See [Native plugins](native-plugins.md). |

On Linux the folder also holds `Neon-Pong.png` when `build.toml` sets an icon. See [The game icon](#the-game-icon).

The program name comes from the game name. See [The build folder](../start/project-layout.md#the-build-folder).

luv deletes and remakes `build/package/` each time you run `luv package`. Do not keep your own files in it.

Ship the whole folder. The game finds its plugins and containers next to the program, no matter which folder a player starts it from. Players can also pass arguments to the program. All of them go to [Process.args](../reference/process.md).

## What goes where

| What | Where it ends up |
| --- | --- |
| Scripts | Inside the program, as bytecode. |
| Assets and other packed files | Inside the program. |
| Native plugins from `native/` | Next to the program. |
| Containers | Next to the program, one `.cont` file each. |
| `build.toml`, `types.d.luau` and the other files that luv leaves out | Not shipped. |

The full list of files that luv leaves out is in [What gets packed](../start/project-layout.md#what-gets-packed).

Native libraries are never packed into the program. A `.dll` or `.so` file outside `native/` does not ship at all. luv warns you about it:

```text
warning: assets/stray.dll is a native library, so it is not packed into the game, move it into native/ to ship it next to the game
```

Packed files are read only. The game can read them with [Asset](../reference/asset.md) and [FS](../reference/fs.md). It cannot change them. Write player data to the [save folder](#the-save-folder) instead.

## The game icon

Set `icon` in [build.toml](../start/build-toml.md#the-game-icon) to give the game an icon. It can be an `.ico` file or any image luv can read, like PNG or SVG.

```toml
[game]
name = "Neon Pong"
icon = "assets/icon.svg"
```

- On Windows, `luv package` puts the icon inside the `.exe`. `luv package` prints `+ icon from assets/icon.svg` when it does.
- On Linux, programs cannot hold an icon. `luv package` writes `Neon-Pong.png` next to the program instead, for desktop shortcuts.
- The game windows use the icon too, unless a script sets its own `Icon`.

## The console on Windows

A packed game opens without a console window, so players do not see the output of `print`. Add `--console` to keep a console window next to the game:

```shell
luv package --console
```

This helps when you test a packed game, or when your game is a command line tool. On Linux the flag does nothing. The game prints to the terminal that started it.

## The error message box

A packed game on Windows shows a message box when it stops because of an error and has no console window. The title is the program name. The text lists the first five error messages, then the reason the game stopped.

With `--console` there is no message box. The errors print in the console instead. On Linux the errors print to the terminal. See [Errors in the game](../start/command-line.md#errors-in-the-game).

## Linux file permissions

The Linux program has no file extension. luv sets its file mode to `755`, so it can run right away. Some zip tools drop this mode. If the game does not start, run this in its folder:

```shell
chmod +x Neon-Pong
```

## One package for each system

`luv package` uses the luv program you run as the engine of the game. So a package made on Windows only runs on Windows. Make the Linux package on Linux. Native plugins are built for the same system too.

## Use a release build of luv

The game program is a copy of your luv with the game added to it. A debug build of luv makes the game run slower. If your luv is a debug build, `luv package` prints this note:

```text
note: this luv is a debug build, package with a release build of luv for the best speed
```

luv from Rokit or from a GitHub release is a release build. If you build luv from source, install it with `cargo install --path . --locked`. See [Installing luv](../start/installing.md).

## The save folder

Use [Process.dirs.save](../reference/process.md) for save files. It is the user data folder of the system, plus the game name.

| System | Save folder |
| --- | --- |
| Windows | `C:\Users\<you>\AppData\Roaming\<game name>` |
| Linux | `~/.local/share/<game name>` |

- The game name comes from `name` in `build.toml`. Characters that file names cannot hold, like `:` and `?`, become `_`.
- On Linux, luv uses `$XDG_DATA_HOME` instead of `~/.local/share` when it is set.
- The folder is the same for `luv test`, `luv run` and the packed game. Saves you make while testing show up in the packed game too.
- luv does not create the folder. Make it with `FS.makeDir` before you write to it.
- It is `nil` when the system has no user data folder.

```luau
local FS = import("FS")
local Process = import("Process")
local Serde = import("Serde")

local folder = Process.dirs.save
if folder then
	FS.makeDir(folder)
	FS.writeFile(`{folder}/save.json`, Serde.Encode("json", { level = 3 }))
end
```

See [Files and saving](files.md) for more.

## Checklist

1. Use a release build of luv on each system you ship for.
2. Run `luv package`.
3. Start the program in `build/package/` once to check that it works.
4. Share the whole `build/package/` folder, for example as a zip file.
