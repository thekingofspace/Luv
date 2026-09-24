# Editor setup

luv works with any editor. VS Code with the Luau language server gives you the best result, with autocomplete and type checks for the whole engine API.

The types need the new Luau type solver, which is still in beta. luv turns it on for you. See [The new type solver](#the-new-type-solver).

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
    "luau-lsp.platform.type": "standard",
    "luau-lsp.fflags.enableNewSolver": true
}
```

| Setting | What it does |
| --- | --- |
| `luau-lsp.types.definitionFiles` | Loads `types.d.luau`, which describes every luv global and type. |
| `luau-lsp.require.mode` | Makes `require` paths work the same way luv reads them. |
| `luau-lsp.platform.type` | Turns off Roblox globals. |
| `luau-lsp.fflags.enableNewSolver` | Turns on the new type solver. The types do not work without it. |

## The new type solver

The luv types are built for the new Luau type solver. It is in beta, and the language server has it off until you ask for it. `luau-lsp.fflags.enableNewSolver` is what asks for it.

With the old solver the types file does not load at all. It uses `keyof` and `index`, which the old solver does not have. Every luv name then looks undefined, from `import`, `udim` and `color` to type names like `UDim`, `Window` and `Shader`.

`luv init` writes the setting for you. If your project came from an older luv, run `luv init` again in it, or add the line to `.vscode/settings.json` yourself. Restart the language server after you change it.

On the command line the same thing is a flag:

```shell
luau-lsp analyze --flag:LuauSolverV2=true --definitions=types.d.luau src
```

## The types file

`types.d.luau` gives your editor the globals `import`, `enum`, `udim`, `color`, `EnterParallel` and `ExitParallel`. It also gives you every type name, like `UDim`, `Window` or `SoundNode`, so you can write type annotations.

```luau
local Window = import("Window")

local window: Window = Window.new()
local size: UDim = window.Size
```

The file is never packed into your game. After you update luv, run `luv init` in your project to get the newest types.

### How luv builds it

luv writes the whole file, so do not edit it. It starts from the engine types that came with your version of luv, then folds in every `.d.luau` file it finds in a `native` folder, in your project and in each container.

`luv init`, `luv test` and `luv build` all build it again from nothing. So a type file you add shows up, and a type file you delete leaves nothing behind. [luv types](command-line.md#luv-types) does only this step.

A plugin type file adds its own types and can add names to `import` and to `window:GetAPI`. See [Type files](../manual/native-plugins.md#type-files).

In the file luv writes, the added names sit at the end of `Imports` and `WindowAPIs` under a line that says which file they came from, and the added types sit at the end of the file between two lines that name the file.

```text
export type Imports = {
	Asset: Asset_API,
	Window: Window_API,
	-- from native/physics.d.luau
	Physics: Physics_API,
}
```

## Other editors

Any editor that runs `luau-lsp` can use the same types file. Point its definition file setting at `types.d.luau`, and turn the new type solver on the same way. In a plain `luau-lsp` server that is the `--flag:LuauSolverV2=true` argument.
