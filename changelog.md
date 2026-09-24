# 1.7

## Raycasts and queries

- Raycasts and queries follow the shape you drew, even when it caves in.
- A RenderableShape can take its own outline of points.
- A RenderableImage can skip its clear pixels in queries, one frame of a sheet at a time.
- The CPU and the GPU now run the same test, so both paths give the same answer.

## Plugins

- Plugins can add a service. It becomes a name for `import`, with its own functions and values.
- Plugins can put a `.d.luau` file in `native/`. luv folds every one into `types.d.luau`.
- `luv types` rebuilds that file on its own. `luv typegen` is the same command.
- A container can ship a type file the same way.
- Plugins can read and write engine objects, call their methods and make new ones.
- Plugins can reach a window and its APIs, so they can make a renderable and hand it to Luau.
- Plugins can make signals, bind their own handlers to one, and fire one from any thread.
- Plugins can hand Luau a function of their own, with data of their own behind it.
- Plugins can set and read globals, and make tables.
- Plugins can push a buffer, so they can make sound and hand it straight over.
- Plugins can run code on a timer on the game thread, and cancel it later.
- `Library:GetServices` lists the services a library added.
- A static property with a setter can now be written from Luau.
