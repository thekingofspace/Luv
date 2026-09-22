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
