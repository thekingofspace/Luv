# Sound API

Plays, changes and records sound for one window.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")
```

## Description

Each window has its own Sound API. You get it with [GetAPI](window.md#getapi). Asking again on the same window gives the same object.

Sound in luv moves through nodes. A [SoundNode](soundnode.md) plays a file. A [ToSpeaker](tospeaker.md) plays sound on an output device. You connect nodes through their [ports](nodeports.md). The [Sound](../manual/sound.md) guide shows how the parts fit together.

luv starts its audio engine the first time a window asks for the Sound API. The output device stays open while any window has a Sound API. luv closes it about one second after the last of those windows closes.

[Bulk.BulkUpdate](bulk.md) can set `Volume` on the Sound API and the fields of `Listener`.

## Properties

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `Volume` | `number` | `1` | The master volume of this window. It scales every [ToSpeaker](tospeaker.md) of the window. It does not change what a [ToBytes](tobytes.md) node gets. Clamped to 0 to 10. Changes glide over 20 ms. |
| `Listener` | [SoundListener](#soundlistener) | none | Where the player hears 3D sound from. Read only. |
| `SampleRate` | `number` | `48000` | The sample rate of the mixer in Hz. It changes to the rate of the default output device when that device opens. Read only. |
| `DefaultDevice` | `string?` | none | The name of the default output device. `nil` when there is none. It can also be `nil` for a moment right after the first `GetAPI("Sound")`. Read only. |
| `IsConnected` | `boolean` | `true` | `true` while an output device exists. luv checks about every 2 seconds. Read only. |
| `ActivationChanged` | [Signal](signal.md)`<boolean>` | none | Fires when `IsConnected` changes. See [ActivationChanged](#activationchanged). Read only. |

Setting `Volume` to NaN or infinity errors with `Volume must be a finite number`.

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [SoundNode](#soundnode)(asset, config) | [SoundNode](soundnode.md) | yes |
| [FromString](#fromstring)(data, config) | [FromString](soundnode.md#fromstring) | yes |
| [FromBytes](#frombytes)(config) | [FromBytes](frombytes.md) | no |
| [ToSpeaker](#tospeaker)(config) | [ToSpeaker](tospeaker.md) | no |
| [ToBytes](#tobytes)(config) | [ToBytes](tobytes.md) | no |
| [Modifier](#modifier)(kind, config) | [SoundModifier](soundmodifier.md) | no |
| [GetNodes](#getnodes)() | `{ NodeObject }` | no |
| [StopAll](#stopall)() | none | no |
| [PauseAll](#pauseall)() | none | no |
| [ResumeAll](#resumeall)() | none | no |
| [GetDevices](#getdevices)() | `{ string }` | yes |
| [DeviceExists](#deviceexists)(name) | `boolean` | yes |

Every function is a method. Call it with `:`, like `Sound:ToSpeaker()`.

## Function descriptions

### SoundNode

```luau
Sound:SoundNode(asset: Asset | string, config: SoundNodeConfig?): SoundNode
```

Loads a sound file and returns a new [SoundNode](soundnode.md). `asset` is an [Asset](asset.md) or a path inside the `assets` folder. Write `"sfx/jump.wav"`, not `"assets/sfx/jump.wav"`. You can leave out the extension when only one file has that name. `config` sets properties of the new node. See [SoundNodeConfig](soundnode.md#config).

This yields the calling coroutine while luv decodes the file. The new node is stopped.

Every SoundNode made from the same file shares one decoded copy. luv frees the copy when no node uses it anymore.

It errors when:

- `asset` is not an Asset or a string. The message is `SoundNode expects an Asset or an asset path, got <type>`.
- The file is missing. The message is `cannot load asset '<path>': no such asset`.
- A path without an extension matches more than one file. The message says `the name is ambiguous`.
- luv cannot read the file as sound. The message starts with `cannot load sound '<path>'`.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local jump = Sound:SoundNode("sfx/jump.wav", { Volume = 0.8 })
jump.Input:Link(Sound:ToSpeaker())
jump:Play()
```

### FromString

```luau
Sound:FromString(data: string | buffer, config: SoundNodeConfig?): FromString
```

