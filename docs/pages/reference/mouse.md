# Mouse API

Mouse input and the mouse cursor for one window.

```luau
local mouse = window:GetAPI("Mouse")
```

## Description

You get the Mouse API from [window:GetAPI](window.md#getapi). Each window has its own. Asking again returns the same object.

Positions use the same units as [Window.Size](window.md#size-and-position). `(0, 0)` is the top left corner of the inside of the window. X grows to the right and Y grows down.

Focus rules:

- `Position` and `IsInside` update even while the window does not have focus.
- Every signal except [ActivationChanged](#activationchanged) only fires while the window has focus.
- [IsButtonDown](#isbuttondown) returns `false` while the window does not have focus.
- When the window loses focus, [ButtonUp](#buttonup) fires for every held button. This happens before [FocusLost](window.md#focuslost).

`Icon`, `Visible` and `LockMode` only change the cursor of this window.

After the window closes, the properties and methods error with `this input API belongs to a window that is closed`.

See [Input](../manual/input.md) for a guide.

## Properties

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `Position` | [UDim](udim.md) | `udim.new(0, 0)` | Where the cursor is. It stays the same while `LockMode` is `Locked`. Read only. |
| `IsInside` | `boolean` | `false` | `true` while the cursor is over the window. Read only. |
| `Icon` | [MouseIcon](enums.md#mouseicon) | `Default` | The cursor image over this window. See [Icons](#icons). |
| `Visible` | `boolean` | `true` | Set it to `false` to hide the cursor over this window. |
| `LockMode` | [MouseLockMode](enums.md#mouselockmode) | `None` | Keeps the cursor in the window or in one place. See [Lock modes](#lock-modes). |
| `IsConnected` | `boolean` | none | `true` when the computer has a mouse. Read only. |

Setting `Icon` or `LockMode` to an item of another enum errors, for example `expected an enum.MouseIcon item, got enum.MouseButton.Left`.

luv checks for a mouse about every 2 seconds. If it cannot check, it reports that a mouse is there. [ActivationChanged](#activationchanged) fires when `IsConnected` changes.

## Lock modes

| Mode | What it does |
| --- | --- |
| `None` | The cursor moves freely. |
| `Confined` | The cursor cannot leave the window. |
| `Locked` | The cursor stays in one place. `Position` stops changing. [Moved](#moved) still fires, and its second argument is the raw motion of the mouse. |

On X11 desktops the OS cannot lock the cursor. There luv keeps the cursor in the window and moves it back to the middle after each move, while the window has focus.

For a camera that turns with the mouse, set `LockMode` to `Locked` and `Visible` to `false`.

```luau
local Window = import("Window")

local window = Window.new()
local mouse = window:GetAPI("Mouse")
local yaw = 0

mouse.LockMode = enum.MouseLockMode.Locked
mouse.Visible = false
mouse.Moved:BindHandler("look", function(_position: UDim, delta: UDim)
	yaw += delta.X * 0.2
end)
```

## Icons

The look of each icon comes from the OS. See [MouseIcon](enums.md#mouseicon).

| Name | Use |
| --- | --- |
| `Default` | The normal arrow. |
| `Pointer` | Links and buttons. |
| `Text` | Text you can select or type in. |
| `Crosshair` | Picking an exact point. |
| `Wait` | The game is busy. |
| `Progress` | The game is busy, but you can still click. |
| `Move` | Something you can move. |
| `NotAllowed` | Something you cannot do. |
| `Grab` | Something you can grab. |
| `Grabbing` | Something you are dragging. |
| `Help` | Help is available. |
| `ResizeHorizontal` | Resizing left and right. |
| `ResizeVertical` | Resizing up and down. |
| `ResizeDiagonalDown` | Resizing from the top left to the bottom right. |
| `ResizeDiagonalUp` | Resizing from the bottom left to the top right. |
| `ZoomIn` | Zooming in. |
| `ZoomOut` | Zooming out. |
| `Cell` | Picking a cell in a grid. |
| `Copy` | Something will be copied. |
| `ContextMenu` | A menu is available. |

## Methods

| Method | Returns | Yields |
| --- | --- | --- |
| [IsButtonDown](#isbuttondown)(button) | `boolean` | no |
| [GetButtonsDown](#getbuttonsdown)() | { [MouseButton](enums.md#mousebutton) } | no |

## Method descriptions

### IsButtonDown

```luau
mouse:IsButtonDown(button: MouseButtonEnum): boolean
```

Returns `true` while the button is held. Returns `false` while the window does not have focus.

### GetButtonsDown

```luau
mouse:GetButtonsDown(): { MouseButtonEnum }
```

Returns every held button, sorted by name.

## Signals

### Moved

```luau
mouse.Moved: Signal<UDim, UDim>
```

Fires when the cursor moves over the window. The first argument is the new `Position`. The second is how far the cursor moved since the last position.

While `LockMode` is `Locked`, the first argument stays the same and the second is the raw motion of the mouse.

### ButtonDown

```luau
mouse.ButtonDown: Signal<MouseButtonEnum, UDim>
```

Fires when a button is pressed. The arguments are the button and the cursor position. The buttons are `Left`, `Right`, `Middle`, `Back` and `Forward`. Other buttons are ignored.

```luau
local Window = import("Window")

local window = Window.new()
local mouse = window:GetAPI("Mouse")

mouse.ButtonDown:BindHandler("click", function(button, position: UDim)
	if button == enum.MouseButton.Left then
		print("clicked at", position.X, position.Y)
	end
end)
```

### ButtonUp

```luau
mouse.ButtonUp: Signal<MouseButtonEnum, UDim>
```

Fires when a button is let go. The arguments are the button and the cursor position. It also fires for every held button when the window loses focus.

### Scrolled

```luau
mouse.Scrolled: Signal<UDim>
```

Fires when the mouse wheel turns or when you scroll on a touchpad. The argument holds the amount in lines.

- Y is positive when the wheel rolls up, away from you. It is negative when the wheel rolls down.
- X is positive when you scroll left and negative when you scroll right.
- One step of a mouse wheel is usually 1. On a touchpad, 40 pixels count as 1 line, so you get fractions.

### Entered

```luau
mouse.Entered: Signal<()>
```

Fires when the cursor moves onto the window.

### Left

```luau
mouse.Left: Signal<()>
```

Fires when the cursor leaves the window.

### ActivationChanged

```luau
mouse.ActivationChanged: Signal<boolean>
```

Fires when a mouse is plugged in or removed. The argument is the new `IsConnected` value. It fires even while the window does not have focus.
