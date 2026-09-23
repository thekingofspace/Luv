# Scripts and modules

Your game is a set of Luau scripts. This page shows how luv starts them and how scripts load each other.

## The main script

When the game starts, luv runs the script set by `main` in [build.toml](../start/build-toml.md). The default is `src/main.luau`. Other scripts only run when a script loads them with `require`.

The main script runs from top to bottom. It can call functions that yield, like [Asset.Load](../reference/asset.md), right at the top level.

Scripts can sit in any folder of the project. See [Where scripts can live](../start/project-layout.md#where-scripts-can-live). Files ending in `.luau` and `.lua` both work. Files ending in `.d.luau` only hold types. luv never runs them.

## Engine libraries

Use the global `import` to get an engine library by its name.

```luau
local Asset = import("Asset")
local Window = import("Window")
```

An unknown name raises an error. The message lists every name you can use. The globals `enum`, `udim` and `color` need no import. See [Globals](../reference/globals.md).

## Modules

Any script can be a module. A module returns one value, usually a table.

```luau title="src/util.luau"
local util = {}

function util.double(value: number): number
	return value * 2
end

return util
```

Another script loads it with `require` and a path:

```luau title="src/main.luau"
local util = require("./util")

print(util.double(21))
```

## Require paths

The examples on the rest of this page use this project:

```tree
my-game/
├── .luaurc
├── build.toml
├── lib/
│   └── answer.luau
└── src/
    ├── main.luau
    ├── shared/
    │   ├── deep.luau
    │   ├── helper.luau
    │   └── init.luau
    └── util.luau
```

| Path | What it is |
| --- | --- |
| `.luaurc` | Defines the alias `@lib`. See [Aliases](#aliases). |
| `build.toml` | The project file. |
| `lib/answer.luau` | A module outside `src`. |
| `src/main.luau` | The main script. |
| `src/shared/` | A folder that works as one module, because it has an `init.luau` file. |
| `src/shared/deep.luau` | A module inside the folder. |
| `src/shared/helper.luau` | A module inside the folder. |
| `src/shared/init.luau` | The file that runs when a script requires the `shared` folder. |
| `src/util.luau` | A module. |

A path must start with `./`, `../` or `@`:

| Path | Points at | Example |
| --- | --- | --- |
| `./name` | `name` in the folder of the current script. | `./util` in `src/main.luau` loads `src/util.luau`. |
| `../name` | `name` in the parent folder. | `../util` in `src/shared/deep.luau` loads `src/util.luau`. |
| `@self/name` | `name` inside the current module. | `@self/helper` in `src/shared/init.luau` loads `src/shared/helper.luau`. |
| `@alias/name` | `name` inside the folder of an alias. | `@lib/answer` loads `lib/answer.luau`. |

Use `/` between folder names. A path cannot point outside the project.

## File extensions

Leave the extension out of the path. For `require("./util")`, luv looks for these files:

1. `util.luau`
2. `util.lua`
3. `util/init.luau`
4. `util/init.lua`

Exactly one of them must exist. If two exist, `require` fails because the path is ambiguous.

You can also write the extension. `require("./util.luau")` loads the same module as `require("./util")`.

## Folders as modules

A folder with an `init.luau` file works as one module. `require("./shared")` in `src/main.luau` runs `src/shared/init.luau`.

Inside `init.luau`, the module is the folder itself. This changes two paths:

- `@self/helper` points inside the folder, at `src/shared/helper.luau`.
- `./util` points next to the folder, at `src/util.luau`.

```luau title="src/shared/init.luau"
local helper = require("@self/helper")
local util = require("./util")

return {
	helper = helper,
	double = util.double,
}
```

In any other file, `@self` points at a folder with the same name as the file. For `src/main.luau`, `@self/name` looks in `src/main/`.

## Aliases

An alias gives a folder a short name. You define aliases in a `.luaurc` file or a `.config.luau` file. These two files do the same thing:

```json title=".luaurc"
{
    "aliases": {
        "lib": "./lib"
    }
}
```

```luau title=".config.luau"
return {
	luau = {
		aliases = {
			lib = "./lib",
		},
	},
}
```

Any script can now use the alias:

```luau
local answer = require("@lib/answer")

print(answer)
```

- An alias path is relative to the folder of the config file.
- luv looks for the alias starting in the folder that `./` points at. It then goes up one folder at a time until it finds a config file with the alias.
- Put the config file in the project root so every script can use it.
- Alias names are not case sensitive. `@Lib` and `@lib` are the same.
- Alias names can use letters, digits, `-`, `_` and `.`.
- An alias can point at another alias, for example `"@lib/tools"`.
- `.luaurc` can have `//` comments and trailing commas.
- luv only uses the `aliases` from these files.
- Both files are packed into the game, so aliases still work after you build.
- The same aliases work in [FS](../reference/fs.md) paths.
- A loaded container adds an alias like `@Expansion`. See [Containers and DLC](containers.md).

## Aliases luv writes for you

Every container gets an alias, so your editor can follow `require("@Expansion")`. luv keeps them up to date each time you run `luv test`, `luv build` or `luv package`. Run [luv luaurc](../start/command-line.md#luv-luaurc) to update the file on its own without building.

- With no `.luaurc` in the project root, luv makes one.
- With a `.luaurc` already there, luv adds its aliases to the ones you wrote.
- The alias name is the container file name without `.cont`, and it points at the container main script.

luv also writes the list of aliases it added to `build/aliases.json`:

```json title="build/aliases.json"
{
    "aliases": {
        "Expansion": "./expansion/src/expansion"
    }
}
```

That list is how luv knows which entries are its own. When you delete a container, the next build removes only that alias and leaves everything else in your `.luaurc` alone.

- Change the value of an alias yourself and luv stops touching it.
- Rewriting the file drops any comments it had. luv only rewrites it when an alias really changes.
- A project with a `.config.luau` and no `.luaurc` is left alone, so the two files cannot fight. luv prints a note.
- Set `aliases = false` in the `[build]` table of [build.toml](../start/build-toml.md) to turn this off.

## Module caching

Each module runs only once. Every later `require` of the same file returns the same value, even when you write the path in another way.

```luau
local first = require("./util")
local second = require("./util.luau")

print(first == second)
```

This prints `true`.

A module can yield while it runs, for example to load an asset. The script that called `require` waits until the module returns.

## Case in file names

> [!WARNING]
> Paths in a packed game are case sensitive. On Windows, `luv test` reads files from the disk, where case does not matter. So `require("./Util")` can work in `luv test` and then fail after `luv build`. Always write paths with the same case as the file names. This also goes for asset paths.

## Scripts become bytecode

`luv build` and `luv package` compile every script into Luau bytecode. The packed game holds no source code.

- `luv build` compiles every script, even the ones that nothing requires. If a script has a syntax error, luv lists every error with its path and line. Then it stops without writing the game.
- `luv test` runs scripts from their source. It compiles each script the first time something loads it. So `luv test` misses syntax errors in scripts that never load.
- Error messages keep the script path and the line number in both cases, for example `src/main.luau:2: boom`.

You cannot read a game script as a file. [Asset](../reference/asset.md) and [FS](../reference/fs.md) refuse script files inside the game with the error `scripts can only be loaded with require`.

## Errors

| Message | Cause |
| --- | --- |
| `error requiring module "util": require path must start with a valid prefix: ./, ../, or @` | The path does not start with `./`, `../` or `@`. |
| `error requiring module "./missing": could not resolve child component "missing"` | No file matches the path. |
| `error requiring module "./util": could not resolve child component "util" (ambiguous)` | More than one file matches, like `util.luau` and `util.lua`. |
| `no module present at resolved path` | The path points at a folder with no `init.luau` file. |
| `error requiring module "@nope/x": @nope is not a valid alias` | No config file defines the alias. |
| `module must return a single value` | The module returned more than one value. |
| `'Sound' cannot be imported, the available imports are ...` | `import` got a name that does not exist. |
