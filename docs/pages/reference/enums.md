# Enums

Named values that engine properties and functions use. Every enum lives in the `enum` global.

```luau
local mode = enum.WindowType.Borderless
```

## Description

`enum` is a read only table. Each field is an enum type, like `enum.KeyCode`. Each enum type is a read only table of items, like `enum.KeyCode.Space`. Reading an item that does not exist gives `nil`.

Pass items to the properties and functions that ask for them. An item of the wrong enum type raises an error like `expected an enum.WindowType item, got enum.MouseButton.Left`.

## EnumItem

Every item is an `EnumItem`.

| Name | Type | Description |
| --- | --- | --- |
| `Name` | `string` | The name of the item, like `"FullScreen"`. Read only. |
| `Value` | `number` | The place of the item in its enum type, starting at 0. The tables on this page list each one. Read only. |
| `EnumType` | `string` | The name of the enum type, like `"WindowType"`. Read only. |

`tostring(item)` gives the full path, like `enum.WindowType.FullScreen`. `typeof(item)` gives `"EnumItem"`.

```luau
local mode = enum.WindowType.FullScreen
print(mode.Name, mode.Value, mode.EnumType)
print(tostring(mode), typeof(mode))
```

This prints:

```text
FullScreen	3	WindowType
enum.WindowType.FullScreen	EnumItem
```

## GetEnumItems

```luau
enum.MouseButton:GetEnumItems(): { EnumItem }
```

Every enum type has `GetEnumItems`. It returns a list of all its items, in order of `Value`.

```luau
for _, button in enum.MouseButton:GetEnumItems() do
	print(button.Value, button.Name)
end
```

## Comparing items

Compare items with `==`. Items of different enum types are never equal, even when their names match. So `enum.MouseButton.Left == enum.TextXAlignment.Left` is `false`.

Items have no `<`. Compare their `Value` fields when you need an order.

## Items as table keys

Each item exists once in a thread, so items work well as table keys. An item that you send with [Messenger](messenger.md) or use inside a parallel block arrives as the same item. Lookups still work on the other side.

```luau
local actions = {
	[enum.KeyCode.W] = "forward",
	[enum.KeyCode.S] = "back",
}
print(actions[enum.KeyCode.W])
```

## AudioFormat

Used by the `Format` of [AudioPacket](audiopacket.md), [FromBytes](frombytes.md) and [ToBytes](tobytes.md).

| Name | Value |
| --- | --- |
| `Float32` | 0 |
| `Int16` | 1 |

## BlendMode

Used by [Renderable.BlendMode](renderable.md).

| Name | Value |
| --- | --- |
| `Alpha` | 0 |
| `Additive` | 1 |
| `Multiply` | 2 |
| `Opaque` | 3 |

## CipherAlgorithm

Used by `GenerateKey`, `Encrypt` and `Decrypt` in [Crypto](crypto.md).

| Name | Value |
| --- | --- |
| `AES128GCM` | 0 |
| `AES256GCM` | 1 |
| `ChaCha20Poly1305` | 2 |

## ControllerAxis

Used by `GetAxis` and `AxisChanged` in the [Controller API](controller.md).

| Name | Value |
| --- | --- |
| `LeftStickX` | 0 |
| `LeftStickY` | 1 |
| `RightStickX` | 2 |
| `RightStickY` | 3 |
| `LeftTrigger` | 4 |
| `RightTrigger` | 5 |

## ControllerButton

Used by `ButtonDown`, `ButtonUp`, `IsButtonDown` and `GetButtonsDown` in the [Controller API](controller.md).

| Name | Value |
| --- | --- |
| `A` | 0 |
| `B` | 1 |
| `X` | 2 |
| `Y` | 3 |
| `LeftBumper` | 4 |
| `RightBumper` | 5 |
| `LeftTrigger` | 6 |
| `RightTrigger` | 7 |
| `Select` | 8 |
| `Start` | 9 |
| `Home` | 10 |
| `LeftStick` | 11 |
| `RightStick` | 12 |
| `DPadUp` | 13 |
| `DPadDown` | 14 |
| `DPadLeft` | 15 |
| `DPadRight` | 16 |

