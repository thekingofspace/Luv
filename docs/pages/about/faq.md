# Frequently asked questions

## What is luv?

luv is a game engine for 2D games written in Luau. You write scripts, luv runs them. See the [Introduction](introduction.md).

## How is luv different from love2d?

Both give you a small set of building blocks instead of an editor. luv is built for Luau, so you get:

- Types and autocomplete for the whole engine in your editor.
- Windows, signals and objects instead of callback functions like `love.update`.
- Calls that wait, like loading a file or a web request, only pause the script that made them. The rest of the game keeps running.
- Native plugins in C or Rust that can add whole classes to Luau.
- One command that packs the game into a single program.

## How is luv different from Ruzit?

luv is the next step after [Ruzit](https://github.com/thekingofspace/Ruzit), my older Luau engine. It is a full rewrite with up to date libraries. It is safer with memory and threads, and it has far more features.

## Does luv support 3D?

Not out of the box. luv has no 3D camera, no 3D models and no 3D physics.

You can still add 3D yourself. Custom renderables run your own shaders, and native plugins can feed them data each frame. The `cube` examples draw a spinning 3D cube this way. See [Adding 3D](../manual/3d.md).

## Does luv have physics?

There is no physics engine built in. luv can find renderables at a point, in an area or along a ray, which covers simple collisions. For more, write your own physics in Luau or load a physics library through a [native plugin](../manual/native-plugins.md).

## Is there a UI system?

There are no ready made buttons or menus. You build them from shapes, images and text, and read the mouse and touch input yourself.

## Can I use my Roblox code?

The Luau language is the same, so plain Luau code works. Roblox APIs like `game`, `workspace` or `Instance` do not exist in luv.

## Which platforms can I ship to?

Windows and Linux. See [System requirements](requirements.md).

## Why does a call pause my script?

Some calls wait for something, like a file, a network reply or a native function. They pause only the coroutine that called them. The window, the frame signals and every other script keep running. See [Yielding and coroutines](../manual/yielding.md).

## Where do save files go?

Use `Process.dirs.save`. It is a folder named after your game inside the user data folder. See [Files and saving](../manual/files.md).

## How do I ship my game?

Run `luv package`. It makes a folder with one program and any native plugins next to it. See [Shipping your game](../manual/shipping.md).

## The window stays black or does not open

luv draws with Vulkan. Update your graphics drivers. On Linux, make sure the Vulkan loader, `libvulkan1`, is installed.
