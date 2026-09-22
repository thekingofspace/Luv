# Controller API

Controller input and vibration for one window.

```luau
local controller = window:GetAPI("Controller")
```

## Description

You get the Controller API from [window:GetAPI](window.md#getapi). Each window has its own. Asking again returns the same object.

Each connected controller has a number id. Methods with an optional `id` read only that controller. Without an `id`, they read all controllers together. [GetControllers](#getcontrollers) lists the ids.

Focus rules:

- [ButtonDown](#buttondown), [ButtonUp](#buttonup) and [AxisChanged](#axischanged) only fire while the window has focus.
- [Connected](#connected), [Disconnected](#disconnected) and [ActivationChanged](#activationchanged) fire even while the window does not have focus.
- While the window does not have focus, [IsButtonDown](#isbuttondown) returns `false`, [GetButtonsDown](#getbuttonsdown) returns an empty table, [GetAxis](#getaxis) returns `0` and [GetStick](#getstick) returns `udim.new(0, 0)`. The other methods keep working.
- When the window loses focus, `ButtonUp` fires for every held button. `AxisChanged` fires with `0` for every axis that is not at 0. This happens before [FocusLost](window.md#focuslost).

When a controller disconnects, its held buttons do not fire `ButtonUp`.

After the window closes, the properties and methods error with `this input API belongs to a window that is closed`.

See [Input](../manual/input.md) for a guide.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `IsConnected` | `boolean` | `true` while at least one controller is connected. Read only. |

## Buttons

The button names follow an Xbox controller. See [ControllerButton](enums.md#controllerbutton).

| Name | Button |
| --- | --- |
| `A` | The bottom face button. |
| `B` | The right face button. |
| `X` | The left face button. |
| `Y` | The top face button. |
| `LeftBumper`, `RightBumper` | The shoulder buttons. |
| `LeftTrigger`, `RightTrigger` | The triggers, as buttons. Use [GetAxis](#getaxis) to read how far they are pressed. |
| `Select` | The small button left of the middle. |
| `Start` | The small button right of the middle. |
| `Home` | The logo button in the middle. |
| `LeftStick`, `RightStick` | Pressing a stick in. |
| `DPadUp`, `DPadDown`, `DPadLeft`, `DPadRight` | The DPad. |

## Axes

See [ControllerAxis](enums.md#controlleraxis).

| Name | Range | Description |
| --- | --- | --- |
| `LeftStickX`, `RightStickX` | -1 to 1 | -1 is left and 1 is right. |
| `LeftStickY`, `RightStickY` | -1 to 1 | -1 is down and 1 is up. |
| `LeftTrigger`, `RightTrigger` | 0 to 1 | 0 is let go and 1 is pressed all the way. |

Small movements near the rest position read as 0. Past that point, the value is scaled so it still reaches 1. luv does not add a dead zone of its own.

## Methods

| Method | Returns | Yields |
| --- | --- | --- |
| [GetControllers](#getcontrollers)() | { [ControllerInfo](#controllerinfo) } | no |
| [IsControllerConnected](#iscontrollerconnected)(id) | `boolean` | no |
| [IsButtonDown](#isbuttondown)(button, id?) | `boolean` | no |
| [GetButtonsDown](#getbuttonsdown)(id?) | { [ControllerButton](enums.md#controllerbutton) } | no |
| [GetAxis](#getaxis)(axis, id?) | `number` | no |
| [GetStick](#getstick)(stick, id?) | [UDim](udim.md) | no |
| [Vibrate](#vibrate)(strength, duration, id?) | none | no |
| [StopVibrating](#stopvibrating)(id?) | none | no |

## Method descriptions

### GetControllers

```luau
controller:GetControllers(): { ControllerInfo }
```

Returns a [ControllerInfo](#controllerinfo) for every connected controller, sorted by id. Use it to find the controllers that were connected before you got the API.

```luau
local Window = import("Window")

local window = Window.new()
local controller = window:GetAPI("Controller")

for _, pad in controller:GetControllers() do
	print(pad.Id, pad.Name, pad.CanVibrate)
end
```

### IsControllerConnected

```luau
controller:IsControllerConnected(id: number): boolean
```

Returns `true` if a controller with this id is connected.

### IsButtonDown

```luau
controller:IsButtonDown(button: ControllerButtonEnum, id: number?): boolean
```

Returns `true` while the button is held. Without an `id`, it returns `true` when any controller holds the button. Returns `false` while the window does not have focus.

### GetButtonsDown

```luau
controller:GetButtonsDown(id: number?): { ControllerButtonEnum }
```

Returns every held button, sorted by name. Without an `id`, it lists the held buttons of all controllers, each one once.

### GetAxis

```luau
controller:GetAxis(axis: ControllerAxisEnum, id: number?): number
```

Returns the value of an axis. See [Axes](#axes) for the ranges. Without an `id`, it returns the value that is furthest from 0 across all controllers. Returns `0` while the window does not have focus, and when no controller has that id.

### GetStick

```luau
controller:GetStick(stick: ControllerStickEnum, id: number?): UDim
```

Returns both axes of a stick as a [UDim](udim.md). X holds the X axis and Y holds the Y axis, so pushing the stick up gives a positive Y. On screen, luv uses a positive Y for down. So flip Y when you move something with a stick.

Without an `id`, it returns the stick that is pushed the furthest across all controllers. Returns `udim.new(0, 0)` while the window does not have focus.

```luau
local Window = import("Window")

local window = Window.new()
local controller = window:GetAPI("Controller")
local position = udim.new(640, 360)

window.PreFrame:BindHandler("move", function(dt: number)
	local stick = controller:GetStick(enum.ControllerStick.Left)
	position += udim.new(stick.X, -stick.Y) * (300 * dt)
end)
```

### Vibrate

```luau
controller:Vibrate(strength: number, duration: number, id: number?)
```

Makes controllers rumble. It returns right away.

- `strength` goes from 0 to 1. A value above 1 counts as 1 and a value below 0 counts as 0. Both motors use the same strength.
- `duration` is in seconds.
- Without an `id`, every controller that can vibrate rumbles. A controller that cannot vibrate ignores the call.
- A call without an `id` first stops every rumble. A call with an `id` first stops the rumble that was started for that id.
- A `strength` or `duration` of 0 only stops.
- It works while the window does not have focus.

It errors with `the vibration strength must be a number between 0 and 1` when `strength` is NaN or infinite. It errors with `the vibration duration must be a number of seconds, 0 or more` when `duration` is negative, NaN or infinite.

```luau
local Window = import("Window")

local window = Window.new()
local controller = window:GetAPI("Controller")

controller.ButtonDown:BindHandler("rumble", function(button, id: number)
	if button == enum.ControllerButton.A then
		controller:Vibrate(0.5, 0.2, id)
	end
end)
```

### StopVibrating

```luau
controller:StopVibrating(id: number?)
```

Stops a rumble. This is the same as `controller:Vibrate(0, 0, id)`. Without an `id`, it stops every rumble. With an `id`, it only stops a rumble that was started for that id. A rumble that was started without an `id` keeps going.

## Signals

### ButtonDown

```luau
controller.ButtonDown: Signal<ControllerButtonEnum, number>
```

Fires when a button is pressed while the window has focus. The arguments are the button and the controller id.

### ButtonUp

```luau
controller.ButtonUp: Signal<ControllerButtonEnum, number>
```

Fires when a button is let go while the window has focus. The arguments are the button and the controller id. It also fires for every held button when the window loses focus.

### AxisChanged

```luau
controller.AxisChanged: Signal<ControllerAxisEnum, number, number>
```

Fires when the value of an axis changes while the window has focus. The arguments are the axis, the new value and the controller id. When the window loses focus, it fires with `0` for every axis that is not at 0.

### Connected

```luau
controller.Connected: Signal<number, string>
```

Fires when a controller connects. The arguments are the id and the name of the controller. It fires even while the window does not have focus.

### Disconnected

```luau
controller.Disconnected: Signal<number>
```

Fires when a controller disconnects. The argument is its id. It fires even while the window does not have focus.

### ActivationChanged

```luau
controller.ActivationChanged: Signal<boolean>
```

Fires when `IsConnected` changes. It fires with `true` when the first controller connects and with `false` when the last one disconnects. It fires after [Connected](#connected) or [Disconnected](#disconnected).

## ControllerInfo

A plain table that describes one connected controller. You get it from [GetControllers](#getcontrollers).

| Name | Type | Description |
| --- | --- | --- |
| `Id` | `number` | The id of the controller. |
| `Name` | `string` | The name that the system reports for the controller. |
| `CanVibrate` | `boolean` | `true` when the controller can rumble with [Vibrate](#vibrate). |