## ControllerStick

Used by `GetStick` in the [Controller API](controller.md).

| Name | Value |
| --- | --- |
| `Left` | 0 |
| `Right` | 1 |

## HashAlgorithm

Used by `Hash`, `Hasher`, `Hmac`, `Hkdf` and `Pbkdf2` in [Crypto](crypto.md), and by [Hasher.Algorithm](hasher.md).

| Name | Value |
| --- | --- |
| `MD5` | 0 |
| `SHA1` | 1 |
| `SHA224` | 2 |
| `SHA256` | 3 |
| `SHA384` | 4 |
| `SHA512` | 5 |
| `SHA3_256` | 6 |
| `SHA3_384` | 7 |
| `SHA3_512` | 8 |
| `BLAKE3` | 9 |

## KeyAlgorithm

Used by `GenerateKeyPair`, `GetPublicKey`, `Sign`, `Verify` and `SharedSecret` in [Crypto](crypto.md).

| Name | Value |
| --- | --- |
| `Ed25519` | 0 |
| `EcdsaP256` | 1 |
| `EcdsaP384` | 2 |
| `X25519` | 3 |

## KeyCode

Used by `KeyDown`, `KeyUp`, `IsKeyDown` and `GetKeysDown` in the [Input API](input.md).

| Name | Value |
| --- | --- |
| `Unknown` | 0 |
| `A` | 1 |
| `B` | 2 |
| `C` | 3 |
| `D` | 4 |
| `E` | 5 |
| `F` | 6 |
| `G` | 7 |
| `H` | 8 |
| `I` | 9 |
| `J` | 10 |
| `K` | 11 |
| `L` | 12 |
| `M` | 13 |
| `N` | 14 |
| `O` | 15 |
| `P` | 16 |
| `Q` | 17 |
| `R` | 18 |
| `S` | 19 |
| `T` | 20 |
| `U` | 21 |
| `V` | 22 |
| `W` | 23 |
| `X` | 24 |
| `Y` | 25 |
| `Z` | 26 |
| `Zero` | 27 |
| `One` | 28 |
| `Two` | 29 |
| `Three` | 30 |
| `Four` | 31 |
| `Five` | 32 |
| `Six` | 33 |
| `Seven` | 34 |
| `Eight` | 35 |
| `Nine` | 36 |
| `F1` | 37 |
| `F2` | 38 |
| `F3` | 39 |
| `F4` | 40 |
| `F5` | 41 |
| `F6` | 42 |
| `F7` | 43 |
| `F8` | 44 |
| `F9` | 45 |
| `F10` | 46 |
| `F11` | 47 |
| `F12` | 48 |
| `F13` | 49 |
| `F14` | 50 |
| `F15` | 51 |
| `F16` | 52 |
| `F17` | 53 |
| `F18` | 54 |
| `F19` | 55 |
| `F20` | 56 |
| `F21` | 57 |
| `F22` | 58 |
| `F23` | 59 |
| `F24` | 60 |
| `Space` | 61 |
| `Return` | 62 |
| `Escape` | 63 |
| `Tab` | 64 |
| `Backspace` | 65 |
| `Delete` | 66 |
| `Insert` | 67 |
| `Home` | 68 |
| `End` | 69 |
| `PageUp` | 70 |
| `PageDown` | 71 |
| `Up` | 72 |
| `Down` | 73 |
| `Left` | 74 |
| `Right` | 75 |
| `LeftShift` | 76 |
| `RightShift` | 77 |
| `LeftControl` | 78 |
| `RightControl` | 79 |
| `LeftAlt` | 80 |
| `RightAlt` | 81 |
| `LeftSuper` | 82 |
| `RightSuper` | 83 |
| `CapsLock` | 84 |
| `NumLock` | 85 |
| `ScrollLock` | 86 |
| `PrintScreen` | 87 |
| `Pause` | 88 |
| `Menu` | 89 |
| `Backquote` | 90 |
| `Minus` | 91 |
| `Equals` | 92 |
| `LeftBracket` | 93 |
| `RightBracket` | 94 |
| `Backslash` | 95 |
| `Semicolon` | 96 |
| `Quote` | 97 |
| `Comma` | 98 |
| `Period` | 99 |
| `Slash` | 100 |
| `KeypadZero` | 101 |
| `KeypadOne` | 102 |
| `KeypadTwo` | 103 |
| `KeypadThree` | 104 |
| `KeypadFour` | 105 |
| `KeypadFive` | 106 |
| `KeypadSix` | 107 |
| `KeypadSeven` | 108 |
| `KeypadEight` | 109 |
| `KeypadNine` | 110 |
| `KeypadPeriod` | 111 |
| `KeypadDivide` | 112 |
| `KeypadMultiply` | 113 |
| `KeypadMinus` | 114 |
| `KeypadPlus` | 115 |
| `KeypadEnter` | 116 |
| `KeypadEquals` | 117 |

