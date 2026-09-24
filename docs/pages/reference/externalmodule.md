# ExternalModule

Inherits: [BaseGameObject](basegameobject.md)

A folder of Luau and files from outside the game, compiled and mounted. [ecall](globals.md#ecall) makes one.

## Description

`ecall` reads a folder from disk, turns every Luau file in it into bytecode, keeps every other file as it is, and mounts the whole thing in the files of the game under `mods`. Nothing runs yet. [Fetch](#fetch) runs `init.luau` and gives you whatever it returned.

The folder itself is never copied. luv holds the bytecode and the bytes in memory, so the game folder is untouched and nothing is written anywhere.

A folder called `hat` is mounted at `mods/hat`. That is a real path in the files of the game, so the mod can require its own scripts, read its own files and load its own pictures and sounds.

```tree
mods/
└── hat/
    ├── init.luau
    ├── helper.luau
    ├── assets/
    │   └── icon.png
    └── parts/
        └── brim.luau
```

| Path | What it is |
| --- | --- |
| `init.luau` | The one file `ecall` runs. A folder without it is refused. |
| `helper.luau` | Another script. `require("@self/helper")` reaches it. |
| `assets/icon.png` | A picture. `Asset.Load("icon.png")` finds it. |
| `parts/brim.luau` | A script in a subfolder. `require("@self/parts/brim")` reaches it. |

```luau
local hat = ecall("mods/hat")
print(hat.Folder, hat.Files)
local api = hat:Fetch()
```

A single `.luau` file works too. It is mounted on its own and becomes the entry.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"ExternalModule"`. Read only. |
| `Name` | `string` | The name of the mounted folder. For `mods/hat` it is `hat`. |
| `Source` | `string` | The folder on disk that was read. Read only. |
| `Folder` | `string` | Where it is mounted in the files of the game, like `mods/hat`. Read only. |
| `Entry` | `string` | The script `Fetch` runs, like `mods/hat/init.luau`. Read only. |
| `Files` | `number` | How many files were mounted. Read only. |
| `IsLoaded` | `boolean` | `true` once the entry has run and its value is kept. Read only. |

## Methods

### Fetch

```luau
handle:Fetch(): any
```

Runs the entry script and returns what it returned. This yields the calling coroutine while it runs, so the mod can use anything the rest of the game can.

The mod is handed its own folder as the first value of `...`, so it always knows where it lives.

luv keeps the value against the entry. So the second Fetch does not run it again, and an `ecall` of the same folder anywhere else gets the same value.

An error inside the mod comes back out of Fetch.

```luau
local hat = ecall("mods/hat")
print(hat:Fetch() == hat:Fetch())
```

### GetFiles

```luau
handle:GetFiles(): { string }
```

Every file the mod mounted, by its path in the files of the game. Useful for seeing what a mod brought with it.

```luau
for _, path in ecall("mods/hat"):GetFiles() do
	print(path)
end
```

### Drop

```luau
handle:Drop(): boolean
```

Forgets the kept value. Returns `true` when there was one, and `false` when there was nothing to forget. The next Fetch runs the entry again from the start.

The files stay mounted. Drop only forgets the value, and it does not reach into anything that already has it. See [Dropping is not unloading](#dropping-is-not-unloading).

## What a mod can reach

Because the folder is mounted, a mod works like any other part of the game.

| What you want | How |
| --- | --- |
| Another script in the mod | `require("@self/helper")` |
| A script in a subfolder | `require("@self/parts/brim")` |
| A file of the mod, as bytes | `FS.readFile("@self/notes.txt")` |
| A picture or sound of the mod | `Asset.Load("icon.png")` |
| A library of the engine | `import("Net")` |
| Another mod | `ecall("mods/other")` |

```luau title="mods/hat/init.luau"
local folder = ...

local Asset = import("Asset")
local FS = import("FS")

local brim = require("@self/parts/brim")

return {
	folder = folder,
	icon = Asset.Load("icon.png"),
	notes = FS.readFile("@self/notes.txt"),
	brim = brim,
}
```

`@self` is the folder of the mod. Use it rather than `./`, because inside an `init.luau` a path that starts with `./` means the folder holding the mod, not the mod itself. That is the same rule the rest of the game follows.

## Sideloading assets

A mod that carries its own pictures and sounds needs nothing special. The folder is mounted, so [Asset.Load](asset.md#load) finds them.

luv looks in three places, in this order:

| Order | Where |
| --- | --- |
| 1 | The `assets` folder of the mod, like `mods/hat/assets/icon.png`. |
| 2 | The folder of the mod, like `mods/hat/icon.png`. |
| 3 | The `assets` folder of the game, like `assets/icon.png`. |

So a mod can carry an `icon.png` of its own and it wins over one the game already has. Leave the extension off and luv finds it either way, the same as anywhere else.

```luau title="mods/hat/init.luau"
local Asset = import("Asset")

return {
	icon = Asset.Load("icon.png"),
	chime = Asset.Load("sounds/chime"),
}
```

Every kind of asset works this way, not only pictures. An [Asset](asset.md#asset-object) is a name and some bytes, and a [RenderableImage](renderableimage.md), a [RenderableText](renderabletext.md) font, a [SoundNode](soundnode.md), a [Shader](shader-library.md#compile) and the icon of a [Window](window.md) all take one.

### Assets from bytes

Sideloading is for bytes that are not in the mounted folder at all. A picture you downloaded, one a plugin built, or one written as base64. [Asset.FromBytes](asset.md#frombytes) turns bytes into an asset with no file behind it.

| Where the bytes come from | How to read them |
| --- | --- |
| A path on disk outside the mod | [FS.readFile](fs.md#readfile) with the full path. |
| A folder of the user, like documents or the save folder | [Process.dirs](process.md#dirs) for the folder, then `FS.readFile`. |
| The internet | [Net.Request](net.md#request) and the body of the reply. |
| Base64 text, or a data URL | [Asset.FromBase64](asset.md#frombase64) on its own. |
| A native plugin | [push_asset](native-c.md#sideloading-assets) from C or Rust. |

```luau
local Asset = import("Asset")
local Net = import("Net")

local reply = Net.Request({ url = "https://example.com/banner.png" })
local banner = Asset.FromBytes("banner.png", reply.body)
```

Give the name a real extension. luv reads the format from the bytes first and falls back to the name, so `banner.png` lands in the right decoder.

An asset made from bytes is not cached and not shared. Each call makes a new one. See [Caching](asset.md#caching).

## Dropping is not unloading

Drop only takes the value out of the cache. Whoever already holds it keeps it, and the mod stays in memory until every one of them lets go. The files stay mounted either way.

That matters when mods lean on each other. Say `ui` fetched `theme` and kept it:

```tree
mods/
├── theme/
│   └── init.luau
└── ui/
    └── init.luau
```

| Mod | What it holds |
| --- | --- |
| `theme` | A table of colors. |
| `ui` | The table from `theme`, kept in a local. |

Dropping `theme` now leaves you with this:

- `ui` still reads the old table, because it has it.
- The next Fetch of `theme` runs the entry again and makes a second table.
- Two tables are alive at once, and changing one does not change the other.

So drop a mod only when nothing else is holding it. Drop the mods that depend on it first, then drop it. When you are not sure what holds what, leave it cached.

```luau
local theme = ecall("mods/theme")
local ui = ecall("mods/ui")

ui:Drop()
theme:Drop()
```

## Errors

| Message | Cause |
| --- | --- |
| `ecall needs the path of a folder or a Luau file` | The path is empty or only spaces. |
| `cannot read <path>: there is no folder or file there` | Nothing is at that path. |
| `<path> has no init.luau` | The folder has no entry script. |
| `<path> is not a Luau file` | The path is a file that does not end in `.luau` or `.lua`. |
| A syntax error with the path in front | A script in the folder does not compile. |
| `a mod can hold at most 4096 files` | The folder holds too many files. |
| `that folder is too large to load` | The files add up to more than 512 MiB. |
