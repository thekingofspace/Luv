# Input

luv reads the keyboard, the mouse, controllers and touch screens. Each window has its own input APIs. You get them from the window with [GetAPI](../reference/window.md#getapi).

```luau
local Window = import("Window")

local window = Window.new({ Title = "Input" })
local input = window:GetAPI("Input")
local mouse = window:GetAPI("Mouse")
local controller = window:GetAPI("Controller")
local touch = window:GetAPI("Touch")
```

| API | What it reads |
| --- | --- |
| [Input API](../reference/input.md) | The keyboard. |
| [Mouse API](../reference/mouse.md) | The mouse and the cursor. |
| [Controller API](../reference/controller.md) | Controllers. |
| [Touch API](../reference/touch.md) | Touch screens. |

## Keyboard

Use [KeyDown](../reference/input.md#keydown) for things that happen once per press. It fires once when a key goes down. It does not repeat while the key is held.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Keys" })
local input = window:GetAPI("Input")

input.KeyDown:BindHandler("jump", function(key)
	if key == enum.KeyCode.Space then
		print("jump")
	end
end)
```

Use [IsKeyDown](../reference/input.md#iskeydown) in the frame loop for things that go on while a key is held.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Keys" })
local input = window:GetAPI("Input")
local position = udim.new(100, 100)

window.PreFrame:BindHandler("walk", function(dt: number)
	if input:IsKeyDown(enum.KeyCode.D) then
		position += udim.new(200 * dt, 0)
	end
end)
```

Keys are named by their place on a US keyboard. So `W`, `A`, `S` and `D` sit in the same place on every keyboard layout. See [Key names](../reference/input.md#key-names).

## Typing text

Use [TextInput](../reference/input.md#textinput) for text boxes and chat. It gives you the typed text, with the keyboard layout and Shift applied. It repeats while a key is held.

`TextInput` never includes Return, Tab, Backspace or Escape. Handle those keys with `KeyDown`. `KeyDown` does not repeat, so each press of Backspace removes one character.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Chat" })
local input = window:GetAPI("Input")
local typed = ""

input.TextInput:BindHandler("chat", function(text: string)
	typed ..= text
end)

input.KeyDown:BindHandler("chat", function(key)
	if key == enum.KeyCode.Backspace then
		typed = string.sub(typed, 1, (utf8.offset(typed, -1) or 1) - 1)
	end
end)
```

## Mouse

```luau
local Window = import("Window")

local window = Window.new({ Title = "Mouse" })
local mouse = window:GetAPI("Mouse")

mouse.ButtonDown:BindHandler("click", function(button, position: UDim)
	if button == enum.MouseButton.Left then
		print("clicked at", position.X, position.Y)
	end
end)

mouse.Scrolled:BindHandler("zoom", function(delta: UDim)
	print("scrolled", delta.Y)
end)
```

`Position` tells you where the cursor is. `(0, 0)` is the top left corner of the inside of the window. Set `Icon` to change the cursor over the window, and set `Visible` to `false` to hide it. See the [Mouse API](../reference/mouse.md).

## Looking around with a locked mouse

For a camera that turns with the mouse, lock and hide the cursor. With `LockMode` set to `Locked`, the cursor stays in one place. [Moved](../reference/mouse.md#moved) still fires, and its second argument is how far the mouse moved.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Camera" })
local mouse = window:GetAPI("Mouse")
local yaw, pitch = 0, 0

mouse.LockMode = enum.MouseLockMode.Locked
mouse.Visible = false

mouse.Moved:BindHandler("look", function(_position: UDim, delta: UDim)
	yaw += delta.X * 0.2
	pitch = math.clamp(pitch - delta.Y * 0.2, -89, 89)
end)
```

Set `LockMode` back to `None` and `Visible` back to `true` to free the cursor, for example when a menu opens. See [Lock modes](../reference/mouse.md#lock-modes).

## Controllers

Read the sticks with [GetStick](../reference/controller.md#getstick) and the triggers with [GetAxis](../reference/controller.md#getaxis). A stick gives a positive Y when you push it up. On screen, Y grows down, so flip Y.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Pads" })
local controller = window:GetAPI("Controller")
local position = udim.new(640, 360)

window.PreFrame:BindHandler("move", function(dt: number)
	local stick = controller:GetStick(enum.ControllerStick.Left)
	local boost = controller:GetAxis(enum.ControllerAxis.RightTrigger)
	position += udim.new(stick.X, -stick.Y) * ((200 + 200 * boost) * dt)
end)
```

Without an id, these methods read every controller together. So one player can use any controller. For more players, pass the id of each controller. The ids come from [GetControllers](../reference/controller.md#getcontrollers) and [Connected](../reference/controller.md#connected).

```luau
local Window = import("Window")

local window = Window.new({ Title = "Pads" })
local controller = window:GetAPI("Controller")

controller.Connected:BindHandler("join", function(id: number, name: string)
	print(name, "joined as player", id)
	controller:Vibrate(0.5, 0.25, id)
end)
```

[Vibrate](../reference/controller.md#vibrate) takes a strength from 0 to 1 and a time in seconds.

## Touch

```luau
local Window = import("Window")

local window = Window.new({ Title = "Touch" })
local touch = window:GetAPI("Touch")

touch.Started:BindHandler("tap", function(id: number, position: UDim, _pressure: number?)
	print("finger", id, "at", position.X, position.Y)
end)

touch.Moved:BindHandler("drag", function(id: number, _position: UDim, delta: UDim, _pressure: number?)
	print("finger", id, "moved", delta.X, delta.Y)
end)
```

Each finger keeps its own id until it lifts. [GetTouches](../reference/touch.md#gettouches) lists the fingers that are down.

## Focus

Input only reaches the window that has focus.

| API | While the window does not have focus |
| --- | --- |
| Input | No key signals fire. `IsKeyDown` returns `false`. |
| Mouse | No signals fire. `Position` and `IsInside` still update. `IsButtonDown` returns `false`. |
| Controller | `ButtonDown`, `ButtonUp` and `AxisChanged` do not fire. The methods that read buttons, axes and sticks report nothing held. `Connected` and `Disconnected` still fire. |
| Touch | No signals fire. |

`ActivationChanged` fires on every API, with or without focus.

When the window loses focus, luv lets go of everything that is held. Before [FocusLost](../reference/window.md#focuslost) fires:

- `KeyUp` fires for every held key.
- `ButtonUp` fires for every held mouse button.
- `Cancelled` fires for every finger on the screen.
- `ButtonUp` fires for every held controller button, and `AxisChanged` fires with 0 for every controller axis that is not at 0.

So a key never stays down while the player is in another window.

## Showing the right button prompts

Each input API has an `IsConnected` property and an `ActivationChanged` signal. `IsConnected` tells you if a device is there. `ActivationChanged` fires when that changes. Use them to pick which button prompts to show.

| API | `IsConnected` is `true` when |
| --- | --- |
| Input | The computer has a keyboard. |
| Mouse | The computer has a mouse. |
| Controller | At least one controller is connected. |
| Touch | The computer has a touch screen. |

luv checks for keyboards, mice and touch screens about every 2 seconds.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Prompts" })
local controller = window:GetAPI("Controller")

local function showPrompts(connected: boolean)
	print(if connected then "Press A to start" else "Press Enter to start")
end

controller.ActivationChanged:BindHandler("prompts", showPrompts)
showPrompts(controller.IsConnected)
```

You can also switch prompts when the player uses a device. For example, show controller prompts after a controller `ButtonDown`, and keyboard prompts after a `KeyDown`.