## MouseButton

Used by `ButtonDown`, `ButtonUp`, `IsButtonDown` and `GetButtonsDown` in the [Mouse API](mouse.md).

| Name | Value |
| --- | --- |
| `Left` | 0 |
| `Right` | 1 |
| `Middle` | 2 |
| `Back` | 3 |
| `Forward` | 4 |

## MouseIcon

Used by `Icon` in the [Mouse API](mouse.md).

| Name | Value |
| --- | --- |
| `Default` | 0 |
| `Pointer` | 1 |
| `Text` | 2 |
| `Crosshair` | 3 |
| `Wait` | 4 |
| `Progress` | 5 |
| `Move` | 6 |
| `NotAllowed` | 7 |
| `Grab` | 8 |
| `Grabbing` | 9 |
| `Help` | 10 |
| `ResizeHorizontal` | 11 |
| `ResizeVertical` | 12 |
| `ResizeDiagonalDown` | 13 |
| `ResizeDiagonalUp` | 14 |
| `ZoomIn` | 15 |
| `ZoomOut` | 16 |
| `Cell` | 17 |
| `Copy` | 18 |
| `ContextMenu` | 19 |

## MouseLockMode

Used by `LockMode` in the [Mouse API](mouse.md).

| Name | Value |
| --- | --- |
| `None` | 0 |
| `Confined` | 1 |
| `Locked` | 2 |

## ResampleMode

Used by [RenderableImage.ResampleMode](renderableimage.md).

| Name | Value |
| --- | --- |
| `Smooth` | 0 |
| `Pixelated` | 1 |

## RollOffMode

Used by [ToSpeaker.RollOffMode](tospeaker.md).

| Name | Value |
| --- | --- |
| `Inverse` | 0 |
| `Linear` | 1 |
| `LinearSquare` | 2 |
| `InverseTapered` | 3 |

## ShapeType

Used by [RenderableShape.Shape](renderableshape.md).

| Name | Value |
| --- | --- |
| `Rectangle` | 0 |
| `Circle` | 1 |
| `Triangle` | 2 |
| `RightTriangle` | 3 |
| `Diamond` | 4 |
| `Pentagon` | 5 |
| `Hexagon` | 6 |
| `Octagon` | 7 |

## TextXAlignment

Used by [RenderableText.TextXAlignment](renderabletext.md).

| Name | Value |
| --- | --- |
| `Left` | 0 |
| `Center` | 1 |
| `Right` | 2 |

## TextYAlignment

Used by [RenderableText.TextYAlignment](renderabletext.md).

| Name | Value |
| --- | --- |
| `Top` | 0 |
| `Center` | 1 |
| `Bottom` | 2 |

## WindowType

Used by [Window.Type](window.md#window-types) and the `Type` field of [Window.new](window.md#new).

| Name | Value |
| --- | --- |
| `Windowed` | 0 |
| `Borderless` | 1 |
| `Maximized` | 2 |
| `FullScreen` | 3 |
| `ExclusiveFullScreen` | 4 |
