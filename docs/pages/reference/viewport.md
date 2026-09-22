# Viewport

Tells you about the screens of the computer.

```luau
local Viewport = import("Viewport")
```

## Description

Each function asks the OS about the screens. It yields the calling coroutine until the OS answers.

Every call returns new tables. They do not update when a screen changes. Call the function again to get fresh values.

Sizes and positions use the same units as [Window.Size and Window.Position](window.md#size-and-position). So you can use them to place windows.

Every function errors when the computer has no display: `screens are not available because no display could be opened`.

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [GetScreens](#getscreens)() | { [Screen](#screen) } | yes |
| [GetPrimaryScreen](#getprimaryscreen)() | [Screen](#screen) | yes |
| [GetScreenSize](#getscreensize)() | [UDim](udim.md) | yes |
| [GetWindowScreen](#getwindowscreen)(window) | [Screen](#screen)`?` | yes |

## Function descriptions

### GetScreens

```luau
Viewport.GetScreens(): { Screen }
```

Returns every screen, in the order the OS lists them.

```luau
local Viewport = import("Viewport")

for _, screen in Viewport.GetScreens() do
	print(screen.Name, screen.Size.X, screen.Size.Y, screen.Scale)
end
```

### GetPrimaryScreen

```luau
Viewport.GetPrimaryScreen(): Screen
```

Returns the primary screen. If the OS marks no screen as primary, it returns the first one. It errors with `no screens are connected` when there are no screens.

### GetScreenSize

```luau
Viewport.GetScreenSize(): UDim
```

Returns the `Size` of the primary screen. It errors like [GetPrimaryScreen](#getprimaryscreen).

### GetWindowScreen

```luau
Viewport.GetWindowScreen(window: Window): Screen?
```

Returns the screen that the [Window](window.md) is on. Returns `nil` when the window is closed or when the OS cannot tell. It errors with `GetWindowScreen expects a Window` when you pass another object.

This example opens a window in the middle of the free part of the primary screen:

```luau
local Window = import("Window")
local Viewport = import("Viewport")

local screen = Viewport.GetPrimaryScreen()
local size = udim.new(1280, 720)
local window = Window.new({
	Title = "Centered",
	Size = size,
	Position = screen.WorkPosition + (screen.WorkSize - size) / 2,
})
local current = Viewport.GetWindowScreen(window)
if current then
	print("The window is on", current.Name)
end
```

## Screen

A plain table that describes one screen.

| Name | Type | Description |
| --- | --- | --- |
| `Name` | `string` | The name the OS gives the screen. It can be empty. |
| `Position` | [UDim](udim.md) | The top left corner of the screen on the desktop. |
| `Size` | [UDim](udim.md) | The size of the screen. This is `PixelSize` divided by `Scale`. |
| `PixelSize` | [UDim](udim.md) | The size of the screen in real pixels. |
| `WorkPosition` | [UDim](udim.md) | The top left corner of the part of the screen that the taskbar does not cover. On Linux this is the same as `Position`. |
| `WorkSize` | [UDim](udim.md) | The size of the part of the screen that the taskbar does not cover. On Linux this is the same as `Size`. |
| `Scale` | `number` | The display scale. `1.5` means 150%. |
| `RefreshRate` | `number?` | How many times each second the screen updates, like `60` or `59.94`. It is `nil` when the OS does not say. |
| `IsPrimary` | `boolean` | `true` for the primary screen. |

Each screen turns its `Position` and `Size` into these units with its own `Scale`.
