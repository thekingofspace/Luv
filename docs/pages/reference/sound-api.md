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
| `LateBlocks` | `number` | `0` | How many times the sound card ran out of sound to play. Read only. See [When sound crackles](#when-sound-crackles). |
| `SkippedBlocks` | `number` | `0` | How many times luv had no sound ready in time. Read only. See [When sound crackles](#when-sound-crackles). |
| `ClippedBlocks` | `number` | `0` | How many blocks went over full volume and had to be rounded off. Read only. See [When sound crackles](#when-sound-crackles). |
| `Load` | `number` | `0` | How much of the time luv has for each block it uses, from 0 to 1. Read only. See [When sound crackles](#when-sound-crackles). |
| `Peak` | `number` | `0` | The loudest sample since the last [ResetStats](#resetstats). `1` is full volume. Read only. |
| `Voices` | `number` | `0` | How many sounds of this window are playing. Read only. See [Voices](#voices). |
| `MaxVoices` | `number` | `0` | The most sounds this window plays at once. `0` means no limit. See [Voices](#voices). |
| `ActivationChanged` | [Signal](signal.md)`<boolean>` | none | Fires when `IsConnected` changes. See [ActivationChanged](#activationchanged). Read only. |

Setting `Volume` to NaN or infinity errors with `Volume must be a finite number`.

## When sound crackles

Sound is built in small blocks and handed to the sound card on a deadline. Miss the deadline and the card plays whatever it has, which is a click. A run of those is the crackle or static you hear. Sound that is too loud crackles too, because it gets cut off at full volume.

These numbers say which of those is happening.

| Name | What it means |
| --- | --- |
| `Load` | How much of each deadline luv spends building sound, from 0 to 1. Below about 0.5 is healthy. Near 1, blocks start to come late. |
| `LateBlocks` | The sound card asked for sound and luv was still working, so the card played a gap. This is the machine running out of room. |
| `SkippedBlocks` | luv could not reach the mixer in time. It fades out over about a millisecond instead of cutting off, so you hear a soft tick rather than a crack. |
| `ClippedBlocks` | The mix went over full volume. luv rounds it off so it does not crack, but it still sounds squashed. |
| `Peak` | The loudest sample since the last [ResetStats](#resetstats). Above `1` means the mix is too loud. |

Every one of these but `Load` only goes up until you call [ResetStats](#resetstats). [GetStats](#getstats) reads them all at once.

```luau
local Process = import("Process")
local Sound = window:GetAPI("Sound")

Process.Heartbeat:BindHandler("audio", function()
	local stats = Sound:GetStats()
	if stats.Load > 0.7 or stats.LateBlocks > 0 then
		print(`sound is too busy: {stats.Load}`)
	end
	if stats.ClippedBlocks > 0 then
		print(`sound is too loud: {stats.Peak}`)
	end
end)
```

`LateBlocks` going up while you record the screen is the machine being busy, not a fault in your game. A lighter recorder gives the deadline more room.

### What to do about it

| What you see | What helps |
| --- | --- |
| `Load` is high or `LateBlocks` goes up | [Bake](#bake) any sound that always goes through the same modifiers. Set [MaxVoices](#voices) so a burst of sounds cannot pile up. Use fewer Reverb and Echo modifiers. |
| `BusiestBlock` in [GetStats](#getstats) is much higher than `Load` | Something happens now and then that costs a lot. Often it is many sounds starting on the same frame. |
| `ClippedBlocks` goes up | Turn sounds down, or put a [SoftClip](modifiers.md#softclip) or a [Limiter](modifiers.md#limiter) right before the [ToSpeaker](tospeaker.md). [AutoGain](modifiers.md#autogain) keeps sound from players at one level. |
| A sound starts late the first time | [Preload](#preload) it, so it is decoded before it is needed. |
| `SkippedBlocks` goes up | It can go up for a moment while an output device opens. Anything more is worth reporting. |

Modifiers with nothing coming in already cost almost nothing. See [Resting](soundmodifier.md#resting).

## Voices

Every sound that plays is a voice, and each one costs the mixer some time. A burst of sounds, like fifty coins at once, can push `Load` past the deadline.

Set `MaxVoices` to cap how many sounds of this window play at once.

```luau
Sound.MaxVoices = 24
```

When a sound starts and the cap is reached, luv stops a sound that is already playing to make room. It picks the one with the lowest [Priority](soundnode.md#properties), and the oldest among those. That sound fires [Stopped](soundnode.md#stopped) like any other stop.

When every playing sound has a higher `Priority` than the new one, luv stops nothing and the new sound does not start. [Play](soundnode.md#play) returns `false` so you can tell.

```luau
local music = Sound:SoundNode("theme.ogg", { Priority = 10, Looping = true })
local coin = Sound:SoundNode("coin.wav", { Priority = 1 })
```

The music is never stopped to make room for a coin. A new coin can take the place of an older one.

Only [Play](soundnode.md#play) is checked. [PlayOneShot](soundnode.md#playoneshot) plays on top of a node that is already there, and [Resume](soundnode.md#resume) brings back a sound that already had a place. A paused sound is not a voice.

`0`, the default, means no limit. [GetStats](#getstats) reports `StolenVoices`, the sounds stopped to make room, and `RefusedVoices`, the ones that did not start.

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
| [Bake](#bake)(asset, config) | [BakedSound](bakedsound.md) | yes |
| [Preload](#preload)(...) | `number` | yes |
| [Unload](#unload)(...) | `number` | no |
| [GetStats](#getstats)() | [SoundStats](#soundstats) | no |
| [ResetStats](#resetstats)() | none | no |

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

### Bake

```luau
Sound:Bake(asset: Asset | BakedSound | string, config: BakeConfig?): BakedSound
```

Plays a sound through a list of modifiers once, ahead of time, and keeps the result as a [BakedSound](bakedsound.md). Playing it costs the same as playing any file, however many modifiers went into it. This yields the calling coroutine while it works, and the work happens off the game thread.

```luau
local explosion = Sound:Bake("explosion.wav", {
	Modifiers = {
		{ Kind = "LowPass", Cutoff = 3000 },
		{ Kind = "Reverb", RoomSize = 0.8, Mix = 0.4 },
		{ Kind = "SoftClip" },
	},
	Tail = 2,
	Normalize = -1,
})

local boom = Sound:SoundNode(explosion)
boom.Input:Link(Sound:ToSpeaker().Output)
boom:Play()
```

Use it for any sound that always goes through the same modifiers. A gunshot with its room, a footstep with its filter, a voice line with its radio sound. See [Baking sounds](../manual/sound.md#baking-sounds).

The config, of type `BakeConfig`. Every field is optional.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Modifiers` | `{ BakeModifier }` | none | The modifiers, in order. Each one is a table with `Kind` and any values of that kind, like `{ Kind = "Echo", Delay = 0.2 }`. Every kind in the [Modifier list](modifiers.md) works. |
| `Tail` | `number` | `0` | Seconds to keep going after the sound ends, so a Reverb or an Echo can ring out. 0 to 60. |
| `Normalize` | `number` | none | Turns the result up or down so its loudest sample lands on this level, in dB. `-1` is just under full volume. -60 to 0. |
| `Volume` | `number` | `1` | The volume of the sound going in. 0 to 10. |
| `Speed` | `number` | `1` | How fast the sound goes in. It changes the pitch too, like `PlaybackSpeed`. 0.01 to 32. |
| `Start` | `number` | `0` | Where in the sound to start, in seconds. |
| `Length` | `number` | none | How many seconds of the sound to use. It fades out over 10 ms at the cut, so it does not click. |
| `Channels` | `number` | none | `1` or `2`. Leave it out and luv keeps one channel when both sides came out the same, which halves the memory. |
| `SampleRate` | `number` | `Sound.SampleRate` | The sample rate of the result. Leave it out, so nothing has to be converted while it plays. |

Silence at the end is trimmed off, so a long `Tail` costs nothing once the ring has died away. A baked sound can be at most 600 seconds long.

In a test with 24 sounds playing at once, each through a Reverb, an Echo and a Chorus, the mixer used 16.5 percent of its time. Baked, the same 24 sounds used 1.8 percent.

| Message | Cause |
| --- | --- |
| `'<kind>' is not a sound modifier` | A `Kind` is not in the [Modifier list](modifiers.md). |
| `Modifiers[<n>] needs a Kind, like "Reverb"` | An entry has no `Kind`. |
| `<name> is not a valid member of <kind>` | An entry sets a value that kind does not have. |
| `Channels must be 1 or 2` | `Channels` is something else. |
| `Length must be a number of seconds above 0` | `Length` is 0 or less. |
| `cannot bake the sound: a baked sound can be at most 600 seconds long` | The sound and its tail run longer than that. |

### Preload

```luau
Sound:Preload(...: Asset | string): number
```

Decodes sounds before you need them and keeps them ready. Returns how many seconds of sound that was. This yields the calling coroutine while it works.

Decoding a large file takes time, and doing it the moment a sound should play makes it start late. Preload what a level needs while the level loads.

```luau
Sound:Preload("music/forest.ogg", "coin.wav", "jump.wav")
```

Every [SoundNode](#soundnode) for those files then starts at once, and they all share the one decoded copy. The sounds stay decoded until you [Unload](#unload) them or the window closes.

### Unload

```luau
Sound:Unload(...: string): number
```

Lets go of sounds that [Preload](#preload) kept, and returns how many it let go. Pass the paths you preloaded, with or without the extension. Pass nothing to let go of all of them.

A node that still uses a sound keeps it. Unload only stops Preload from holding on.

### GetStats

```luau
Sound:GetStats(): SoundStats
```

Reads every number about how the mixer is doing at once. See [SoundStats](#soundstats) and [When sound crackles](#when-sound-crackles).

### ResetStats

```luau
Sound:ResetStats()
```

Sets `Peak`, `BusiestBlock`, `ClippedBlocks`, `LateBlocks`, `SkippedBlocks`, `StolenVoices` and `RefusedVoices` back to 0. Call it just before a part of the game you want to measure.

Every window mixes into the same device, so the numbers of the mixer are shared. Resetting them in one window resets them in all. `StolenVoices` and `RefusedVoices` belong to each window.

## SoundStats

What [GetStats](#getstats) returns.

| Name | Type | Description |
| --- | --- | --- |
| `Load` | `number` | How much of the time luv has for each block it uses, on average, from 0 to 1. |
| `BusiestBlock` | `number` | The slowest single block since the last ResetStats, on the same scale as `Load`. Above 1 means that block was late. |
| `Peak` | `number` | The loudest sample since the last ResetStats, before it is rounded off. |
| `ClippedBlocks` | `number` | Blocks that went over full volume. |
| `LateBlocks` | `number` | The same as the [property](#properties). |
| `SkippedBlocks` | `number` | The same as the [property](#properties). |
| `Nodes` | `number` | Every node luv is mixing, in every window. |
| `RestingNodes` | `number` | Modifiers that rest because nothing comes in. See [Resting](soundmodifier.md#resting). |
| `Voices` | `number` | Sounds of this window playing now. |
| `MaxVoices` | `number` | The cap. `0` means none. |
| `StolenVoices` | `number` | Sounds stopped to make room. |
| `RefusedVoices` | `number` | Sounds that did not start because of the cap. |
| `SampleRate` | `number` | The sample rate of the mixer in Hz. |

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
