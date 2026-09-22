# Container

Finds and loads containers. A container is a `.cont` file that adds scripts, assets and native plugins to a game, like DLC or a mod.

```luau
local Container = import("Container")
```

## Description

luv looks for `.cont` files in one folder when the game starts:

| How the game runs | Folder |
| --- | --- |
| `luv test` | The `build` folder of the project. |
| `luv run` | The folder of the `.luvit` file. |
| Packed game | The folder of the game program. |

A name can be the file name without `.cont`, or the name from the container manifest. Names are not case sensitive. So `"Expansion Pack"` finds `Expansion-Pack.cont`.

Loading a container adds its files to the game. It does not run anything. You run its main script with `require`. A loaded container stays loaded until the game ends.

To make a container, see [Containers and DLC](../manual/containers.md).

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [Exists](#exists)(name) | `boolean` | no |
| [IsLoaded](#isloaded)(name) | `boolean` | no |
| [GetContainers](#getcontainers)() | `{ string }` | no |
| [GetLibrary](#getlibrary)(name) | [ContainerLibrary](containerlibrary.md)`?` | no |
| [LoadLibrary](#loadlibrary)(name) | [ContainerLibrary](containerlibrary.md) | yes |
| [Refresh](#refresh)() | `{ string }` | yes |

## Function descriptions

### Exists

```luau
Container.Exists(name: string): boolean
```

Returns `true` if a container with this name was found in the last scan.

### IsLoaded

```luau
Container.IsLoaded(name: string): boolean
```

Returns `true` if the container is loaded.

### GetContainers

```luau
Container.GetContainers(): { string }
```

Returns the id of every container found in the last scan. The id is the file name without `.cont`.

### GetLibrary

```luau
Container.GetLibrary(name: string): ContainerLibrary?
```

Returns the [ContainerLibrary](containerlibrary.md) of a loaded container. Returns `nil` if it is not loaded.

### LoadLibrary

```luau
Container.LoadLibrary(name: string): ContainerLibrary
```

Loads a container and returns its [ContainerLibrary](containerlibrary.md). This yields the calling coroutine while the file is read. Calling it again for a loaded container is fine and returns right away.

After this call:

- `require` can load the container scripts.
- `Asset` can load the container assets.

Files of the game win over files of a container with the same path. Native plugins of a container sit next to the game, so [DLL.Load](dll.md#load) finds them by name.

It errors when:

- No container has this name. The message is `no container named <name> was found next to the game`.
- The file is not a valid container.
- The main script listed in the manifest is missing.

```luau
local Container = import("Container")

if Container.Exists("Expansion") then
	local library = Container.LoadLibrary("Expansion")
	local expansion = require(library:GetRequire())
	expansion.start()
end
```

### Refresh

```luau
Container.Refresh(): { string }
```

Scans the folder again and returns the id of every container found. Use it when new files may have been added while the game runs. Loaded containers stay loaded. This yields the calling coroutine.

```luau
local Container = import("Container")

for _, id in Container.Refresh() do
	if not Container.IsLoaded(id) then
		print("New content:", id)
	end
end
```
