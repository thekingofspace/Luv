# ContainerLibrary

Inherits: [BaseGameObject](basegameobject.md)

Information about a loaded container.

## Description

You get a `ContainerLibrary` from [Container.LoadLibrary](container.md#loadlibrary) or [Container.GetLibrary](container.md#getlibrary). Each call gives you a new object for the same container.

Destroying the object does not unload the container.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"ContainerLibrary"`. Read only. |
| `Name` | `string` | The name from the container manifest. |
| `Id` | `string` | The file name without `.cont`. Read only. |
| `Version` | `string` | The version from the manifest. Read only. |
| `Description` | `string` | The description from the manifest. Read only. |
| `Main` | `string` | The path of the main script, for example `src/expansion/init.luau`. Read only. |
| `Path` | `string` | The full path of the `.cont` file on disk. Read only. |
| `Natives` | `{ string }` | The file names of the native plugins inside the container, for example `{ "bonus.dll" }`. Read only. |

## Methods

### GetRequire

```luau
library:GetRequire(): string
```

Returns the path that loads the main script with `require`. This is `"@"` followed by the id, for example `"@Expansion"`.

```luau
local Container = import("Container")

local library = Container.LoadLibrary("Expansion")
print(library.Name, library.Version)

local expansion = require(library:GetRequire())
```

### Destroy

```luau
library:Destroy()
```

Marks the object as destroyed. The container stays loaded. See [BaseGameObject](basegameobject.md#destroy).
