# The build.toml file

Every project has a `build.toml` file in its root. luv finds it by looking in the folder you give it and then in each parent folder.

## Example

```toml
[game]
name = "Neon Pong"
version = "0.1.0"
authors = ["Your Name"]
description = "A small pong game"
icon = "assets/icon.svg"
main = "src/main.luau"

[build]
output = "build"
```

## The game table

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `name` | string | required | The game name. Used for the output file names, the default window title, `Process.gameName` and the save folder. |
| `version` | string | `"0.1.0"` | Shown when you build and stored inside the packed game. |
| `authors` | list of strings | `[]` | Stored inside the packed game. |
| `description` | string | `""` | Stored inside the packed game. |
| `icon` | string | none | The game icon. The path is relative to the project root. See [The game icon](#the-game-icon). |
| `main` | string | `"src/main.luau"` | The first script to run. It must be a `.luau` or `.lua` file inside the project. |

## The game icon

`icon` can point at an `.ico` file or at any image luv can read: PNG, JPEG, GIF, WebP, BMP, TIFF, TGA, DDS, HDR, EXR, PNM, QOI, farbfeld or SVG.

| Where | What luv does with it |
| --- | --- |
| Windows program | `luv package` puts the icon inside the `.exe`, so it shows in Explorer, on the taskbar and on shortcuts. |
| Linux program | Linux programs cannot hold an icon. `luv package` writes it as `<name>.png` next to the game instead, for desktop shortcuts. |
| Game windows | A window that sets no `Icon` of its own uses the game icon. See [Window.Icon](../reference/window.md#icon). |

- An `.ico` file goes into the program as it is, with all of its sizes.
- Any other image is made square, then luv makes the sizes 16, 24, 32, 48, 128 and 256 from it. An SVG is drawn straight at 256 so it stays sharp.
- When the file does not exist, `luv package` still works, but it prints a warning and the game has no icon.
- When the file is not an image luv can read, `luv package` stops with an error.

## The build table

The whole `[build]` table is optional.

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `output` | string | `"build"` | The folder that `luv test`, `luv build` and `luv package` write into. It is relative to the project root. |

## Rules

- `name` is the only field you must set.
- Unknown fields are ignored.
- There are no fields for native plugins or containers. luv finds those by their folders. See [Native plugins](../manual/native-plugins.md) and [Containers and DLC](../manual/containers.md).

## Errors

| Message | Cause |
| --- | --- |
| `could not find build.toml in ... or any parent directory` | You ran `luv` outside a project. |
| `missing field name` | The `[game]` table has no `name`. |
| ``main script `x` does not exist in ...`` | The `main` path points at a missing file. |
| ``main script `x` must be a .luau or .lua file`` | The `main` path is not a script. |
