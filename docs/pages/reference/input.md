# Input API

Keyboard input for one window.

```luau
local input = window:GetAPI("Input")
```

## Description

You get the Input API from [window:GetAPI](window.md#getapi). Each window has its own. Asking again returns the same object.

Keys only reach the window that has focus:

- While the window does not have focus, [KeyDown](#keydown), [KeyUp](#keyup) and [TextInput](#textinput) do not fire, and [IsKeyDown](#iskeydown) returns `false`.
- When the window loses focus, [KeyUp](#keyup) fires for every key that is held. This happens before [FocusLost](window.md#focuslost).

Keys are named by their place on a US keyboard. The name does not change with the keyboard layout. For the text that a key types, use [TextInput](#textinput). Keys that luv does not know come in as `enum.KeyCode.Unknown`.

After the window closes, the properties and methods error with `this input API belongs to a window that is closed`.

See [Input](../manual/input.md) for a guide.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `IsConnected` | `boolean` | `true` when the computer has a keyboard. Read only. |

luv checks for a keyboard about every 2 seconds. If it cannot check, it reports that a keyboard is there. [ActivationChanged](#activationchanged) fires when the value changes.

## Methods

| Method | Returns | Yields |
| --- | --- | --- |
| [IsKeyDown](#iskeydown)(key) | `boolean` | no |
| [GetKeysDown](#getkeysdown)() | { [KeyCode](enums.md#keycode) } | no |

## Method descriptions

### IsKeyDown

```luau
input:IsKeyDown(key: KeyCodeEnum): boolean
```

Returns `true` while the key is held. Returns `false` while the window does not have focus. It errors when `key` is an item of another enum, for example `expected an enum.KeyCode item, got enum.MouseButton.Left`.

```luau
local Window = import("Window")

local window = Window.new()
local input = window:GetAPI("Input")

window.PreFrame:BindHandler("walk", function(dt: number)
	if input:IsKeyDown(enum.KeyCode.D) then
		print("walking right for", dt, "seconds")
	end
end)
```

### GetKeysDown

```luau
input:GetKeysDown(): { KeyCodeEnum }
```

Returns every key that is held, sorted by name.

## Signals

### KeyDown

```luau
input.KeyDown: Signal<KeyCodeEnum>
```

Fires once when a key is pressed. The argument is the key. It does not fire again while the key is held, even when the OS repeats the key. Use [IsKeyDown](#iskeydown) to check a held key every frame.

```luau
local Window = import("Window")

local window = Window.new()
local input = window:GetAPI("Input")

input.KeyDown:BindHandler("pause", function(key)
	if key == enum.KeyCode.Escape then
		print("paused")
	end
end)
```

### KeyUp

```luau
input.KeyUp: Signal<KeyCodeEnum>
```

Fires when a key is let go. The argument is the key. It also fires for every held key when the window loses focus.

### TextInput

```luau
input.TextInput: Signal<string>
```

Fires with the text that a key press types. The text follows the keyboard layout and the Shift key. Unlike [KeyDown](#keydown), it repeats while a key is held, like typing in a text box.

Control characters are removed. So Return, Tab, Backspace, Escape and Delete never fire `TextInput`. Handle those keys with [KeyDown](#keydown). For one key press, `KeyDown` fires before `TextInput`.

```luau
local Window = import("Window")

local window = Window.new()
local input = window:GetAPI("Input")
local typed = ""

input.TextInput:BindHandler("chat", function(text: string)
	typed ..= text
end)
```

### ActivationChanged

```luau
input.ActivationChanged: Signal<boolean>
```

Fires when a keyboard is plugged in or removed. The argument is the new `IsConnected` value. It fires even while the window does not have focus.

## Key names

Most names match the label on the key. These ones are good to know:

| Key | Name |
| --- | --- |
| The digit keys above the letters | `Zero` to `Nine` |
| Enter | `Return` |
| The arrow keys | `Up`, `Down`, `Left` and `Right` |
| Shift, Control and Alt | `LeftShift`, `RightShift`, `LeftControl`, `RightControl`, `LeftAlt` and `RightAlt` |
| The Windows key | `LeftSuper` and `RightSuper` |
| The menu key | `Menu` |
| The key left of 1 | `Backquote` |
| The number pad | `KeypadZero` to `KeypadNine`, `KeypadPeriod`, `KeypadDivide`, `KeypadMultiply`, `KeypadMinus`, `KeypadPlus`, `KeypadEnter` and `KeypadEquals` |

See [KeyCode](enums.md#keycode) for the full list.
