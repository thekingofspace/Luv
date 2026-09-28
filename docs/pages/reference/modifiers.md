# Modifier list

Every kind you can pass to [Sound:Modifier](sound-api.md#modifier).

Each kind has every member of [SoundModifier](soundmodifier.md), like `Enabled`, `Input` and `Output`. Its Luau type is the kind with `Modifier` after it. `Sound:Modifier("Gain")` returns a `GainModifier`.

Values outside a range are clamped. `Mix` works the same way in every kind. See [Mix](soundmodifier.md#mix). Some values glide when they change. See [Smooth changes](soundmodifier.md#smooth-changes).

## Gain

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Changes the volume.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Volume` | `1` | 0 to 10 | none | `1` keeps the volume as it is. Changes glide over 20 ms. |

### Fade

```luau
gain:Fade(volume: number, duration: number)
```

Moves `Volume` in a straight line to `volume` over `duration` seconds. `volume` is clamped to 0 to 10. A `duration` of `0` sets it at once. Reading `Volume` during a fade gives the value at that moment. Setting `Volume` ends the fade.

It errors with `Fade needs a finite volume and a duration of 0 seconds or more`.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local music = Sound:SoundNode("music/theme.ogg", { Looping = true })
local fader = Sound:Modifier("Gain", { Volume = 0 })
music.Input:Link(fader.Output)
fader.Input:Link(Sound:ToSpeaker())
music:Play()
fader:Fade(1, 2)
```

## Pan

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Pans the sound.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Pan` | `0` | -1 to 1 | none | `-1` is hard left and `1` is hard right. `0` keeps the sound as it is. Changes glide over 20 ms. |

The other channel moves into the side you pan to. At `-1` the left side plays both channels and the right side is silent. So a mono sound gets up to twice as loud on that side.

## LowPass

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A low pass filter.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Cutoff` | `1000` | 10 to 24000 | Hz | Sound above this is cut. |
| `Resonance` | `0.707` | 0.1 to 20 | none | Higher values boost the sound near `Cutoff`. |

## HighPass

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A high pass filter.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Cutoff` | `1000` | 10 to 24000 | Hz | Sound below this is cut. |
| `Resonance` | `0.707` | 0.1 to 20 | none | Higher values boost the sound near `Cutoff`. |

## BandPass

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A band pass filter.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Frequency` | `1000` | 10 to 24000 | Hz | The middle of the band that stays. |
| `Q` | `1` | 0.1 to 40 | none | Higher values keep a narrower band. |

## Notch

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A notch filter.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Frequency` | `1000` | 10 to 24000 | Hz | The middle of the band that is cut. |
| `Q` | `1` | 0.1 to 40 | none | Higher values cut a narrower band. |

## Peak

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A peak filter.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Frequency` | `1000` | 10 to 24000 | Hz | The middle of the band that changes. |
| `Q` | `1` | 0.1 to 40 | none | Higher values change a narrower band. |
| `Gain` | `0` | -48 to 48 | dB | Above 0 boosts. Below 0 cuts. |

## LowShelf

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A low shelf filter.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Frequency` | `200` | 10 to 24000 | Hz | Where the change starts. |
| `Gain` | `0` | -48 to 48 | dB | Above 0 boosts. Below 0 cuts. |

## HighShelf

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A high shelf filter.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Frequency` | `4000` | 10 to 24000 | Hz | Where the change starts. |
| `Gain` | `0` | -48 to 48 | dB | Above 0 boosts. Below 0 cuts. |

## Equalizer

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A three band equalizer.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `LowGain` | `0` | -48 to 48 | dB | The change below `LowFrequency`. |
| `MidGain` | `0` | -48 to 48 | dB | The change between `LowFrequency` and `HighFrequency`. |
| `HighGain` | `0` | -48 to 48 | dB | The change above `HighFrequency`. |
| `LowFrequency` | `400` | 10 to 24000 | Hz | Where the low band ends. |
| `HighFrequency` | `4000` | 10 to 24000 | Hz | Where the high band starts. luv keeps it a little above `LowFrequency`. |

## Echo

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Adds an echo.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Delay` | `0.3` | 0.001 to 5 | seconds | The time between repeats. Changes glide over 50 ms. |
| `Feedback` | `0.4` | 0 to 0.95 | none | How much of each repeat comes back again. |
| `Mix` | `0.5` | 0 to 1 | none | See [Mix](soundmodifier.md#mix). |
| `PingPong` | `false` | none | none | When `true`, the repeats go back and forth between left and right. The sound going into the echo is mixed to mono. |

## Reverb

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Adds reverb.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `RoomSize` | `0.6` | 0 to 1 | none | Higher values ring longer. |
| `Damping` | `0.5` | 0 to 1 | none | Higher values make the tail duller. |
| `Width` | `1` | 0 to 1 | none | `0` makes the tail mono. `1` makes it fully stereo. |
| `Mix` | `0.35` | 0 to 1 | none | See [Mix](soundmodifier.md#mix). |
| `PreDelay` | `0.02` | 0 to 0.5 | seconds | The time before the reverb starts. |

## Chorus

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Adds a chorus.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Rate` | `0.8` | 0 to 20 | Hz | How fast the effect moves. |
| `Depth` | `0.5` | 0 to 1 | none | How far the effect moves. |
| `Mix` | `0.5` | 0 to 1 | none | See [Mix](soundmodifier.md#mix). |

## Flanger

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Adds a flanger.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Rate` | `0.25` | 0 to 20 | Hz | How fast the effect moves. |
| `Depth` | `0.7` | 0 to 1 | none | How far the effect moves. |
| `Feedback` | `0.5` | -0.95 to 0.95 | none | How much of the result goes back in. Negative values flip it. |
| `Mix` | `0.5` | 0 to 1 | none | See [Mix](soundmodifier.md#mix). |

## Phaser

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Adds a phaser.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Rate` | `0.5` | 0 to 20 | Hz | How fast the effect moves. |
| `Depth` | `0.7` | 0 to 1 | none | How far the effect moves. |
| `Feedback` | `0.5` | 0 to 0.95 | none | How much of the result goes back in. |
| `Mix` | `0.5` | 0 to 1 | none | See [Mix](soundmodifier.md#mix). |

## Tremolo

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Adds tremolo.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Rate` | `5` | 0 to 40 | Hz | How many times per second. |
| `Depth` | `0.5` | 0 to 1 | none | How far the volume dips. At `1` it dips to silence. |

## Vibrato

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Adds vibrato.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Rate` | `5` | 0 to 40 | Hz | How many times per second. |
| `Depth` | `0.3` | 0 to 1 | none | How far the pitch moves. |

Vibrato has no `Mix`. You only hear the changed sound.

## Distortion

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Distorts the sound.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Drive` | `0.5` | 0 to 1 | none | How hard the sound is pushed. |
| `Tone` | `8000` | 200 to 20000 | Hz | Sound above this is cut after the distortion. |
| `Mix` | `1` | 0 to 1 | none | See [Mix](soundmodifier.md#mix). |

## BitCrusher

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A bit crusher.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Bits` | `8` | 1 to 24 | bits | Fewer bits sound rougher. Fractions work too. |
| `Downsample` | `1` | 1 to 64 | frames | Holds each sample for this many frames. Whole numbers only. |
| `Mix` | `1` | 0 to 1 | none | See [Mix](soundmodifier.md#mix). |

## Compressor

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A compressor.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Threshold` | `-20` | -80 to 0 | dB | The level where it starts to turn the sound down. |
| `Ratio` | `4` | 1 to 50 | none | How much it turns down. At `4`, a level 4 dB over `Threshold` comes out 1 dB over. |
| `Attack` | `0.01` | 0 to 1 | seconds | How fast it turns down. |
| `Release` | `0.1` | 0 to 5 | seconds | How fast it lets go. |
| `MakeupGain` | `0` | -24 to 48 | dB | Gain added at the end. |

## Limiter

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A limiter.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Threshold` | `-1` | -40 to 0 | dB | The highest level that passes. |
| `Release` | `0.05` | 0 to 5 | seconds | How fast the level comes back after a loud part. |

It turns loud parts down right away.

## NoiseGate

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A noise gate.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Threshold` | `-50` | -100 to 0 | dB | The level that opens the gate. |
| `Attack` | `0.005` | 0 to 1 | seconds | How fast it opens. |
| `Release` | `0.1` | 0 to 5 | seconds | How fast it closes. |
| `Hold` | `0.05` | 0 to 5 | seconds | How long it stays open after the sound drops below `Threshold`. |

The gate starts closed.

## PitchShift

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Changes the pitch but not the speed.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Pitch` | `1` | 0.25 to 4 | none | `2` is one octave up. `0.5` is one octave down. |

## RingModulator

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A ring modulator.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Frequency` | `440` | 0 to 20000 | Hz | The frequency of the sine wave. |
| `Mix` | `1` | 0 to 1 | none | See [Mix](soundmodifier.md#mix). |

## StereoWidth

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Makes the stereo sound wider or narrower.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Width` | `1` | 0 to 3 | none | `0` is mono. `1` keeps the sound as it is. Higher values are wider. Changes glide over 20 ms. |

## Meter

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Measures the sound and passes it through unchanged.

Meter has no values to set besides `Enabled`. It has two values that you read:

| Name | Type | Description |
| --- | --- | --- |
| `Peak` | `number` | The loudest recent sample. It falls by 20 dB each second. `1` is full volume. Read only. |
| `Loudness` | `number` | The average level (RMS) over about the last 0.3 seconds. Read only. |

Both stop changing while `Enabled` is `false`.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local music = Sound:SoundNode("music/theme.ogg")
local meter = Sound:Modifier("Meter")
music.Input:Link(meter.Output)
meter.Input:Link(Sound:ToSpeaker())
music:Play()
window.PreFrame:BindHandler("clip", function()
	if meter.Peak > 0.9 then
		print("Too loud")
	end
end)
```

## AllPass

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Changes the timing of each frequency and leaves its volume alone. On its own it sounds the same. Mixed with the original, or chained a few times, it gives phase effects.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Frequency` | `1000` | 10 to 24000 | Hz | The frequency it shifts the most. |
| `Q` | `0.707` | 0.1 to 40 | none | How narrow the shift is. |

## DcBlock

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Removes a steady offset, the part of a sound that sits above or below zero and never moves. You cannot hear it, but it uses up room before full volume, so the next loud part clips sooner and crackles.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Frequency` | `20` | 1 to 200 | Hz | Everything below this is removed. |

Put it first in a chain fed by sound you did not make, like a microphone or a [FromBytes](frombytes.md) stream.

## SoftClip

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Rounds off loud peaks so they never pass the ceiling. A hard cut at full volume is what makes a loud mix crackle. SoftClip bends the peak down smoothly instead.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Ceiling` | `-1` | -24 to 0 | dB | The highest level that comes out. |
| `Knee` | `0.5` | 0 to 1 | none | How far below the ceiling the rounding starts. `0` cuts hard at the ceiling. `1` starts rounding from silence. |

It has no memory and no delay, so it costs almost nothing. A [Limiter](#limiter) turns the whole sound down for a moment instead, which stays cleaner through long loud parts. Use SoftClip for short peaks and a Limiter for loud stretches.

## AutoGain

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Turns the sound up or down by itself so it stays near one level. Good for voice chat, and for any sound that arrives at a volume you do not control.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Target` | `-18` | -60 to 0 | dB | The average level it aims for. |
| `Speed` | `1` | 0.01 to 10 | seconds | How long it takes to catch up. Short reacts quickly and can pump. Long is smooth. |
| `MaxGain` | `18` | 0 to 48 | dB | The most it turns the sound up or down. |

It has one value that you read:

| Name | Type | Description |
| --- | --- | --- |
| `CurrentGain` | `number` | The gain it uses right now, in dB. Read only. |

Below about -70 dB it holds its gain instead of lifting silence into hiss.

## Expander

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

The opposite of a [Compressor](#compressor). It makes quiet parts quieter, so hum and room noise drop away while the sound itself stays.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Threshold` | `-40` | -100 to 0 | dB | Sound below this level is turned down. |
| `Ratio` | `2` | 1 to 20 | none | How much. At `2`, a level 1 dB under `Threshold` comes out 2 dB under. |
| `Attack` | `0.005` | 0 to 1 | seconds | How fast it opens when the sound comes back. |
| `Release` | `0.1` | 0 to 5 | seconds | How fast it turns down when the sound drops. |

A [NoiseGate](#noisegate) shuts all the way. An Expander only leans, so it sounds more natural.

## Exciter

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Adds brightness by making new high notes out of the ones already there. It lifts a sound that seems dull or muffled without raising the hiss the way turning up the treble does.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Frequency` | `3000` | 500 to 16000 | Hz | Only sound above this is excited. |
| `Amount` | `0.3` | 0 to 1 | none | How much is added. |

## AutoWah

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

A filter that opens as the sound gets louder and closes as it gets quieter. Put a guitar or a voice through it and it says "wah".

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Sensitivity` | `0.5` | 0 to 1 | none | How easily loud sound opens the filter. |
| `Frequency` | `250` | 20 to 5000 | Hz | Where the filter sits while it is closed. |
| `Range` | `3` | 0 to 8 | octaves | How far up it opens. |
| `Resonance` | `4` | 0.1 to 20 | none | How sharp the peak of the filter is. |
| `Mix` | `1` | 0 to 1 | none | See [Mix](soundmodifier.md#mix). |

## Haas

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Makes a sound wider by playing one side a few milliseconds late. You still hear the sound from the early side, but the gap between them sounds wide.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Delay` | `0.012` | -0.05 to 0.05 | seconds | How late one side is. Above 0 the right side is late. Below 0 the left side is late. Changes glide over 20 ms. |

Keep it between about 0.005 and 0.03. Longer than that and it becomes an echo. A Haas sound can thin out when both sides are added together, so check it with `Channels = 1` on a [ToSpeaker](tospeaker.md).

## AutoPan

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Moves the sound from side to side by itself.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Rate` | `0.5` | 0 to 20 | Hz | How many times a second it goes over and back. |
| `Depth` | `1` | 0 to 1 | none | How far it moves. `1` goes all the way to each side. |

## Transient

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Changes the start of each hit and the ring after it, each on its own. It makes drums punchier or softens a harsh click, with no compressor.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Attack` | `0` | -1 to 1 | none | Above 0 the start of each hit gets louder. Below 0 it gets softer. |
| `Sustain` | `0` | -1 to 1 | none | Above 0 the ring after each hit gets louder. Below 0 it dies away sooner. |

With both at `0` it does no work at all. It follows the shape of the sound, not its volume, so loud and quiet hits change the same way.

## Spectrum

Inherits: [SoundModifier](soundmodifier.md) < [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Measures how loud each part of the range is and passes the sound through unchanged. Use it to drive a visualizer, or a light that pulses with the music.

| Name | Default | Range | Unit | Description |
| --- | --- | --- | --- | --- |
| `Bands` | `8` | 1 to 32 | none | How many parts to split the range into. |
| `Smoothing` | `0.6` | 0 to 0.99 | none | How slowly the levels move. `0` jumps with every block. Higher is calmer. |

The bands run from 40 Hz to 16 kHz, spaced the way the ear hears pitch, so each band covers the same number of notes.

### GetLevels

```luau
spectrum:GetLevels(): { number }
```

The level of each band, from low to high. `1` is about full volume.

### GetFrequencies

```luau
spectrum:GetFrequencies(): { number }
```

The middle of each band in Hz, from low to high, in the same order as GetLevels.

```luau
local Process = import("Process")

local spectrum = Sound:Modifier("Spectrum", { Bands = 16 })
music.Input:Link(spectrum.Output)
spectrum.Input:Link(speaker.Output)

Process.Heartbeat:BindHandler("bars", function()
	for index, level in spectrum:GetLevels() do
		bars[index].Size = udim.new(12, level * 300)
	end
end)
```
