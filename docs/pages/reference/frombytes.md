# FromBytes

Inherits: [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Plays sound samples that you push into it.

## Description

You make a FromBytes node with [Sound:FromBytes](sound-api.md#frombytes). Link its `Input` to a [ToSpeaker](tospeaker.md). Then [Push](#push) samples into it. It starts to play on its own once it has enough sound. It has no Play or Stop method.

Use it for sound that your game makes while it runs, or for sound that comes in over the network. The packets of a [ToBytes](tobytes.md) node play in FromBytes as they are.

It also has every member of [NodeObject](nodeobject.md).

## Properties

| Name | Type | Default | Range | Description |
| --- | --- | --- | --- | --- |
| `Input` | [NodeInput](nodeports.md) | none | none | Sends the sound on. Read only. |
| `Volume` | `number` | `1` | 0 to 10 | The volume. Changes glide over 20 ms. |
| `MaxBuffered` | `number` | `1` | 0.02 to 60 | The most sound it keeps waiting, in seconds. See [Buffering](#buffering). |
| `Prebuffer` | `number` | `0.05` | 0 to 10 | How much sound it waits for before it starts, in seconds. |
| `SampleRate` | `number` | `48000` | 1000 to 384000 | The sample rate of raw samples you push, in Hz. Whole numbers only. |
| `Channels` | `number` | `2` | 1 to 2 | The number of channels in raw samples you push. |
| `Format` | [AudioFormat](enums.md#audioformat) | `enum.AudioFormat.Float32` | none | The sample format of raw samples you push. |
| `Buffered` | `number` | `0` | none | How many seconds of pushed sound wait to be played. Read only. |
| `IsPlaying` | `boolean` | `false` | none | `true` while it plays sound. Read only. |

## Buffering

FromBytes keeps the sound you push in a queue and plays it in order.

- It starts once the queue holds `Prebuffer` seconds, or half of `MaxBuffered` if that is less. With a `Prebuffer` of `0`, it starts as soon as any sound arrives.
- When the queue runs empty, it fades out over 10 ms and fires [Drained](#drained). Then it waits for `Prebuffer` seconds again before it goes on.
- When the queue holds more than `MaxBuffered` seconds, luv drops the oldest pushes. It drops whole pushes and never the newest one. This keeps a live stream close to real time. Raise `MaxBuffered` to queue more sound.
- `Buffered` changes right away when you push or clear.
- The queue plays in real time even when nothing is linked to the node.

Each push keeps its own sample rate. You can change `SampleRate` between pushes. luv converts every push to the rate of the mixer.

## Push formats

| What you push | How luv reads it |
| --- | --- |
| A string or buffer from [ToString](audiopacket.md#tostring) or [ToBuffer](audiopacket.md#tobuffer) of an [AudioPacket](audiopacket.md) | luv sees the `LUVA` header. It uses the sample rate, channels and format stored in the packet. `SampleRate`, `Channels` and `Format` of the node are ignored. |
| An [AudioPacket](audiopacket.md) | The same as above. |
| Any other string or buffer | Raw samples in the `SampleRate`, `Channels` and `Format` of the node. |

Raw samples have no header. Every number is little endian. With 2 channels the samples take turns: left, right, left, right. The data must hold whole frames. A frame is one sample for each channel.

| Format | Bytes per sample | Values |
| --- | --- | --- |
| `Float32` | 4 | 32 bit floats. Full volume is -1 to 1. NaN and infinity play as 0. |
| `Int16` | 2 | Signed 16 bit whole numbers from -32768 to 32767. |

A mono stream plays on both sides.

Push does not take sound files like WAV or MP3. Use [Sound:FromString](sound-api.md#fromstring) for those.

## Methods

### Push

```luau
stream:Push(data: string | buffer | AudioPacket)
```

Adds sound to the end of the queue. See [Push formats](#push-formats). Empty data is ignored.

It errors when:

| Message | Cause |
| --- | --- |
| `Push expects a string, a buffer or an AudioPacket, got number` | `data` has another type. |
| `raw audio must be whole frames of 8 bytes (Float32 2 channels), got 3 bytes` | The raw data does not hold whole frames. |
| `the audio packet uses version 2, only version 1 is supported` | The packet header has another version. |
| `the audio packet uses an unknown sample format 7` | The format byte of the header is not 0 or 1. |
| `audio packets hold 1 or 2 channels, this one says 3` | The channels byte of the header is wrong. |
| `the audio packet has an unsupported sample rate of 500` | The sample rate in the header is not 1000 to 384000. |
| `the audio packet should hold 3840 bytes of samples but holds 100` | The packet is cut short or has extra bytes. |

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

### Clear

```luau
stream:Clear()
```

Drops everything in the queue. If it was playing, it fades out over 10 ms. `Buffered` becomes `0`. It does not fire [Drained](#drained).

## Signals

### Drained

```luau
stream.Drained: Signal<()>
```

Fires when the queue runs empty while it plays. It fires again each time that happens. It comes from the audio engine, so it fires a few milliseconds after the sound runs out.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local stream = Sound:FromBytes({ Channels = 1 })
stream.Input:Link(Sound:ToSpeaker())
local chunk = buffer.create(4800 * 4)
for index = 0, 4799 do
	buffer.writef32(chunk, index * 4, 0.2 * math.sin(index / 48000 * 2 * math.pi * 220))
end
stream.Drained:BindHandler("done", function()
	print("The stream ran out")
end)
stream:Push(chunk)
```

## Config

The config table of [Sound:FromBytes](sound-api.md#frombytes). Its type is `FromBytesConfig`. Every field is optional.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Name` | `string` | `"FromBytes"` | The name of the node. |
| `Volume` | `number` | `1` | The volume. |
| `MaxBuffered` | `number` | `1` | The most sound it keeps waiting, in seconds. |
| `Prebuffer` | `number` | `0.05` | How much sound it waits for before it starts, in seconds. |
| `SampleRate` | `number` | `48000` | The sample rate of raw samples, in Hz. |
| `Channels` | `number` | `2` | The number of channels in raw samples. |
| `Format` | [AudioFormat](enums.md#audioformat) | `enum.AudioFormat.Float32` | The sample format of raw samples. |
