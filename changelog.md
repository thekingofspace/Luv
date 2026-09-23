# 0.1.5

The first release of luv.

## Added

- Containers get an alias in `.luaurc`. luv writes them when you test, build or package.
- `luv luaurc` updates that file on its own. `luv aliases` is the same command.
- Set `aliases = false` in the `[build]` table of `build.toml` to turn it off.

## Faster

- Higher frame rates all round. A scene of 5000 moving objects went from 125 to 178 frames a second.
- Steadier frame timing. A cap of 60 or 144 now lands on target.
- Setting a value that is already there costs nothing.
- Reading a property is about twice as fast.

## Fixed

- Closing a window lets go of the handlers bound to it.
- Destroying a Callback keeps its address safe for plugins that still hold it.
- Destroying a socket closes it right away. `IsOpen` turns false and sending raises an error.
- Destroying a Child closes its three pipes.
- Destroying a Library lets go of its Exports table.
