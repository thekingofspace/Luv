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
| [FromBytes](#frombytes)(name, data) | [Asset](#asset-object) | no |
| [FromBase64](#frombase64)(name, text) | [Asset](#asset-object) | no |

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

### FromBytes

```luau
Asset.FromBytes(name: string, data: Bytes): Asset
```

Makes an [Asset object](#asset-object) out of bytes you already have, without touching the `assets` folder. `data` is a string or a buffer. This is called sideloading. Use it for a picture you downloaded, unpacked or built yourself.

`name` is not a path and nothing is read from disk. It names the asset and, more importantly, gives it an extension. luv reads the format from the bytes first and falls back to the extension, so give a real one like `avatar.png`.

It errors with `a sideloaded asset needs a name, like 'avatar.png'` for an empty name, and with `the asset data must be a string or buffer` for anything else.

```luau
local Asset = import("Asset")
local Net = import("Net")
local Window = import("Window")

local window = Window.new({ Title = "Sideload" })
local Renderable = window:GetAPI("Renderable")

local reply = Net.Request({ url = "https://example.com/badge.png" })
local badge = Asset.FromBytes("badge.png", reply.body)
Renderable.new("RenderableImage", { Image = badge, Position = udim.new(100, 100) })
```

### FromBase64

```luau
Asset.FromBase64(name: string, text: string): Asset
```

The same as [FromBytes](#frombytes), with the bytes written as base64 text. Spaces and new lines in the text are ignored, and both the plain and the URL safe alphabets work.

A data URL works too. luv takes everything after `;base64,` so you can paste one straight in.

It errors with `the text is not valid base64` when the text does not decode.

```luau
local Asset = import("Asset")

local DOT = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEklEQVR4nGP4z8DwHwyBNBgAAEnICff5q7YNAAAAAElFTkSuQmCC"
local icon = Asset.FromBase64("dot.png", DOT)
print(icon.Size, icon.Extension)

local same = Asset.FromBase64("dot.png", "data:image/png;base64," .. DOT)
print(same.Size)
```

A sideloaded asset is not cached and not shared. Each call makes a new one. See [Caching](#caching).

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
