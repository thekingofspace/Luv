# Touch API

Touch screen input for one window.

```luau
local touch = window:GetAPI("Touch")
```

## Description

You get the Touch API from [window:GetAPI](window.md#getapi). Each window has its own. Asking again returns the same object.

Every finger on the screen has a number id. The id stays the same from [Started](#started) until [Ended](#ended) or [Cancelled](#cancelled).

Positions use the same units as [Window.Size](window.md#size-and-position). `(0, 0)` is the top left corner of the inside of the window.

Focus rules:

- Touches only reach the window that has focus. While the window does not have focus, no signal fires except [ActivationChanged](#activationchanged), and [GetTouches](#gettouches) returns an empty table.
- When the window loses focus, [Cancelled](#cancelled) fires for every finger that is down. This happens before [FocusLost](window.md#focuslost).

After the window closes, the properties and methods error with `this input API belongs to a window that is closed`.

See [Input](../manual/input.md) for a guide.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `IsConnected` | `boolean` | `true` when the computer has a touch screen. Read only. |

luv checks for a touch screen about every 2 seconds. If it cannot check, it reports that there is no touch screen. [ActivationChanged](#activationchanged) fires when the value changes.

## Methods

| Method | Returns | Yields |
| --- | --- | --- |
| [GetTouches](#gettouches)() | { [TouchPoint](#touchpoint) } | no |

## Method descriptions

### GetTouches

```luau
touch:GetTouches(): { TouchPoint }
```

Returns every finger that is down, sorted by id.

## Signals

### Started

```luau
touch.Started: Signal<number, UDim, number?>
```

Fires when a finger touches the screen. The arguments are the id, the position and the pressure. The pressure goes from 0 to 1. It is `nil` when the screen does not report pressure.

```luau
local Window = import("Window")

local window = Window.new()
local touch = window:GetAPI("Touch")

touch.Started:BindHandler("tap", function(id: number, position: UDim, pressure: number?)
	print("finger", id, "down at", position.X, position.Y, "pressure", pressure or 1)
end)

touch.Ended:BindHandler("tap", function(id: number, position: UDim)
	print("finger", id, "up at", position.X, position.Y)
end)
```

### Moved

```luau
touch.Moved: Signal<number, UDim, UDim, number?>
```

Fires when a finger moves. The arguments are the id, the new position, how far the finger moved since its last position, and the pressure.

### Ended

```luau
touch.Ended: Signal<number, UDim>
```

Fires when a finger lifts off the screen. The arguments are the id and the last position. It only fires for a finger that luv saw go down or move while the window had focus.

### Cancelled

```luau
touch.Cancelled: Signal<number>
```

Fires when the OS cancels a touch. It also fires for every finger that is down when the window loses focus. The argument is the id. It only fires for a finger that luv saw go down or move while the window had focus.

### ActivationChanged

```luau
touch.ActivationChanged: Signal<boolean>
```

Fires when a touch screen is added or removed. The argument is the new `IsConnected` value. It fires even while the window does not have focus.

## TouchPoint

A plain table that describes one finger that is down. You get it from [GetTouches](#gettouches).

| Name | Type | Description |
| --- | --- | --- |
| `Id` | `number` | The id of the finger. |
| `Position` | [UDim](udim.md) | Where the finger is. |
