# ToBytes

Inherits: [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Turns the sound linked into it into packets of samples.

## Description

You make a ToBytes node with [Sound:ToBytes](sound-api.md#tobytes). Link nodes into its `Output`. It cuts their sound into [AudioPacket](audiopacket.md) values and fires [OnIncoming](#onincoming) with each one.

Use it to send sound over the network, to save sound, or to look at the samples. A [FromBytes](frombytes.md) node can play the packets.

ToBytes does not play anything. To hear a sound and record it, link the sound to a [ToSpeaker](tospeaker.md) and to a ToBytes node. The master [Volume](sound-api.md#properties) and the focus of the window do not change what ToBytes gets.

It also has every member of [NodeObject](nodeobject.md).

## Properties

| Name | Type | Default | Range | Description |
| --- | --- | --- | --- | --- |
| `Output` | [NodeOutput](nodeports.md) | none | none | Takes sound in. Read only. |
| `Enabled` | `boolean` | `true` | none | When `false`, it makes no packets and drops the samples it was collecting. |
| `SampleRate` | `number` | `48000` | 1000 to 384000 | The sample rate of the packets in Hz. Whole numbers only. |
| `Channels` | `number` | `2` | 1 to 2 | The number of channels in the packets. `1` mixes left and right together. |
| `Format` | [AudioFormat](enums.md#audioformat) | `enum.AudioFormat.Int16` | none | The sample format of the packets. |
| `PacketDuration` | `number` | `0.02` | 0.0025 to 1 | The length of each packet in seconds. |
| `SkipSilence` | `boolean` | `false` | none | When `true`, silent packets are not sent. |

## Signals

### OnIncoming

```luau
bytes.OnIncoming: Signal<AudioPacket>
```

Fires with each new [AudioPacket](audiopacket.md). Packets come in order.

luv only hands out packets while something listens to this signal. With no handler and no waiting coroutine, the packets are thrown away.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local music = Sound:SoundNode("music/theme.ogg")
local bytes = Sound:ToBytes({ SampleRate = 16000, Channels = 1 })
music.Input:Link(bytes.Output)
music.Input:Link(Sound:ToSpeaker())
bytes.OnIncoming:BindHandler("level", function(packet: AudioPacket)
	print(packet.Sequence, packet.Peak)
end)
music:Play()
```

## Packet timing

- Each packet holds `PacketDuration` times `SampleRate` frames, rounded. The default packet holds 960 frames.
- Packets follow each other with no gap and no overlap.
- On average one packet comes every `PacketDuration` seconds. They often come in small groups, because the output device asks for sound in blocks.
- Packets only come while `Enabled` is `true` and at least one node is linked into the `Output`.
- `Sequence` goes up by one for each packet. It starts at 0 for each ToBytes node.
- A silent packet has a `Peak` below 0.0001. When `SkipSilence` skips one, `Sequence` still counts it, so you can see the gap.
- Changing `SampleRate` drops the samples that were not sent yet.
- Packets keep coming when there is no output device. luv keeps time with its own clock then.

## Config

The config table of [Sound:ToBytes](sound-api.md#tobytes). Its type is `ToBytesConfig`. Every field is optional.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Name` | `string` | `"ToBytes"` | The name of the node. |
| `Enabled` | `boolean` | `true` | Whether it makes packets. |
| `SampleRate` | `number` | `48000` | The sample rate of the packets in Hz. |
| `Channels` | `number` | `2` | The number of channels in the packets. |
| `Format` | [AudioFormat](enums.md#audioformat) | `enum.AudioFormat.Int16` | The sample format of the packets. |
| `PacketDuration` | `number` | `0.02` | The length of each packet in seconds. |
| `SkipSilence` | `boolean` | `false` | Whether silent packets are skipped. |
