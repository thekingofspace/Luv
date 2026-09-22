# Files and saving

This page shows how to load assets, read the files of your game and save data on the computer of the player.

## Loading assets

Put images, sounds, fonts and data files in the `assets` folder of your project. Load them with [Asset](../reference/asset.md). Paths start inside `assets`, and the extension is optional.

```luau
local Asset = import("Asset")

local icon = Asset.Load("gui/icon")
local intro = Asset.LoadString("text/intro.txt")
print(icon.Size, #intro)
```

`Asset.Load` gives an [Asset object](../reference/asset.md#asset-object). Many parts of luv take one directly, like images, fonts and sounds. `Asset.LoadString` gives the bytes as a string.

## Reading game files

[FS](../reference/fs.md) reads any file of your game, not only assets. Start the path with `./` or `../`. The path starts from the folder of the script, like `require` does.

```tree
my-game/
├── data/
│   └── levels.json
└── src/
    └── main.luau
```

| Path | What it is |
| --- | --- |
| `data/levels.json` | A data file. `src/main.luau` reads it as `"../data/levels.json"`. |
| `src/main.luau` | The script that reads the data file. |

```luau
local FS = import("FS")
local Serde = import("Serde")

local levels = Serde.Decode("json", FS.readFile("../data/levels.json"))
print(#levels)
for _, name in FS.readDir("../data") do
	print(name)
end
```

The same paths work in a packed game. Aliases from `.luaurc` work too, like `"@Data/levels.json"`. See [Scripts and modules](scripts.md).

Game files are read only. Writing to a `./` path is an error. Scripts cannot be read with FS. Load them with `require`.

## Saving data

Save files in `Process.dirs.save`. It is a folder named after your game inside the app data folder of the user. On Windows that is `C:\Users\<name>\AppData\Roaming\<game>`. luv does not create it, so call `FS.makeDir` first.

```luau
local FS = import("FS")
local Process = import("Process")

local folder = Process.dirs.save
if folder then
	FS.makeDir(folder)
	FS.writeFile(folder .. "/settings.txt", "volume=80")
	print(FS.readFile(folder .. "/settings.txt"))
end
```

Do not save into the game folder. An installed game often cannot write there.

A path with no `./`, `../` or `@` in front is a normal path on disk. A short one like `"save.txt"` starts from the working folder of the program, not from the game. Always build save paths from `Process.dirs`. See [Dirs](../reference/process.md#dirs).

## Saving tables as JSON

[Serde](../reference/serde.md) turns a table into JSON and back. TOML and YAML work the same way.

```luau
local FS = import("FS")
local Process = import("Process")
local Serde = import("Serde")

local folder = Process.dirs.save
if folder then
	local path = folder .. "/save.json"
	FS.makeDir(folder)
	FS.writeFile(path, Serde.Encode("json", { level = 3, coins = 120 }, true))
	local save = Serde.Decode("json", FS.readFile(path))
	print(save.level, save.coins)
end
```

Check that a save is there before you read it:

```luau
local FS = import("FS")
local Process = import("Process")
local Serde = import("Serde")

local function loadSave(): { [string]: any }
	local folder = Process.dirs.save
	if folder and FS.exists(folder .. "/save.json") then
		return Serde.Decode("json", FS.readFile(folder .. "/save.json"))
	end
	return { level = 1, coins = 0 }
end
```

A few things to know:

- An empty table saves as `[]`.
- UDim and Color values save as tables with their fields. Build them again after loading.
- Strings must be text. Turn raw bytes into text with [Crypto.ToBase64](../reference/crypto.md#tobase64) first.

## Saving when the game closes

[BindToClose](../reference/process.md#bindtoclose) runs a function when the game closes. The game waits for it, up to 30 seconds.

```luau
local FS = import("FS")
local Process = import("Process")
local Serde = import("Serde")

local state = { level = 1, coins = 0 }
Process.BindToClose(function()
	local folder = Process.dirs.save
	if folder then
		FS.makeDir(folder)
		FS.writeFile(folder .. "/save.json", Serde.Encode("json", state))
	end
end)
```

## Streaming with File

For big files or line by line work, open a [File](../reference/file.md) with [FS.open](../reference/fs.md#open). It works like `io.open` in Lua.

```luau
local FS = import("FS")
local Process = import("Process")

local path = Process.dirs.temp .. "/log.txt"
local file, message = FS.open(path, "a")
if not file then
	error(message)
end
file:write("started at ", os.time(), "\n")
file:close()

for line in FS.lines(path) do
	print(line)
end
```

`FS.open` returns `nil` and a message when it fails. The other FS functions throw an error. Catch it with `pcall` when a file might be missing.

## Nothing blocks the game

Reading and writing yield only the calling coroutine. The rest of the game keeps running while a big file loads. See [Yielding and coroutines](yielding.md).