Decodes a sound file that is already in memory and returns a new node. `data` holds the bytes of a whole file, like a WAV or MP3 file. It does not take raw samples. Use [FromBytes](#frombytes) for those.

The new node has every member of a SoundNode. See [FromString](soundnode.md#fromstring) for the small differences. This yields the calling coroutine while luv decodes the data.

It errors when:

- `data` is not a string or a buffer. The message is `FromString expects the encoded sound as a string or a buffer, got <type>`.
- luv cannot read the data as sound. The message starts with `cannot load the sound`.

```luau
local Asset = import("Asset")
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local click = Sound:FromString(Asset.LoadString("sfx/click.wav"))
click.Input:Link(Sound:ToSpeaker())
click:PlayOneShot()
```

### FromBytes

```luau
Sound:FromBytes(config: FromBytesConfig?): FromBytes
```

Returns a new [FromBytes](frombytes.md) node. It plays samples that you push into it. `config` sets its properties. See [FromBytesConfig](frombytes.md#config).

### ToSpeaker

```luau
Sound:ToSpeaker(config: ToSpeakerConfig?): ToSpeaker
```

Returns a new [ToSpeaker](tospeaker.md) node. It plays the sound linked into it on an output device. `config` sets its properties. See [ToSpeakerConfig](tospeaker.md#config).

### ToBytes

```luau
Sound:ToBytes(config: ToBytesConfig?): ToBytes
```

Returns a new [ToBytes](tobytes.md) node. It turns the sound linked into it into [AudioPacket](audiopacket.md) values. `config` sets its properties. See [ToBytesConfig](tobytes.md#config).

### Modifier

```luau
Sound:Modifier(kind: string, config: { [string]: any }?): SoundModifier
```

Returns a new [SoundModifier](soundmodifier.md) of the given kind. The [Modifier list](modifiers.md) has every kind and its values. Kind names are case sensitive. The type of the result matches the kind, so `Sound:Modifier("Gain")` gives a `GainModifier`. `config` sets properties of the new modifier.

An unknown kind errors with a message that starts with `'<kind>' is not a sound modifier` and lists every kind.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local music = Sound:SoundNode("music/theme.ogg")
local echo = Sound:Modifier("Echo", { Delay = 0.25, Mix = 0.3 })
music.Input:Link(echo.Output)
echo.Input:Link(Sound:ToSpeaker())
music:Play()
```

### GetNodes

```luau
Sound:GetNodes(): { NodeObject }
```

Returns every node of this window that is not destroyed. The oldest node comes first.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

for _, node in Sound:GetNodes() do
	print(node.ClassName, node.Name)
end
```

### StopAll

```luau
Sound:StopAll()
```

Calls [Stop](soundnode.md#stop) on every SoundNode and FromString of this window. [FromBytes](frombytes.md) nodes keep playing. It also forgets the nodes that [PauseAll](#pauseall) paused.

### PauseAll

```luau
Sound:PauseAll()
```

Calls [Pause](soundnode.md#pause) on every SoundNode and FromString of this window that is playing. luv remembers which nodes it paused.

### ResumeAll

```luau
Sound:ResumeAll()
```

Resumes the nodes that [PauseAll](#pauseall) paused, if they are still paused. Nodes that you paused yourself stay paused.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

window.FocusLost:BindHandler("pause", function()
	Sound:PauseAll()
end)
window.FocusGained:BindHandler("resume", function()
	Sound:ResumeAll()
end)
```

### GetDevices

```luau
Sound:GetDevices(): { string }
```

Returns the name of every output device. This yields the calling coroutine while luv asks the system.

### DeviceExists

```luau
Sound:DeviceExists(name: string): boolean
```

Returns `true` if an output device has exactly this name. The name is case sensitive. This yields the calling coroutine.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local voice = Sound:ToSpeaker()
if Sound:DeviceExists("Headphones (USB Audio)") then
	voice.Device = "Headphones (USB Audio)"
end
```

## Signals

### ActivationChanged

```luau
Sound.ActivationChanged: Signal<boolean>
```

Fires when `IsConnected` changes. The argument is the new value. It only fires for changes that happen after you got the Sound API.

## SoundListener

`Sound.Listener` is where the player hears 3D sound from. Each window has one. Every [ToSpeaker](tospeaker.md) with `Spatial` set to `true` uses it.

| Name | Type | Description |
| --- | --- | --- |
| `Position` | `vector` | Where the listener is. Starts at `vector.create(0, 0, 0)`. A [UDim](udim.md) also works. luv reads its X, Y and Z. |
| `Forward` | `vector` | The way the listener faces. Starts at `vector.create(0, 0, -1)`. |
| `Up` | `vector` | Which way is up for the listener. Starts at `vector.create(0, 1, 0)`. |

- Every field reads back as a `vector`.
- `Forward` and `Up` must be vectors that are not zero. They do not need a length of 1.
- With the default `Forward` and `Up`, positive X is to the right of the listener.
- In a 2D game, keep every Z at 0.

| Message | Cause |
| --- | --- |
| `Forward must be a vector, got string` | The value is not a vector. |
| `Forward cannot be a zero vector` | `Forward` or `Up` is zero. |
| `Position must hold finite numbers` | A part of the value is NaN or infinity. |

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

Sound.Listener.Position = vector.create(480, 300, 0)
Sound.Listener.Forward = vector.create(0, 0, -1)
```

## Window ownership and focus

- Every node belongs to the window whose Sound API made it. Nodes of two windows cannot be linked.
- A [ToSpeaker](tospeaker.md) with `OwnedByWindow` set to `true` goes quiet while its window does not have focus. It fades out over 50 ms and fades back in when focus returns. The sounds keep playing while it is quiet.
- To stop time in the background, call [PauseAll](#pauseall) when focus is lost. Call [ResumeAll](#resumeall) when it comes back.
- When the window closes, luv fades its sound out over 20 ms and destroys all of its nodes. See [What happens when a window closes](window.md#what-happens-when-a-window-closes).
- After the window closes, every call on the Sound API errors with `this Sound API belongs to a window that is closed`. `window:GetAPI("Sound")` errors with `the Sound API is not available because the window is closed`.
