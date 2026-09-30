# 0.1.13

## Errors

- A new `Exception` import. `Exception.Raised` fires for every error that has a source, the ones nothing caught, the ones that used to be printed and forgotten, and the ones from other threads. Each one comes with its message, its value, the script and line, the full stack, the thread and whether it was caught.
- `Exception.Traceback`, `Exception.GetStack` and `Exception.Caller` read the stack right where you are. `Exception.Try` works like `pcall` and hands back the full error. `Exception.GetRecent` keeps the last 50 errors, and `Exception.Count` counts them all.
- A new `epcall` global. It is `pcall`, with the same results, except that an error it catches also reaches `Exception.Raised`.
- An error in a parallel thread is now passed back to the main thread, so it shows up in the main thread's `Exception.Raised` too.

## Threads

- A new `Thread` import. `Thread.Running()` is the thread you are on, `Thread.Get` finds threads by state, `Thread.Set` gives yours a state and some data, and `Thread.WaitFor` waits until a thread with a state shows up.
- A thread can `Send` a message to one thread only, and `MarkReady` tells the game it has finished setting up. `WaitReady` waits for that.
- `task.parallel(function ... end, ...)` runs a function on a new thread and passes it values, so a thread can be given its own job, like running the entities of a level.
- `Signal:BindParallel` binds a handler that runs on a thread of its own. Every fire sends its values across, and `UnBind` ends the thread.
- Parallel code can use imports and modules from the top of the script. A Messenger, a module from `require` or any other import that it uses is set up for it on the other thread, so there is nothing to import again.
- A module table can now be sent through a Messenger. The other thread gets its own copy of the same module.
- `task.desynchronize()` and `task.synchronize()` mark a parallel block. `EnterParallel` and `ExitParallel` still work, and your editor marks them as deprecated.

## Scripts

- Headers at the top of a script. `---@start` starts a script on its own, like the main script. `---@startasync` starts it on a thread of its own. `---@boot` runs in every new thread before anything else, and `---@bootready` runs when a thread calls `MarkReady`.
- `---@capture` prints how long a script took to compile, and `---@capture["took %time% to complete"]` picks the words.

## Globals

- A new `global` global. `global.new` adds a global, `global.newImport` adds a name for `import`, and `global.newAPI` adds a name for `window:GetAPI` whose functions get the window first.
- `SetGlobal` still works, and your editor marks it as deprecated and points to `global.new`.

## Registry

- A new `Registry` import for items, mods and anything else kept under ids like `"items.weapons.sword"`.
- Values are frozen. They only change through `Register`, so `Changed` tells everyone about every change. `Stage` and `Commit` apply many changes at once.
- `List` and `GetAll` read a whole group by its prefix.
- `Registry.Safe` makes a registry that every thread shares. A change in one thread fires `Changed` in the others.

## Building

- `luv build` and `luv package` show a progress bar and print a line as scripts, assets and each container finish.
- Plugins in a new `nativeInter` folder are built into the game program itself, so a package can be one file with nothing beside it.
- Everything in a new `export` folder is copied next to the game, folders and all, like a readme or licenses.
- Every `.d.luau` file in the project is now folded into `types.d.luau`, not only the ones in `native`. A file named `types.d.luau` is skipped. The editor settings are updated to leave all of them alone.

## Native plugins

- Plugin API version 3 adds hooks. `on_heartbeat` runs on every heartbeat, `on_frame` on every frame of a window, `on_close` when the game closes and `on_error` for every error that `Exception` sees. `cancel` removes one.
