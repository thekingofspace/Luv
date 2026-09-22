# SoundModifier

Inherits: [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Inherited by: [Gain](modifiers.md#gain), [Pan](modifiers.md#pan), [LowPass](modifiers.md#lowpass), [HighPass](modifiers.md#highpass), [BandPass](modifiers.md#bandpass), [Notch](modifiers.md#notch), [Peak](modifiers.md#peak), [LowShelf](modifiers.md#lowshelf), [HighShelf](modifiers.md#highshelf), [Equalizer](modifiers.md#equalizer), [Echo](modifiers.md#echo), [Reverb](modifiers.md#reverb), [Chorus](modifiers.md#chorus), [Flanger](modifiers.md#flanger), [Phaser](modifiers.md#phaser), [Tremolo](modifiers.md#tremolo), [Vibrato](modifiers.md#vibrato), [Distortion](modifiers.md#distortion), [BitCrusher](modifiers.md#bitcrusher), [Compressor](modifiers.md#compressor), [Limiter](modifiers.md#limiter), [NoiseGate](modifiers.md#noisegate), [PitchShift](modifiers.md#pitchshift), [RingModulator](modifiers.md#ringmodulator), [StereoWidth](modifiers.md#stereowidth), [Meter](modifiers.md#meter)

A node that changes sound on its way through.

## Description

You make a modifier with [Sound:Modifier](sound-api.md#modifier). Pass the kind as a string and an optional config table. The [Modifier list](modifiers.md) has every kind and its values.

A modifier takes sound in through its `Output` and sends the changed sound on through its `Input`. Put it between a sound and a speaker:

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local music = Sound:SoundNode("music/theme.ogg")
local echo = Sound:Modifier("Echo", { Delay = 0.25, Mix = 0.3 })
local speaker = Sound:ToSpeaker()
music.Input:Link(echo.Output)
echo.Input:Link(speaker.Output)
music:Play()
```

`ClassName` is the kind, like `"Echo"`. `Name` starts out the same. The Luau type is the kind with `Modifier` after it, like `EchoModifier`. Modifiers have no signals.

It also has every member of [NodeObject](nodeobject.md).

## Properties

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `Input` | [NodeInput](nodeports.md) | none | Sends the changed sound on. Read only. |
| `Output` | [NodeOutput](nodeports.md) | none | Takes sound in. Read only. |
| `Enabled` | `boolean` | `true` | Turns the modifier on or off. See [Enabled](#enabled). |

Each kind adds its own values. See the [Modifier list](modifiers.md).

## Enabled

- Turning a modifier on or off blends over 10 ms, so it does not click.
- While it is off, the sound passes through unchanged. The modifier does no work.
- When you turn it on after it was fully off, it starts fresh. Echo and reverb tails from before are gone. The values of a [Meter](modifiers.md#meter) start at 0 again.

## Mix

Many modifiers have a `Mix` value from 0 to 1. It sets how much you hear of the sound that came in and of the changed sound.

| Mix | Sound that came in | Changed sound |
| --- | --- | --- |
| `0` | full | none |
| `0.25` | full | half |
| `0.5` | full | full |
| `0.75` | half | full |
| `1` | none | full |

## Frequencies

Frequency values are in Hz. The filter kinds are LowPass, HighPass, BandPass, Notch, Peak, LowShelf, HighShelf and Equalizer.

- The frequencies of the filter kinds are clamped to 10 to 24000.
- luv also keeps those frequencies and the `Tone` of [Distortion](modifiers.md#distortion) below 0.49 times [Sound.SampleRate](sound-api.md#properties). At a sample rate of 48000, that is 23520.
- The `Frequency` of [RingModulator](modifiers.md#ringmodulator) and every `Rate` have their own ranges. The sample rate does not limit them.

## Smooth changes

- `Volume` of Gain, `Pan` of Pan and `Width` of StereoWidth glide over 20 ms.
- The frequencies, `Q`, `Resonance` and gains of the filter kinds glide to the new value.
- `Delay` of Echo glides over 50 ms.
- Other values change at once.
- Values from the config table apply at once.
