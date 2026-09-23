# Window

Inherits: [BaseGameObject](basegameobject.md)

A window on the screen. It runs a frame loop and gives you the APIs for drawing, input and sound.

```luau
local Window = import("Window")
```

## Description

You open a window with [Window.new](#new). Its `ClassName` is `"Window"`.

Each window runs its own frame loop. Every frame it fires [PreFrame](#preframe), [OnFrame](#onframe) and [AfterFrame](#afterframe), and then it draws the window. The `FPS` property sets the most frames per second.

Drawing, input and sound belong to a window. You get them with [GetAPI](#getapi).

The game keeps running while a window is open, even after the main script ends. You do not need to keep the window in a variable to keep it open. Code inside a parallel block can open windows too.

See [Windows and frames](../manual/windows.md) for a guide.

It also has every member of [BaseGameObject](basegameobject.md). [Destroy](#destroy) works a little differently on a window.

## Creating a window

### new

```luau
Window.new(config: WindowConfig?): Window
```

Opens a new window and returns it. It does not yield. The frame loop starts right away. The window shows up on screen a moment later.

When the OS reports the real size, place and focus of the new window, the matching signals fire. For example [FocusGained](#focusgained) fires when the new window gets focus.

```luau
local Window = import("Window")

local window = Window.new({
	Title = "Space Miner",
	Size = udim.new(1280, 720),
	FPS = 144,
	BackgroundColor = color.fromRGB(12, 14, 24),
})
```

#### Config

Every field is optional. Unknown fields are ignored.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Title` | `string` | the game name | The text in the title bar. The default is `name` from [build.toml](../start/build-toml.md). |
| `Size` | [UDim](udim.md) | `udim.new(1280, 720)` | The size of the inside of the window. X is the width and Y is the height. |
| `Position` | [UDim](udim.md) | none | Where the top left corner of the window goes on the desktop. When you leave it out, the OS picks a place. |
| `SizeLocked` | `boolean` | `false` | Pins the window to its size. See [SizeLocked and Resizable](#sizelocked-and-resizable). |
| `FPS` | `number` | `60` | The most frames per second. It must be more than 0. Values over 1000 become 1000. |
| `Type` | [WindowType](enums.md#windowtype) | `Windowed` | How the window shows up. See [Window types](#window-types). |
| `Resizable` | `boolean` | `true` | Whether the user can drag the edges to resize the window. |
| `Icon` | [Asset](asset.md) or `string` | none | The window icon. See [Icon](#icon). |
| `BackgroundColor` | [Color](color.md) | `color.black` | The color behind everything that is drawn. |

`Window.new` errors when:

| Message | Cause |
| --- | --- |
| `windows are not available because no display could be opened` | The computer has no display. |
| `FPS must be a number greater than 0` | `FPS` is 0, negative or NaN. |
| `a window position must hold finite numbers` | `Position` holds NaN or an infinite number. |
| `expected an enum.WindowType item, got enum.MouseButton.Left` | `Type` is an item of another enum. |
| `Icon must be an Asset or an asset path, got number` | `Icon` is not an Asset, a string or `nil`. |

If the OS cannot make the window, the window closes and [Closed](#closed) fires. luv prints the error `the window could not be opened: <reason>`.

## Properties

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `Title` | `string` | the game name | The text in the title bar. |
| `Size` | [UDim](udim.md) | `udim.new(1280, 720)` | The size of the inside of the window. A new value applies when the OS confirms it. See [Size and Position](#size-and-position). |
| `Position` | [UDim](udim.md) | set by the OS | The top left corner of the window on the desktop. A new value applies when the OS confirms it. See [Size and Position](#size-and-position). |
| `SizeLocked` | `boolean` | `false` | Pins the window to its size. See [SizeLocked and Resizable](#sizelocked-and-resizable). |
| `Resizable` | `boolean` | `true` | Whether the user can drag the edges to resize the window. |
| `FPS` | `number` | `60` | The most frames per second. It must be more than 0, or it errors with `FPS must be a number greater than 0`. Values over 1000 become 1000. A new value applies from the next frame. |
| `Type` | [WindowType](enums.md#windowtype) | `Windowed` | How the window shows up. See [Window types](#window-types). It holds the last type you set. It does not change when the user maximizes the window with the OS buttons. |
| `Icon` | [Asset](asset.md) or `string` | `nil` | The window icon. See [Icon](#icon). |
| `BackgroundColor` | [Color](color.md) | `color.black` | The color behind everything that is drawn. A new value applies from the next frame. |
| `Focused` | `boolean` | `false` | `true` while the window has keyboard focus. Read only. |
| `IsOpen` | `boolean` | `true` | `false` once the window is closed. Read only. |

Setting a property after the window closes does nothing.

### Size and Position

Sizes and positions do not change with the display scale. On a screen with a scale of 150%, a window with a width of 1000 covers 1500 real pixels.

- `Size` is the inside of the window. It does not include the title bar or the border.
- `Position` is the top left corner of the whole window, with its title bar and border. It can be negative when you have more than one screen.

Setting `Size` or `Position` sends a request to the OS. The property keeps its old value until the OS confirms the change. Then [SizeChanged](#sizechanged) or [Moved](#moved) fires, and the property holds the new value.

```luau
local Window = import("Window")

local window = Window.new()
window.Size = udim.new(1024, 768)
local size = window.SizeChanged:Wait()
print(size.X, size.Y)
```

- Setting `Size` errors when X or Y is 0 or less: `window sizes must be greater than 0`.
- Setting `Position` errors when a number is NaN or infinite: `a window position must hold finite numbers`.
- In the `FullScreen` and `ExclusiveFullScreen` types, new values for `Size` and `Position` are ignored.
- A `Borderless` window that covers its screen ignores new values for `Position`.

### SizeLocked and Resizable

- `Resizable` only decides whether the user can drag the edges.
- `SizeLocked` pins the window to its current size. The user cannot resize it, and the maximize button is turned off. A locked window cannot be resized even when `Resizable` is `true`.
- A script can still set `Size` on a locked window. The lock then moves to the new size.
- The lock applies to `Windowed` windows and to `Borderless` windows that do not cover the screen. When you switch back to `Windowed`, the window gets its locked size again.

### Icon

Set `Icon` to an [Asset](asset.md) or to an asset path like `"icon.png"`. A path works like the paths you give to [Asset](asset.md). Set it to `nil` to go back to the default icon.

The default icon is the game icon from the `icon` field of [build.toml](../start/build-toml.md#the-game-icon). A window that never sets `Icon` uses it too. When `build.toml` has no icon, the default is the plain icon of the OS.

luv loads the image in the background, so the icon shows up a moment later. If the file is missing or is not an image, luv prints an error that starts with `cannot use the window icon:`. Setting a value of any other type errors with `Icon must be an Asset or an asset path, got <type>`.

Reading `Icon` gives back the value you set.

An icon can be a PNG, JPEG, GIF, WebP, BMP, ICO, TIFF, TGA, DDS, HDR, EXR, PNM, QOI, farbfeld or SVG file. An SVG icon uses the size written in the SVG.

```luau
local Window = import("Window")
local Asset = import("Asset")

local window = Window.new({ Title = "Space Miner", Icon = "icon.png" })
window.Icon = Asset.Load("icon-dark.png")
```

## Window types

Set the type with the `Type` field of [Window.new](#new) or with the `Type` property. See [WindowType](enums.md#windowtype).

| Type | What it does |
| --- | --- |
| `Windowed` | A normal window with a title bar and a border. |
| `Borderless` | A window with no title bar and no border. If its `Size` covers the whole screen, it fills that screen and covers the taskbar. A smaller one stays a normal window at its `Size` and `Position`. |
| `Maximized` | A window with a title bar that fills the screen. The taskbar stays visible. |
| `FullScreen` | Fills the whole screen the window is on and covers the taskbar. The screen keeps its resolution. |
| `ExclusiveFullScreen` | Takes over the screen the window is on. luv switches the screen to its highest resolution, at the highest refresh rate for that resolution. If the screen lists no modes, this works like `FullScreen`. |

`FullScreen` and `ExclusiveFullScreen` use the screen the window is on. To pick a screen, set `Position` first. [Viewport](viewport.md) tells you where each screen is.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local input = window:GetAPI("Input")

input.KeyDown:BindHandler("fullscreen", function(key)
	if key == enum.KeyCode.F11 then
		window.Type = if window.Type == enum.WindowType.FullScreen
			then enum.WindowType.Windowed
			else enum.WindowType.FullScreen
	end
end)
```

## Methods

### GetAPI

```luau
window:GetAPI(name: "Renderable"): Renderable_API
window:GetAPI(name: "Input"): Input_API
window:GetAPI(name: "Mouse"): Mouse_API
window:GetAPI(name: "Controller"): Controller_API
window:GetAPI(name: "Touch"): Touch_API
window:GetAPI(name: "Sound"): Sound_API
```

Returns one of the APIs of this window. It does not yield.

| Name | What you get |
| --- | --- |
| `"Renderable"` | The [Renderable API](renderable-api.md). It draws shapes, images and text in this window. |
| `"Input"` | The [Input API](input.md) for the keyboard. |
| `"Mouse"` | The [Mouse API](mouse.md). |
| `"Controller"` | The [Controller API](controller.md). |
| `"Touch"` | The [Touch API](touch.md). |
| `"Sound"` | The [Sound API](sound-api.md). |

luv makes each API the first time you ask for it. Later calls return the same one. Names are case sensitive.

It errors when:

- The name is not in the list. The message is `'<name>' is not a window API, the available APIs are Renderable, Input, Mouse, Controller, Touch, Sound`.
- The window is closed. The message is `the <name> API is not available because the window is closed`.

```luau
local Window = import("Window")

local window = Window.new()
local Renderable = window:GetAPI("Renderable")
local input = window:GetAPI("Input")
```

### AddPostProcess

```luau
window:AddPostProcess(shader: Shader?): PostProcess
```

Adds a [PostProcess](postprocess.md) pass to the window and returns it. When you pass a [Shader](shader.md), it is loaded on the new pass. By default a new pass runs after the passes that are already there. It does not yield.

It errors when the shader cannot be loaded. Then no pass is added. It also errors when the window is closed: `post processes cannot be added because the window is closed`.

See [Post processing](../manual/post-processing.md) for a guide.

### GetPostProcesses

```luau
window:GetPostProcesses(): { PostProcess }
```

Returns the passes of this window in the order they run. Returns an empty table after the window closes.

### ClearPostProcesses

```luau
window:ClearPostProcesses()
```

Destroys every pass of this window.

### Close

```luau
window:Close()
```

Closes the window and fires [Closed](#closed). Calling it on a closed window does nothing. See [What happens when a window closes](#what-happens-when-a-window-closes).

After `Closed` has fired, luv destroys every signal of the window and of its input APIs, so their handlers are dropped. Anything those handlers held goes away with them.

If you call it inside a [PreFrame](#preframe) or [OnFrame](#onframe) handler, the other frame signals of that frame still fire. No new frames start.

### Destroy

```luau
window:Destroy()
```

Closes the window without firing [Closed](#closed). Like [Close](#close), it destroys every signal of the window and of its input APIs, so their handlers are gone. A coroutine that waits on one of these signals gets an error like `Closed was destroyed while it was being waited on`.

Use `Close` when you want your `Closed` handlers to run. Use `Destroy` when you want the window gone without them. See [BaseGameObject](basegameobject.md#destroy).

## Signals

### SizeChanged

```luau
window.SizeChanged: Signal<UDim>
```

Fires once when a resize is done. The argument is the new [Size](#size-and-position).

- On Windows, it fires right away when a script sets `Size`, when the window is maximized and when the type changes. When the user drags an edge, it fires once when they let go.
- On Linux, it fires 200 milliseconds after the last size change.

It also fires after the window opens if the OS gave the window another size than the one you asked for.

### WindowUpdate

```luau
window.WindowUpdate: Signal<UDim>
```

Fires every time the size changes, with the new size. This includes every step while the user drags an edge. Use it to update your layout during a resize. For the same change, it fires before [SizeChanged](#sizechanged).

### Moved

```luau
window.Moved: Signal<UDim>
```

Fires when the OS reports a new [Position](#size-and-position). The argument is the new position. It fires many times while the user drags the window. It can also fire right after the window opens, when the OS reports where it put the window.

### PreFrame

```luau
window.PreFrame: Signal<number>
```

Fires at the start of every frame. The argument is the time in seconds since the last frame.

Every frame, luv fires `PreFrame`, then [OnFrame](#onframe), then [AfterFrame](#afterframe). Then it draws the window. All three get the same number. Changes you make in any of them show up in that frame. See [The frame loop](../manual/windows.md#the-frame-loop).

### OnFrame

```luau
window.OnFrame: Signal<number>
```

Fires after [PreFrame](#preframe) in every frame, with the same number.

### AfterFrame

```luau
window.AfterFrame: Signal<number>
```

Fires after [OnFrame](#onframe) in every frame, with the same number. luv draws the window right after it.

### FocusGained

```luau
window.FocusGained: Signal<()>
```

Fires when the window gets keyboard focus. This includes a new window that opens with focus. `Focused` is `true` from then on.

### FocusLost

```luau
window.FocusLost: Signal<()>
```

Fires when the window loses keyboard focus. Right before it fires, the input APIs let go of every key, button and touch that is held. See [Focus](../manual/input.md#focus).

### Closed

```luau
window.Closed: Signal<()>
```

Fires once when the window closes. That happens when:

- A script calls [Close](#close).
- The user clicks the close button of the window.
- The OS could not make the window.

It does not fire for [Destroy](#destroy). When it fires, the window is already closed. The close button always closes the window. You cannot stop it.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
window.Closed:BindHandler("save", function()
	print("saving the game")
end)
```

## What happens when a window closes

- `IsOpen` becomes `false` and the window leaves the screen.
- Frames stop.
- Every [Renderable](renderable.md) and [PostProcess](postprocess.md) of the window is destroyed. The Renderable API errors with `renderables cannot be created because the window is closed`.
- Properties and methods of the input APIs error with `this input API belongs to a window that is closed`.
- The sounds of the window stop and its sound nodes are destroyed. The Sound API errors with `this Sound API belongs to a window that is closed`.
- [GetAPI](#getapi) errors.
- [Closed](#closed) fires, unless the window was destroyed.
- Every signal of the window and of its input APIs is destroyed, so their handlers are dropped. Binding a handler after that errors.

An open window keeps the game running. After the last window closes, the game ends when no other work is left.

## Multiple windows

You can open as many windows as you like. Each call to [Window.new](#new) makes a new one.

- Each window has its own frame loop, `FPS`, renderables, post processes and APIs.
- Keyboard, mouse and touch input only reach the window that has focus. Controller buttons and sticks also only reach the window that has focus.
- Every window that has the Controller API hears when a controller connects or disconnects.
- A [ToSpeaker](tospeaker.md) with [OwnedByWindow](tospeaker.md#ownedbywindow) set to `true` goes quiet while its window does not have focus. `OwnedByWindow` is `true` by default.
- The game keeps running until every window is closed.

```luau
local Window = import("Window")

local main = Window.new({ Title = "Space Miner" })
local tools = Window.new({ Title = "Tools", Size = udim.new(360, 640), FPS = 30 })

main.Closed:BindHandler("tools", function()
	tools:Close()
end)
```
