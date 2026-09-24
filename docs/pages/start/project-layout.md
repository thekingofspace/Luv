# Project layout

A luv project is a normal folder. `luv init` fills it with a few files. This page shows what each one is for.

## The folder tree

```tree
my-game/
├── .gitignore
├── .vscode/
│   └── settings.json
├── assets/
├── build.toml
├── native/
│   ├── luv.h
│   └── luv.rs
├── src/
│   └── main.luau
└── types.d.luau
```

| Path | What it is |
| --- | --- |
| `.gitignore` | Tells Git to skip the `build` folder. |
| `.vscode/settings.json` | Editor settings for the Luau language server. See [Editor setup](editor-setup.md). |
| `assets/` | Images, sounds, fonts, shaders and any other data. `Asset.Load("x.png")` reads `assets/x.png`. |
| `build.toml` | The project file. It holds the game name, version and main script. See [The build.toml file](build-toml.md). |
| `native/` | C files, C++ files and Rust crates that luv builds into native plugins. See [Native plugins](../manual/native-plugins.md). |
| `native/luv.h` | The C header for native plugins. |
| `native/luv.rs` | The same API for Rust plugins. |
| `src/main.luau` | The first script that runs. You can change it in `build.toml`. |
| `types.d.luau` | Type info for your editor. luv writes it from the engine types and every plugin type file. It is never packed into the game. See [How luv builds it](editor-setup.md#how-luv-builds-it). |

## Where scripts can live

Scripts can sit anywhere in the project, not only in `src`. Every `.luau` and `.lua` file that gets packed turns into bytecode. You load them with `require`. See [Scripts and modules](../manual/scripts.md).

## The build folder

`luv test`, `luv build` and `luv package` write into `build/`. You can change the name in `build.toml`.

```tree
build/
├── My-Game.luvit
├── aliases.json
├── cube.dll
├── Expansion.cont
├── native-objects/
├── native-target/
└── package/
    ├── My-Game.exe
    ├── cube.dll
    └── Expansion.cont
```

| Path | What it is |
| --- | --- |
| `My-Game.luvit` | The packed game made by `luv build`. It holds bytecode and assets. |
| `aliases.json` | The list of aliases luv wrote into `.luaurc`. See [Scripts and modules](../manual/scripts.md#aliases-luv-writes-for-you). |
| `cube.dll` or `libcube.so` | Native plugins built from `native/`. |
| `Expansion.cont` | One file for each container. See [Containers and DLC](../manual/containers.md). |
| `native-objects/` | Temporary files from the C compiler. |
| `native-target/` | The Cargo folder for Rust plugins. |
| `package/` | The finished game made by `luv package`. Ship this folder. |

The file names come from the game name. Every character that is not a letter, digit, `-` or `_` becomes `-`. So `My Game` becomes `My-Game`.

## What gets packed

When you build, luv packs every file in the project except these:

- `build.toml` and every `container.toml`.
- The `native` folder.
- Files ending in `.d.luau`.
- Files and folders whose names start with a dot. `.luaurc` and `.config.luau` are the only exceptions.
- Native libraries such as `.dll` and `.so` files. luv warns you if it finds one outside `native`.
- The build folder.
- Container folders. Each container becomes its own `.cont` file.

`luv test` hides the same files, so a test run matches the packed game.

## Updating the project files

Run `luv init` again inside an existing project after you update luv. It refreshes `types.d.luau`, `native/luv.h` and `native/luv.rs`. It never touches `build.toml`, your scripts or your assets.

`types.d.luau` is built from nothing each time, out of the engine types and every `.d.luau` file in a `native` folder. `luv test` and `luv build` do the same, and [luv types](command-line.md#luv-types) does only that step.
