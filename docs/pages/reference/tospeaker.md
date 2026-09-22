# ToSpeaker

Inherits: [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Plays the sound linked into it on an output device.

## Description

You make a ToSpeaker with [Sound:ToSpeaker](sound-api.md#tospeaker). Link other nodes into its `Output`. It adds all of them together and plays the result.

A ToSpeaker plays normal stereo sound, or places the sound in 3D around the [listener](sound-api.md#soundlistener). It can also play on one chosen output device.

The master [Volume](sound-api.md#properties) of the Sound API scales every ToSpeaker of the window. It has no methods and no signals.

It also has every member of [NodeObject](nodeobject.md).

## Properties

| Name | Type | Default | Range | Description |
| --- | --- | --- | --- | --- |
| `Output` | [NodeOutput](nodeports.md) | none | none | Takes sound in. Read only. |
| `Volume` | `number` | `1` | 0 to 10 | The volume. Changes glide over 20 ms. |
| `Device` | `string?` | `nil` | none | The output device. `nil` means the default device. See [Device](#device). |
| `OwnedByWindow` | `boolean` | `true` | none | Goes quiet while the window has no focus. See [OwnedByWindow](#ownedbywindow). |
| `Spatial` | `boolean` | `false` | none | Places the sound in 3D. See [Spatial sound](#spatial-sound). |
| `Position` | `vector` | `vector.create(0, 0, 0)` | none | Where the sound is. |
| `Direction` | `vector` | `vector.create(0, 0, 0)` | none | The way the sound faces. Zero means every way. See [Cones](#cones). |
| `MinDistance` | `number` | `50` | 0 or more | Up to this distance the sound plays at full volume. |
| `MaxDistance` | `number` | `2000` | 0 or more | The distance where the volume stops falling. |
| `RollOffMode` | [RollOffMode](enums.md#rolloffmode) | `enum.RollOffMode.Linear` | none | How the volume falls with distance. See [Roll off](#roll-off). |
| `ConeInnerAngle` | `number` | `360` | 0 to 360 | Full volume inside this angle, in degrees. |
| `ConeOuterAngle` | `number` | `360` | 0 to 360 | `ConeOuterVolume` outside this angle, in degrees. |
| `ConeOuterVolume` | `number` | `0` | 0 to 1 | The volume outside `ConeOuterAngle`. |
| `Binaural` | `boolean` | `true` | none | Adds small differences between the ears. See [Binaural](#binaural). |

## Device

| Value | Where the sound plays |
| --- | --- |
| `nil` | The default output device of the system. When the default changes, luv follows it within about 2 seconds. |
| A name | The output device with exactly this name. [Sound:GetDevices](sound-api.md#getdevices) lists the names. |

- If the named device is missing, the speaker plays on the default device. luv looks for the device again about every 3 seconds. When the device shows up, the sound moves to it.
- If the device goes away while it plays, the sound moves back to the default device.
- Moving to another device fades out over 10 ms and fades back in on the new device.
- A device that is not the default adds about 30 ms of delay.
- A value that is not a string or `nil` errors with `Device must be a device name or nil, got <type>`.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local chat = Sound:ToSpeaker({ OwnedByWindow = false })
for _, name in Sound:GetDevices() do
	if string.find(name, "Headphones", 1, true) then
		chat.Device = name
	end
end
```

## OwnedByWindow

When `OwnedByWindow` is `true`, the speaker goes quiet while its window does not have focus. It fades out over 50 ms and fades back in when focus returns. The nodes linked into it keep playing, so a song stays in time.

Set it to `false` for sound that should keep playing in the background, like music or voice chat.

## Spatial sound

When `Spatial` is `true`, the speaker places its sound around [Sound.Listener](sound-api.md#soundlistener). Turning it on or off blends over 50 ms.

- luv mixes the sound down to mono first.
- A sound straight in front of the listener plays at full volume in both ears.
- With the default listener, positive X is to the right.
- The volume falls with distance. See [Roll off](#roll-off).
- `Position` and `Direction` take a `vector`. A [UDim](udim.md) also works. luv reads its X, Y and Z. Both read back as a `vector`.
- Distances use the same units as `Position`. The defaults suit a world measured in pixels. For meters, try a `MinDistance` of `1` and a `MaxDistance` of `50`.
- A value that is not a vector errors with `Position must be a vector, got <type>`. NaN or infinity errors with `Position must hold finite numbers`.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local engine = Sound:SoundNode("sfx/engine.ogg", { Looping = true })
local car = Sound:ToSpeaker({ Spatial = true, MinDistance = 100, MaxDistance = 3000 })
engine.Input:Link(car.Output)
engine:Play()
Sound.Listener.Position = vector.create(640, 360, 0)
car.Position = vector.create(200, 360, 0)
```

> [!TIP]
> A ToSpeaker has one `Position` for everything linked into it. Moving it also moves the sounds that are still ringing through it. Give each sound its own ToSpeaker when they need their own places.

## Roll off

`RollOffMode` decides how the volume falls between `MinDistance` and `MaxDistance`. Every mode plays at full volume up to `MinDistance`.

| RollOffMode | Volume past MinDistance |
| --- | --- |
| `Inverse` | `MinDistance` divided by the distance. It stops falling at `MaxDistance`, so it never reaches silence. |
| `Linear` | Falls in a straight line to silence at `MaxDistance`. Silent beyond it. This is the default. |
| `LinearSquare` | The `Linear` value times itself. It falls faster right after `MinDistance` and reaches silence at `MaxDistance`. |
| `InverseTapered` | The quieter of `Inverse` and `LinearSquare`. It reaches silence at `MaxDistance`. |

If `MaxDistance` is not larger than `MinDistance`, luv uses a value just above `MinDistance`.

## Cones

A cone makes the speaker quieter to its sides and back. It only works when `Direction` is not zero and `ConeInnerAngle` is less than 360.

luv measures the angle between `Direction` and the line from the speaker to the listener.

| Angle | Volume |
| --- | --- |
| Up to half of `ConeInnerAngle` | Full volume. |
| From half of `ConeOuterAngle` | `ConeOuterVolume`. |
| In between | Blends from full volume to `ConeOuterVolume`. |

When `ConeOuterAngle` is smaller than `ConeInnerAngle`, luv uses `ConeInnerAngle` for both. luv multiplies the cone volume with the roll off volume.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local siren = Sound:ToSpeaker({ Spatial = true, Direction = vector.create(1, 0, 0) })
siren.ConeInnerAngle = 90
siren.ConeOuterAngle = 220
siren.ConeOuterVolume = 0.3
local alarm = Sound:SoundNode("sfx/alarm.ogg", { Looping = true })
alarm.Input:Link(siren.Output)
alarm:Play()
```

## Binaural

When `Binaural` is `true`, luv adds small differences between the two ears:

- The far ear hears the sound up to 0.66 ms later.
- The far ear hears a duller sound.
- A sound behind the listener sounds duller.
- The far ear keeps about a fifth of the volume, even when the sound is hard to one side.

When `Binaural` is `false`, luv only changes the left and right volume. A sound hard to one side plays in one ear only.

## Config

The config table of [Sound:ToSpeaker](sound-api.md#tospeaker). Its type is `ToSpeakerConfig`. Every field is optional.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Name` | `string` | `"ToSpeaker"` | The name of the node. |
| `Volume` | `number` | `1` | The volume. |
| `Device` | `string?` | `nil` | The output device. |
| `OwnedByWindow` | `boolean` | `true` | Goes quiet while the window has no focus. |
| `Spatial` | `boolean` | `false` | Places the sound in 3D. |
| `Position` | `vector` | `vector.create(0, 0, 0)` | Where the sound is. |
| `Direction` | `vector` | `vector.create(0, 0, 0)` | The way the sound faces. |
| `MinDistance` | `number` | `50` | Up to this distance the sound plays at full volume. |
| `MaxDistance` | `number` | `2000` | The distance where the volume stops falling. |
| `RollOffMode` | [RollOffMode](enums.md#rolloffmode) | `enum.RollOffMode.Linear` | How the volume falls with distance. |
| `ConeInnerAngle` | `number` | `360` | Full volume inside this angle, in degrees. |
| `ConeOuterAngle` | `number` | `360` | `ConeOuterVolume` outside this angle, in degrees. |
| `ConeOuterVolume` | `number` | `0` | The volume outside `ConeOuterAngle`. |
| `Binaural` | `boolean` | `true` | Adds small differences between the ears. |
