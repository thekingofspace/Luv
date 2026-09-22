# Editor setup

luv works with any editor. VS Code with the Luau language server gives you the best result, with autocomplete and type checks for the whole engine API.

## VS Code

1. Install the [Luau Language Server](https://marketplace.visualstudio.com/items?itemName=JohnnyMorganz.luau-lsp) extension.
2. Open your project folder in VS Code.

That is all. `luv init` already wrote `.vscode/settings.json` for you:

```json
{
    "luau-lsp.types.definitionFiles": {
        "luv": "./types.d.luau"
    },
    "luau-lsp.require.mode": "relativeToFile",
    "luau-lsp.platform.type": "standard"
}
```

| Setting | What it does |
| --- | --- |
| `luau-lsp.types.definitionFiles` | Loads `types.d.luau`, which describes every luv global and type. |
| `luau-lsp.require.mode` | Makes `require` paths work the same way luv reads them. |
| `luau-lsp.platform.type` | Turns off Roblox globals. |

## The types file

`types.d.luau` gives your editor the globals `import`, `enum`, `udim`, `color`, `EnterParallel` and `ExitParallel`. It also gives you every type name, like `UDim`, `Window` or `SoundNode`, so you can write type annotations.

```luau
local Window = import("Window")

local window: Window = Window.new()
local size: UDim = window.Size
```

The file is never packed into your game. After you update luv, run `luv init` in your project to get the newest types.

## Other editors

Any editor that runs `luau-lsp` can use the same types file. Point its definition file setting at `types.d.luau`.
