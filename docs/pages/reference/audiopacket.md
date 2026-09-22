# AudioPacket

A short piece of sound made by a [ToBytes](tobytes.md) node.

## Description

[ToBytes](tobytes.md) fires [OnIncoming](tobytes.md#onincoming) with one AudioPacket at a time. A packet never changes, so you can keep it as long as you like.

You can push a packet into a [FromBytes](frombytes.md) node. You can also turn it into bytes with [ToString](#tostring) or [ToBuffer](#tobuffer). Send the bytes anywhere and push them into a FromBytes node there. The bytes carry the format of the packet, so FromBytes needs no settings for them.

`#packet` gives `Frames`. `tostring(packet)` gives text like `"AudioPacket(960 frames, 48000 Hz, 2 channels)"`.

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `SampleRate` | `number` | The sample rate in Hz. Read only. |
| `Channels` | `number` | `1` or `2`. Read only. |
| `Frames` | `number` | The number of frames. A frame is one sample for each channel. Read only. |
| `Duration` | `number` | The length in seconds. This is `Frames` divided by `SampleRate`. Read only. |
| `Format` | [AudioFormat](enums.md#audioformat) | The sample format. Read only. |
| `Sequence` | `number` | The number of the packet. See [Packet timing](tobytes.md#packet-timing). Read only. |
| `Peak` | `number` | The loudest sample. `1` is full volume. It can go above `1` when the sound is louder than that. Read only. |
| `Loudness` | `number` | The average level (RMS) of the samples. Read only. |

luv measures `Peak` and `Loudness` on the stereo sound. That happens before it mixes to mono or rounds to `Int16`.

## Methods

### ToString

```luau
packet:ToString(): string
```

Returns the packet as bytes in a string. See [Byte layout](#byte-layout).

### ToBuffer

```luau
packet:ToBuffer(): buffer
```

Returns the same bytes as [ToString](#tostring) in a buffer.

### GetSamples

```luau
packet:GetSamples(channel: number?): { number }
```

Returns the samples as numbers. With no channel, it returns every sample in order: left, right, left, right. With a channel, it returns only that channel. Channel `1` is the left channel and channel `2` is the right channel. `Int16` samples come back as numbers from -1 to 1.

A channel that the packet does not have errors, for example with `the packet has 1 channel, channel 2 does not exist`.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local music = Sound:SoundNode("music/theme.ogg")
local bytes = Sound:ToBytes({ Format = enum.AudioFormat.Float32 })
music.Input:Link(bytes.Output)
bytes.OnIncoming:BindHandler("inspect", function(packet: AudioPacket)
	local left = packet:GetSamples(1)
	print(tostring(packet), packet.Duration, #left, packet.Loudness)
end)
music:Play()
```

## Byte layout

[ToString](#tostring) and [ToBuffer](#tobuffer) give a 20 byte header and then the samples. Every number is little endian.

| Bytes | Type | Value |
| --- | --- | --- |
| 0 to 3 | 4 characters | Always `LUVA`. |
| 4 | u8 | The version. Always `1`. |
| 5 | u8 | The format. `0` is `Float32` and `1` is `Int16`. |
| 6 | u8 | The number of channels, `1` or `2`. |
| 7 | u8 | Always `0`. |
| 8 to 11 | u32 | The sample rate in Hz. |
| 12 to 15 | u32 | The number of frames. |
| 16 to 19 | u32 | The sequence number. |
| 20 and on | samples | The samples in the packet format. With 2 channels they take turns: left, right. |

A `Float32` packet takes `20 + Frames * Channels * 4` bytes. An `Int16` packet takes `20 + Frames * Channels * 2` bytes. The default ToBytes packet has 960 frames, 2 channels and `Int16` samples. It takes 3860 bytes.

`string.unpack` can read the header:

```luau
local function describe(data: string): string
	local magic, version, format, channels, _, rate, frames, sequence = string.unpack("<c4BBBBI4I4I4", data)
	return `{magic} {version} {format} {channels} {rate} {frames} {sequence}`
end
```
