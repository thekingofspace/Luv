# Asset

Loads files from the `assets` folder of the game.

```luau
local Asset = import("Asset")
```

## Description

Asset paths start inside the `assets` folder in the root of the project. They are the same no matter which script makes the call. Do not put `assets/` in front. `Asset.Load("gui/icon.png")` loads `assets/gui/icon.png`.

This works the same in `luv test` and in a packed game. Loaded containers can add assets too. See [Container](container.md).

Path rules:

- Use `/` or `\` between folder names.
- A path cannot go above the `assets` folder.
- Aliases like `@Data` do not work here.
- Scripts cannot be loaded as assets. Use `require` for them.

Other parts of luv take an Asset object in place of a path, for example images, fonts, shaders and sounds.

## Leaving out the extension

When no file has the exact name, luv looks in the same folder for files with that name and any extension. Only the last extension is ignored. If one file matches, it loads that file. If more than one file matches, the call errors. Add the extension to pick one.

```tree
assets/
├── notes.txt
├── gui/
│   ├── icon.png
│   └── menu.json
└── sounds/
    ├── click.ogg
    └── click.wav
```

| File | Paths that load it |
| --- | --- |
| `notes.txt` | `"notes.txt"` or `"notes"` |
| `gui/icon.png` | `"gui/icon.png"` or `"gui/icon"` |
| `gui/menu.json` | `"gui/menu.json"` or `"gui/menu"` |
| `sounds/click.ogg` | Only `"sounds/click.ogg"`. `"sounds/click"` matches two files, so it errors. |
| `sounds/click.wav` | Only `"sounds/click.wav"`. |

## Caching

luv keeps one copy of each asset in memory while something still uses it, like an Asset object. Loading the same file again during that time reuses the copy. It does not read the file again. When nothing uses the asset anymore, the memory is freed.

## Errors

Every error message starts with `cannot load asset '<path>': `. The rest of the message tells you what went wrong.

| Message | Cause |
| --- | --- |
| `asset paths must name a file inside the assets folder` | The path is empty or goes above the `assets` folder. |
| `no such asset` | No file matches the path. |
| `the name is ambiguous, it matches ...` | The path has no extension and more than one file matches. The message lists them. |
| `scripts can only be loaded with require` | The file is a `.luau` or `.lua` script. |

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [Load](#load)(path) | [Asset](#asset-object) | yes |
| [LoadString](#loadstring)(path) | `string` | yes |

## Function descriptions

### Load

```luau
Asset.Load(path: string): Asset
```

Loads a file and returns an [Asset object](#asset-object). This yields the calling coroutine while the file is read. It errors when the file cannot be loaded. See [Errors](#errors).

```luau
local Asset = import("Asset")

local icon = Asset.Load("gui/icon")
print(icon.Name, icon.Path, icon.Size, icon.Extension)
```

### LoadString

```luau
Asset.LoadString(path: string): string
```

Loads a file and returns all of its bytes as a string. It works for text files and for binary files. This yields the calling coroutine. It errors the same way as [Load](#load).

```luau
local Asset = import("Asset")
local Serde = import("Serde")

local menu = Serde.Decode("json", Asset.LoadString("gui/menu"))
print(menu.title)
```

## Asset object

Inherits: [BaseGameObject](basegameobject.md)

One file loaded with [Asset.Load](#load).

The object keeps the bytes of the file in memory until it is destroyed or garbage collected.

### Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"Asset"`. Read only. |
| `Name` | `string` | The file name, for example `"icon.png"`. |
| `Path` | `string` | The path inside the `assets` folder with the real extension, for example `"gui/icon.png"`. Read only. |
| `Size` | `number` | The size in bytes. It is `0` after the asset is destroyed. Read only. |
| `Extension` | `string?` | The extension without the dot, for example `"png"`. It is `nil` when the file name has no dot. Read only. |

### Methods

#### ReadString

```luau
asset:ReadString(): string
```

Returns all bytes of the file as a string. It does not yield. It errors after the asset is destroyed.

#### ReadBuffer

```luau
asset:ReadBuffer(): buffer
```

Returns all bytes of the file in a new buffer. It does not yield. It errors after the asset is destroyed.

#### Open

```luau
asset:Open(): File
```

Returns a read only [File](file.md) for the bytes of the asset. The File is in text mode. Each call gives a new File with its own position. You can read and seek. Writing returns `nil` and `the game's files are read-only`. It does not yield. It errors after the asset is destroyed.

```luau
local Asset = import("Asset")

local notes = Asset.Load("notes.txt"):Open()
for line in notes:lines() do
	print(line)
end
notes:close()
```

#### Destroy

```luau
asset:Destroy()
```

Releases the bytes of the asset. After this, `ReadString`, `ReadBuffer` and `Open` error with `Asset '<Name>' has been destroyed`, and `Size` is `0`. Files you opened before keep working. See [BaseGameObject](basegameobject.md#destroy).
