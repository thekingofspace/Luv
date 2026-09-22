# Containers and DLC

A container is a folder in your project that luv packs into its own `.cont` file. The game can load it while it runs. Use containers for DLC, mods or large optional parts of a game.

For the full script API, see [Container](../reference/container.md) and [ContainerLibrary](../reference/containerlibrary.md).

## Making a container

Add a folder with a `container.toml` file anywhere in your project. luv finds containers by this file. The name of the folder does not matter.

```tree
my-game/
├── assets/
├── build.toml
├── expansion/
│   ├── assets/
│   │   └── expansion/
│   │       └── levels.txt
│   ├── container.toml
│   ├── native/
│   │   └── bonus.c
│   └── src/
│       └── expansion/
│           ├── helper.luau
│           └── init.luau
├── native/
│   └── luv.h
└── src/
    └── main.luau
```

| Path | What it is |
| --- | --- |
| `expansion/` | The container folder. |
| `expansion/assets/expansion/levels.txt` | An asset of the container. |
| `expansion/container.toml` | The container manifest. See [The container.toml file](#the-container-toml-file). |
| `expansion/native/bonus.c` | A native plugin of the container. See [Native plugins in a container](#native-plugins-in-a-container). |
| `expansion/src/expansion/helper.luau` | A script of the container. |
| `expansion/src/expansion/init.luau` | The main script of the container. |
| `assets/`, `build.toml`, `native/`, `src/` | The game itself. See [Project layout](../start/project-layout.md). |

luv does not look for containers in folders whose names start with a dot, in the build folder or in the `native` folder of the game. It does not look inside a container for more containers.

A container folder is never packed into the game. It becomes its own `.cont` file. luv leaves the same files out of a container as out of a game. See [What gets packed](../start/project-layout.md#what-gets-packed).

## The container.toml file

```toml
[container]
name = "Expansion"
version = "1.2.0"
description = "More levels"
main = "src/expansion/init.luau"
```

| Field | Type | Default | What it does |
| --- | --- | --- | --- |
| `name` | string | required | The container name. The `.cont` file name comes from it. |
| `version` | string | `"0.1.0"` | Shown when you build. Scripts read it from `ContainerLibrary.Version`. |
| `description` | string | `""` | Scripts read it from `ContainerLibrary.Description`. |
| `main` | string | required | The main script. The path is relative to the container folder. It must be a `.luau` or `.lua` file. |
| `natives` | list of strings | `[]` | The native plugins of the container. luv fills this in when it builds, so leave it out. |

Unknown fields are ignored. Each container needs a name that no other container in the project uses.

## Building containers

`luv test`, `luv build` and `luv package` build each container into the build folder. The file name comes from the container name, the same way the game file name comes from the game name. So a container named `Expansion Pack` becomes `build/Expansion-Pack.cont`.

- Scripts turn into bytecode, the same as the scripts of the game.
- luv only rebuilds a `.cont` file when something in the container changed, or when you update luv.
- `luv test` prints a line like `Built container Expansion v1.2.0` when it rebuilds one.
- The game never reads the container folder itself, not even in `luv test`. It always loads the built `.cont` file, just like a packed game.

> [!NOTE]
> luv never deletes old `.cont` files from the build folder. When you rename or remove a container, delete its old `.cont` file too. Otherwise `luv test` still finds it.

## Where the game looks

The game looks for `.cont` files in one folder:

| How the game runs | Folder |
| --- | --- |
| `luv test` | The build folder of the project. |
| `luv run` | The folder of the `.luvit` file. |
| Packed game | The folder of the game program. |

It does not look in subfolders. It scans the folder once when the game starts. Call [Container.Refresh](../reference/container.md#refresh) to scan it again while the game runs.

## Loading a container

```luau
local Container = import("Container")

if Container.Exists("Expansion") then
	local library = Container.LoadLibrary("Expansion")
	local expansion = require(library:GetRequire())
	expansion.start()
end
```

- [Container.LoadLibrary](../reference/container.md#loadlibrary) adds the files of the container to the game. It does not run any script. It yields while it reads the file.
- [GetRequire](../reference/containerlibrary.md#getrequire) returns `"@Expansion"`. Requiring that path runs the main script and returns its value. Like any module, the main script runs only once.
- `require("@Expansion")` also works once the container is loaded.
- A loaded container stays loaded until the game ends.

Names are not case sensitive. You can use the file name without `.cont` or the name from `container.toml`.

## Scripts in a container

When a container loads, its files join the root folder of the game. Every path inside the container works as if the file sat in the game. So `expansion/src/expansion/init.luau` becomes `src/expansion/init.luau`.

Container scripts use `require` like any other script. See [Scripts and modules](scripts.md).

```luau title="expansion/src/expansion/init.luau"
local Asset = import("Asset")
local helper = require("@self/helper")

local expansion = {}

function expansion.start()
	helper.setup()
	print(Asset.LoadString("expansion/levels.txt"))
end

return expansion
```

A container script can also require the scripts of the game. From `src/expansion/init.luau`, `require("./shared")` loads the game script `src/shared.luau`. Your editor only sees the container folder on disk, so it cannot follow these paths.

## Assets in a container

Put the assets of a container in `assets/` inside the container folder. Once the container is loaded, [Asset.Load](../reference/asset.md) finds them like any other asset. Before that, loading them fails.

```luau
local Asset = import("Asset")
local Container = import("Container")

Container.LoadLibrary("Expansion")
print(Asset.LoadString("expansion/levels.txt"))
```

## Native plugins in a container

A container can have its own `native/` folder. It works like the `native/` folder of the game. See [Native plugins](native-plugins.md).

- luv builds these plugins into the build folder, next to the plugins of the game. They are not packed into the `.cont` file.
- `#include "luv.h"` works, because luv also looks for headers in the `native/` folder of the game.
- The container stores the file names of its plugins. Read them from `ContainerLibrary.Natives`.
- Load them with [DLL.Load](../reference/dll.md) like any other plugin.
- No two plugins in the game and its containers can have the same name.

```c title="expansion/native/bonus.c"
#include "luv.h"

LUV_EXPORT int bonus_value(void) {
    return 42;
}
```

A container script loads it like this:

```luau
local DLL = import("DLL")

local bonus = DLL.Load("./bonus")
local value = bonus:GetFunction("bonus_value", "int")
print(value())
```

## The path clash rule

A loaded container shares the root folder of the game. So a container file cannot use a path that the game or another container already uses. luv checks this when it builds. A clash stops the build with an error like this:

```text
error: containers share the game's root folder, so their files cannot use paths the game or another container already uses, give each container its own folders such as src/<container>/ and assets/<container>/:
  src/main.luau is in both the game and container Clash
  assets/base.txt is in both the game and container Clash
```

Give each container its own folders to avoid clashes:

| Folder | Holds |
| --- | --- |
| `src/<name>/` | The scripts of the container. |
| `assets/<name>/` | The assets of the container. |

Both folders go inside the container folder. The example at the top of this page uses this layout.

A `.cont` file made in another project is not checked. If its paths clash anyway, the file of the game wins. Between two containers, the one that loaded first wins.

## Shipping DLC separately

`luv package` copies every `.cont` file and every native plugin next to the game program. So the base game ships with all of its containers by default. See [Shipping your game](shipping.md).

To ship a container on its own:

1. Run `luv package`.
2. Move the `.cont` file of the container out of `build/package/`.
3. Also move out the native plugins built from the `native/` folder of the container.
4. Ship what is left in `build/package/` as the base game.
5. Ship the files you moved out as the DLC.

Players put the DLC files next to the game program. The game finds them the next time it starts. It can also find them without a restart with [Container.Refresh](../reference/container.md#refresh).

`luv package` clears `build/package/` every time. Repeat these steps after each package.

## Errors

| Message | Cause |
| --- | --- |
| ``the main script `x` of container Expansion does not exist in ...`` | The `main` path points at a missing file. |
| ``the main script `x` of container Expansion must be a .luau or .lua file`` | The `main` path is not a script. |
| `the containers in a and b are both named Expansion, container names must be unique` | Two containers use the same name. |
| `... and ... both produce a native library named bonus.dll` | Two native plugins have the same name. |
| `no container named Expansion was found next to the game` | `LoadLibrary` got a name that is not in the folder the game looks in. |

A missing `name` or `main` field stops the build with a `missing field` error.
