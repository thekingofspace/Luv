# Introduction

```luau
local Window = import("Window")

local window = Window.new({ Title = "Hello" })
print("Hello world!")
```

Welcome to the official documentation of **luv**, a game engine for making 2D games in Luau. You write your game as Luau scripts. luv runs them, opens windows, draws, plays sound and reads input. When you are done, luv packs everything into one program you can share.

This page gives a quick tour of the engine and of these docs. It shows where to start if you are new and where to look if you need something specific.

## Before you start

If you have never used luv, start with [Installing luv](../start/installing.md) and then [Your first game](../start/first-game.md). Those two pages get a window on screen in a few minutes.

If you already know Luau, the [Manual](../manual/scripts.md) explains each part of the engine. The reference sections list every function, property and signal.

## About luv

luv is a lot like [love2d](https://love2d.org), but made for Luau. Think of it as the next step of that idea. It gives you a small set of fast building blocks and lets you build the game your way. There is no editor to learn. You work with plain files and your favorite code editor.

Here is what luv gives you:

- Windows with frame signals, many windows at once, and screen info.
- Shapes, images and text you can place and style.
- Custom shaders in WGSL, GLSL or SPIR-V, plus post processing.
- A sound system made of nodes you link together, with 26 sound modifiers.
- Keyboard, mouse, controller and touch input.
- Files, saving, HTTP, TCP, UDP and WebSockets.
- Hashing, encryption, signing and random numbers.
- Native plugins written in C or Rust.
- Containers for DLC and mods.
- One command to pack the game into a single program.

luv runs on Windows and Linux.

## Where luv came from

luv grew out of [Ruzit](https://github.com/thekingofspace/Ruzit), an older Luau game engine I made. luv is a full rewrite, not an update.

Compared to Ruzit, luv:

- Uses up to date libraries.
- Is safer with memory and with work done on other threads.
- Never blocks the main thread. Slow work pauses only the script that asked for it.
- Has far more features.

## What about 3D?

luv has no 3D support built in. There is no 3D camera, no 3D model loading and no 3D physics.

Adding 3D yourself is not impossible though. A custom `Renderable` can run your own vertex and fragment shaders, and a native plugin can feed it data every frame. The `cube` and `cube-rust` examples draw a spinning 3D cube this way. See [Adding 3D](../manual/3d.md) to learn how.

## Organization of the documentation

These docs are split into sections:

- **About** has this page, the feature list, the system requirements and common questions.
- **Getting Started** shows how to install luv, make a project and run it.
- **Manual** explains each part of the engine with short examples.
- **Globals and Data Types** covers `import`, `UDim`, `Color`, enums, signals and other basics.
- **Libraries** covers everything you get from `import`, like `FS`, `Net` and `DLL`.
- **Window** covers windows and the APIs you get from a window: drawing, input and sound.
- **Native API** covers the C and Rust side of native plugins.

## About this documentation

Every page on this site is a plain Markdown file inside the `docs/pages` folder of the luv repository. The menu on the left comes from `docs/sidebar.md`. To fix a page, click **Edit on GitHub** at the top of it.

The design of these docs is inspired by the [love2d docs](https://love2d.org/wiki/Main_Page) and the [Godot docs](https://docs.godotengine.org).
