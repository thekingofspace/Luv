# Sound

luv plays sound through nodes that you link together. This page shows how to use them. The [Sound API](../reference/sound-api.md) page lists every function.

## Getting the Sound API

Each window has its own Sound API. Get it from the window with [GetAPI](../reference/window.md#getapi):

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")
```

Every example on this page uses these lines.

## How nodes connect

There are three groups of nodes:

| Group | Nodes | Ports |
| --- | --- | --- |
| Make sound | [SoundNode](../reference/soundnode.md), [FromString](../reference/soundnode.md#fromstring), [FromBytes](../reference/frombytes.md) | `Input` |
| Change sound | Every kind in the [Modifier list](../reference/modifiers.md) | `Input` and `Output` |
| Take sound out | [ToSpeaker](../reference/tospeaker.md), [ToBytes](../reference/tobytes.md) | `Output` |

Sound flows from the node that owns the Input to the node that owns the Output. So `a.Input:Link(b.Output)` sends the sound of `a` into `b`. A modifier takes sound in through its `Output` and sends it on through its `Input`.

This tree shows a typical chain. Each node sends its sound to the node above it.

```tree
speaker (ToSpeaker)
└── reverb (Reverb)
    ├── music (SoundNode)
    └── click (FromString)
```

Here is the code for that chain:

```luau
local Asset = import("Asset")
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local speaker = Sound:ToSpeaker()
local reverb = Sound:Modifier("Reverb", { Mix = 0.2 })
local music = Sound:SoundNode("music/theme.ogg")
local click = Sound:FromString(Asset.LoadString("sfx/click.wav"))
reverb.Input:Link(speaker.Output)
music.Input:Link(reverb.Output)
click.Input:Link(reverb.Output)
```

An Output adds together everything linked into it. An Input can send its sound to many Outputs at once. See [NodeInput and NodeOutput](../reference/nodeports.md) for every rule.

## Playing a sound

[Sound:SoundNode](../reference/sound-api.md#soundnode) loads a file from the `assets` folder. It yields while luv decodes the file. See [Yielding and coroutines](yielding.md).

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local music = Sound:SoundNode("music/theme.ogg", { Volume = 0.6 })
music.Input:Link(Sound:ToSpeaker())
music.Ended:BindHandler("done", function()
	print("The song is over")
end)
music:Play()
```

- `Play` starts the sound. Calling it again starts over from the beginning.
- `Pause` keeps the position and `Resume` goes on from it.
- `Stop` ends the sound and goes back to the start.
- `PlayPosition` reads or moves the position in seconds.
- `Ended` fires when the sound reaches its end.

A node with no link still plays. You just do not hear it. The window keeps every node alive, so call `Destroy` on nodes you no longer need.

## One shots

`PlayOneShot` plays the whole file again on top of what already plays. Use it for hits, steps and other sounds that can overlap. It does not change `IsPlaying` and fires no signals.

[Sound:FromString](../reference/sound-api.md#fromstring) makes a node from a file that is already in memory. It works well for sounds your game builds or downloads.

```luau
local Asset = import("Asset")
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local hit = Sound:FromString(Asset.LoadString("sfx/hit.wav"), { Volume = 0.8 })
hit.Input:Link(Sound:ToSpeaker())
hit:PlayOneShot()
hit:PlayOneShot(0.5)
```

A node plays at most 8 one shots at once. See [PlayOneShot](../reference/soundnode.md#playoneshot).

## Looping

Set `Looping` to `true` to repeat a sound. `LoopStart` and `LoopEnd` pick the part that repeats. The part before `LoopStart` plays once, so it works as an intro.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local theme = Sound:SoundNode("music/theme.ogg", { Looping = true, LoopStart = 8.25 })
theme.Input:Link(Sound:ToSpeaker())
theme.Looped:BindHandler("count", function(count: number)
	print(`Loop {count}`)
end)
theme:Play()
```

`LoopEnd` of `0` means the end of the file. `Looped` fires each time the sound jumps back.

## Adding modifiers

Put modifiers between a sound and a speaker. You can change their values at any time.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local music = Sound:SoundNode("music/theme.ogg", { Looping = true })
local muffle = Sound:Modifier("LowPass", { Cutoff = 20000 })
local fader = Sound:Modifier("Gain", { Volume = 0 })
music.Input:Link(muffle.Output)
muffle.Input:Link(fader.Output)
fader.Input:Link(Sound:ToSpeaker())
music:Play()
fader:Fade(1, 2)
```

Set `muffle.Cutoff` to `500` later to muffle the music. `Fade` moves the volume of a Gain over time. Set `Enabled` to `false` to skip a modifier without unlinking it.

The [Modifier list](../reference/modifiers.md) has every kind with its values.

## Baking sounds

Every modifier works on every block while its sound plays. A gunshot through a Reverb costs the Reverb each time it fires, and twenty gunshots cost twenty Reverbs.

When a sound always goes through the same modifiers, bake it. [Sound:Bake](../reference/sound-api.md#bake) plays it through them once, ahead of time, and gives you a [BakedSound](../reference/bakedsound.md). Playing that costs no more than playing the file.

```luau
local shot = Sound:Bake("shot.wav", {
	Modifiers = {
		{ Kind = "HighPass", Cutoff = 120 },
		{ Kind = "Reverb", RoomSize = 0.7, Mix = 0.3 },
	},
	Tail = 1.5,
	Normalize = -3,
})

local speaker = Sound:ToSpeaker()
local function fire()
	local node = Sound:SoundNode(shot)
	node.Input:Link(speaker.Output)
	node:Play()
end
```

Bake while the level loads, since it yields until it is done. Bake a few versions for variety, like the same shot at `Speed = 0.9`, `1` and `1.1`.

Keep modifiers live when they have to change while the sound plays, like a muffle that follows the player behind a wall.

## Keeping sound smooth

Crackle comes from two places. The mixer runs out of time, or the mix gets too loud. [Sound:GetStats](../reference/sound-api.md#getstats) tells you which. See [When sound crackles](../reference/sound-api.md#when-sound-crackles).

What helps the most:

- Bake sounds that always go through the same modifiers.
- Set [MaxVoices](../reference/sound-api.md#voices) so a burst of sounds cannot pile up.
- Put a [SoftClip](../reference/modifiers.md#softclip) or a [Limiter](../reference/modifiers.md#limiter) right before each speaker.
- [Preload](../reference/sound-api.md#preload) the sounds of a level while it loads.

```luau
Sound.MaxVoices = 32
Sound:Preload("coin.wav", "jump.wav", "music/forest.ogg")

local guard = Sound:Modifier("SoftClip", { Ceiling = -1 })
guard.Input:Link(Sound:ToSpeaker().Output)
```

Link everything to `guard.Output` instead of to the speaker, and nothing can reach full volume.

You do not need to unlink modifiers that wait for a sound. A modifier with nothing coming in rests and costs almost nothing. See [Resting](../reference/soundmodifier.md#resting).

## 3D sound

Set `Spatial` to `true` on a [ToSpeaker](../reference/tospeaker.md). Then its `Position` and [Sound.Listener](../reference/sound-api.md#soundlistener) decide the side and the volume. Positions are native Luau vectors.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local engine = Sound:SoundNode("sfx/engine.ogg", { Looping = true })
local car = Sound:ToSpeaker({ Spatial = true, MinDistance = 100, MaxDistance = 3000 })
engine.Input:Link(car.Output)
engine:Play()
Sound.Listener.Position = vector.create(640, 360, 0)
local x = 0
window.PreFrame:BindHandler("drive", function(dt: number)
	x += 200 * dt
	car.Position = vector.create(x, 360, 0)
end)
```

With the default listener, positive X is to the right. In a 2D game, keep every Z at 0. `MinDistance`, `MaxDistance` and `RollOffMode` decide how the volume falls with distance. See [Roll off](../reference/tospeaker.md#roll-off).

> [!TIP]
> A ToSpeaker has one `Position` for everything linked into it. Moving it also moves the sounds that are still ringing through it. Give each sound its own ToSpeaker when they need their own places.

## Output devices

A ToSpeaker plays on the default output device. Set its `Device` to a name from [Sound:GetDevices](../reference/sound-api.md#getdevices) to pick another one. If that device is missing, the sound plays on the default device until the device shows up. `Sound.DefaultDevice` holds the name of the default device.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local chat = Sound:ToSpeaker({ OwnedByWindow = false })
if Sound:DeviceExists("Headphones (USB Audio)") then
	chat.Device = "Headphones (USB Audio)"
end
Sound.ActivationChanged:BindHandler("output", function(connected: boolean)
	print(if connected then "Sound is back" else "No output device")
end)
```

`ActivationChanged` fires when the last output device goes away and when one comes back.

## Streaming with FromBytes

A [FromBytes](../reference/frombytes.md) node plays samples that you push. It starts on its own once it has enough sound. It fires `Drained` when it runs out.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local tone = Sound:FromBytes({ SampleRate = 22050, Channels = 1, Format = enum.AudioFormat.Int16 })
tone.Input:Link(Sound:ToSpeaker())
local parts: { string } = {}
for index = 0, 22049 do
	parts[#parts + 1] = string.pack("<i2", math.floor(12000 * math.sin(index / 22050 * 2 * math.pi * 440)))
end
tone:Push(table.concat(parts))
```

Raw samples use the `SampleRate`, `Channels` and `Format` of the node. See [Push formats](../reference/frombytes.md#push-formats).

## Capturing with ToBytes

A [ToBytes](../reference/tobytes.md) node cuts the sound linked into it into [AudioPacket](../reference/audiopacket.md) values. This sends a sound over UDP:

```luau
local Net = import("Net")
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local voice = Sound:SoundNode("voice/hello.ogg")
local capture = Sound:ToBytes({ SampleRate = 16000, Channels = 1, PacketDuration = 0.04 })
voice.Input:Link(capture.Output)
local socket = Net.UdpBind()
capture.OnIncoming:BindHandler("send", function(packet: AudioPacket)
	socket:Send(packet:ToBuffer(), "127.0.0.1", 40000)
end)
voice:Play()
```

The other side pushes each packet into a FromBytes node. Packets carry their own format, so FromBytes needs no settings for them.

```luau
local Net = import("Net")
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local incoming = Sound:FromBytes({ Prebuffer = 0.08, MaxBuffered = 0.3 })
incoming.Input:Link(Sound:ToSpeaker({ OwnedByWindow = false }))
local socket = Net.UdpBind(40000)
socket.Received:BindHandler("play", function(data: string)
	incoming:Push(data)
end)
```

See [UdpSocket](../reference/udpsocket.md) for the socket side. ToBytes does not play anything. To hear a sound and send it, link it to a ToSpeaker too.

## Focus and window close

- By default, a ToSpeaker goes quiet while its window does not have focus. The sounds keep playing. Set `OwnedByWindow` to `false` on speakers that should stay on, like music or voice chat.
- To stop time instead, call `PauseAll` when focus is lost and `ResumeAll` when it comes back.
- When a window closes, luv fades its sound out and destroys all of its nodes. After that, the Sound API of the window errors.

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

See [Windows and frames](windows.md) for more about focus and closing.

## Supported file formats

[Sound:SoundNode](../reference/sound-api.md#soundnode) and [Sound:FromString](../reference/sound-api.md#fromstring) read these formats:

| Format | Notes |
| --- | --- |
| WAV and AIFF | |
| MP3 | |
| FLAC | |
| OGG | With Vorbis or Opus sound, like `.ogg` and `.opus` files. |
| M4A and MP4 | With AAC or ALAC sound. |
| CAF | |
| MKV and WebM | |

- luv decodes the whole file when it loads. It keeps the sound in memory as 16 bit samples.
- A sound can be up to about 46 minutes long in stereo at 48000 Hz. Longer files error with `the sound is too long to load into memory`.
- A file with more than two channels keeps the first two.
- Opus sound must be mono or stereo.
- SoundNode uses the file extension as a hint. FromString finds the format from the data alone.
- If luv cannot read a file, SoundNode errors with a message that starts with `cannot load sound`.
