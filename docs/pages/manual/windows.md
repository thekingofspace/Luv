# Windows and frames

A luv game draws into windows. This page shows how to open a window, run code every frame, handle resizes and close windows.

## Opening a window

```luau
local Window = import("Window")

local window = Window.new({
	Title = "Space Miner",
	Size = udim.new(1280, 720),
})
```

[Window.new](../reference/window.md#new) returns the window right away. The window shows up on screen a moment later. Every field of the config is optional. The title defaults to the game name from [build.toml](../start/build-toml.md).

The game keeps running while a window is open, even after your main script ends. You do not need to keep the window in a variable.

## The frame loop

Each window runs its own frame loop. Every frame, luv does these steps in order:

1. Fires [PreFrame](../reference/window.md#preframe).
2. Fires [OnFrame](../reference/window.md#onframe).
3. Fires [AfterFrame](../reference/window.md#afterframe).
4. Draws the window and runs its [post processes](post-processing.md).

All three signals get the same number. It is the time in seconds since the last frame. Multiply speeds by it, so things move at the same speed at any frame rate.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Frames" })
local Renderable = window:GetAPI("Renderable")
local box = Renderable.new("RenderableShape", {
	Position = udim.new(0, 100),
	Size = udim.new(40, 40),
})

window.OnFrame:BindHandler("move", function(dt: number)
	box.Position += udim.new(200 * dt, 0)
end)
```

The loop starts as soon as `Window.new` returns. It stops when the window closes. It keeps going while the window is minimized or does not have focus.

A handler that yields does not hold up the frame. The rest of the handler runs later. See [Yielding and coroutines](yielding.md). An error in a handler is printed, and the frame loop keeps going.

### Frame rate

`FPS` sets the most frames per second. The default is 60 and the highest is 1000. You can change it at any time.

luv waits between frames. When a frame runs late, the next frame starts right away. luv does not run extra frames to catch up. So the time between two frames can be longer than `1 / FPS`. Always use the number that the frame signals give you.

This window runs slower while the player is in another window:

```luau
local Window = import("Window")

local window = Window.new({ Title = "Frames", FPS = 144 })

window.FocusLost:BindHandler("fps", function()
	window.FPS = 15
end)

window.FocusGained:BindHandler("fps", function()
	window.FPS = 144
end)
```

## Resizing

The user can resize a window by dragging its edges. Two signals tell you about it:

| Signal | When it fires |
| --- | --- |
| [WindowUpdate](../reference/window.md#windowupdate) | Every time the size changes, also in the middle of a drag. |
| [SizeChanged](../reference/window.md#sizechanged) | Once, when the resize is done. |

When a resize counts as done depends on the system. On Windows, a drag is done when the user lets go of the edge, and every other resize is done right away. On Linux, a resize is done 200 milliseconds after the last change. The game keeps running while the user drags an edge.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Layout" })

window.WindowUpdate:BindHandler("layout", function(size: UDim)
	print("resizing to", size.X, size.Y)
end)

window.SizeChanged:BindHandler("layout", function(size: UDim)
	print("resized to", size.X, size.Y)
end)
```

To resize from a script, set `Size`. The property changes when the OS confirms the new size. Wait for `SizeChanged` when you need the new value.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Layout" })
window.Size = udim.new(1024, 768)
local size = window.SizeChanged:Wait()
print(size.X, size.Y)
```

To stop the user from resizing, set `Resizable` to `false`. `SizeLocked` also pins the size and turns off the maximize button. See [SizeLocked and Resizable](../reference/window.md#sizelocked-and-resizable).

## Full screen and borderless

Set `Type` to change how the window shows up. [Window types](../reference/window.md#window-types) explains each type.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner", Type = enum.WindowType.FullScreen })
```

You can change `Type` at any time. This key switches between a normal window and full screen:

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

A `Borderless` window with the size of a whole screen fills that screen and covers the taskbar. [Viewport](../reference/viewport.md) gives you the size and place of each screen.

```luau
local Window = import("Window")
local Viewport = import("Viewport")

local screen = Viewport.GetPrimaryScreen()
local window = Window.new({
	Title = "Space Miner",
	Type = enum.WindowType.Borderless,
	Position = screen.Position,
	Size = screen.Size,
})
```

## Several windows

Each call to `Window.new` opens another window. Every window has its own frame loop, `FPS`, drawing, input and sound. Input only goes to the window that has focus.

```luau
local Window = import("Window")

local main = Window.new({ Title = "Space Miner" })
local tools = Window.new({ Title = "Tools", Size = udim.new(360, 640), FPS = 30 })

main.Closed:BindHandler("tools", function()
	tools:Close()
end)
```

See [Multiple windows](../reference/window.md#multiple-windows).

## Closing a window

Call [Close](../reference/window.md#close) to close a window. [Closed](../reference/window.md#closed) fires when the window closes. It also fires when the user clicks the close button. You cannot stop the close button, so save your game in a `Closed` handler.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })

window.Closed:BindHandler("save", function()
	print("saving the game")
end)
```

A closed window takes its drawings, post processes, input and sound with it. See [What happens when a window closes](../reference/window.md#what-happens-when-a-window-closes).

[Destroy](../reference/window.md#destroy) also closes the window, but `Closed` does not fire.

## When the game ends

An open window keeps the game running. After the last window closes, the game ends when no other work is left.
