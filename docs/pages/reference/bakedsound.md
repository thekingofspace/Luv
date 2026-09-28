# BakedSound

Inherits: [BaseGameObject](basegameobject.md)

A sound that was played through its modifiers ahead of time. [Sound:Bake](sound-api.md#bake) makes one.

## Description

A BakedSound holds finished sound in memory. It is not a node and does not play by itself. Hand it to [Sound:SoundNode](sound-api.md#soundnode) to play it, as often and in as many nodes as you like. Every node shares the one copy.

```luau
local shot = Sound:Bake("shot.wav", {
	Modifiers = { { Kind = "Reverb", Mix = 0.3 } },
	Tail = 1,
})

local speaker = Sound:ToSpeaker()
for index = 1, 8 do
	local node = Sound:SoundNode(shot)
	node.Input:Link(speaker.Output)
end
```

You can bake a BakedSound again, to stack more modifiers on top.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `ClassName` | `string` | Always `"BakedSound"`. Read only. |
| `Name` | `string` | The name of the file it came from. |
| `Source` | `string?` | The path of the file it came from, inside `assets`. Read only. |
| `Duration` | `number` | The length in seconds, with the tail. Read only. |
| `SampleRate` | `number` | The sample rate in Hz. Read only. |
| `Channels` | `number` | `1` or `2`. Read only. |
| `Frames` | `number` | How many samples each channel holds. Read only. |
| `Peak` | `number` | The loudest sample. `1` is full volume. Read only. |
| `Memory` | `number` | How many bytes it takes up. Read only. |

## Methods

### GetBytes

```luau
baked:GetBytes(): buffer
```

The sound as a WAV file. Save it and load it next time like any other sound, so the baking only ever happens once.

```luau
local FS = import("FS")
local Process = import("Process")

local path = Process.dirs.save .. "/rain.wav"

local rain
if FS.exists(path) then
	rain = Sound:FromString(FS.readFile(path))
else
	local baked = Sound:Bake("rain.ogg", { Modifiers = { { Kind = "LowPass", Cutoff = 2000 } } })
	FS.writeFile(path, baked:GetBytes())
	rain = Sound:SoundNode(baked)
end
```

### Destroy

```luau
baked:Destroy()
```

Frees the sound. Nodes that were made from it keep playing, because each one holds its own reference. Reading a property after Destroy errors with `this BakedSound has been destroyed`.

## Memory

A baked sound is kept as 16 bit samples. One second at 48000 Hz takes 96 KB for one channel and 192 KB for two.

| Sound | Memory |
| --- | --- |
| A 1 second footstep, one channel | 94 KB |
| A 3 second explosion with a 2 second tail, two channels | 938 KB |
| A 2 minute song, two channels | 22 MB |

Bake short sounds that play often. A long song with a light filter is better played live.
