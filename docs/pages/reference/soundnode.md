# SoundNode

Inherits: [NodeObject](nodeobject.md) < [BaseGameObject](basegameobject.md)

Plays a sound file.

## Description

You make a SoundNode with [Sound:SoundNode](sound-api.md#soundnode). It loads a file from the `assets` folder. A node made with [Sound:FromString](sound-api.md#fromstring) works the same way. See [FromString](#fromstring).

A new node is stopped. Link its `Input` to a [ToSpeaker](tospeaker.md) and call [Play](#play). A node that is not linked still plays. You just do not hear it.

Each node plays one main sound. [PlayOneShot](#playoneshot) plays extra copies on top of it.

It also has every member of [NodeObject](nodeobject.md).

## Properties

| Name | Type | Default | Range | Description |
| --- | --- | --- | --- | --- |
| `Input` | [NodeInput](nodeports.md) | none | none | Sends the sound on. Read only. |
| `Volume` | `number` | `1` | 0 to 10 | The volume. `1` plays the file as it is. Changes glide over 20 ms while the sound plays. |
| `PlaybackSpeed` | `number` | `1` | 0.01 to 32 | How fast the file plays. It also changes the pitch. |
| `Looping` | `boolean` | `false` | none | Plays the loop part again and again. See [Looping](#looping). |
| `LoopStart` | `number` | `0` | 0 or more | Where the loop part starts, in seconds. |
| `LoopEnd` | `number` | `0` | 0 or more | Where the loop part ends, in seconds. `0` means the end of the file. |
| `PlayPosition` | `number` | `0` | 0 to `Length` | Where the main sound is, in seconds. See [PlayPosition](#playposition). |
| `Length` | `number` | none | none | The length of the file in seconds. Read only. |
| `IsPlaying` | `boolean` | `false` | none | `true` while the main sound plays. Read only. |
| `IsPaused` | `boolean` | `false` | none | `true` while the main sound is paused. Read only. |
| `SampleRate` | `number` | none | none | The sample rate of the file in Hz. Read only. |
| `Channels` | `number` | none | none | `1` or `2`. A file with more channels keeps the first two. Read only. |
| `Asset` | `string?` | none | none | The path of the file inside `assets`, with its extension. `nil` for FromString. Read only. |

## Looping

When `Looping` is `true`, the sound starts at `PlayPosition` as usual. When it reaches `LoopEnd`, it jumps back to `LoopStart`. So the part before `LoopStart` plays once, like an intro.

- A `LoopEnd` of `0` means the end of the file.
- Both points are clamped to `Length`.
- If `LoopEnd` is not after `LoopStart`, the whole file loops.
- The points are times in the file. `PlaybackSpeed` does not change them.
- You can change `Looping` while the sound plays. When you turn it off, the sound plays on to the end of the file.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local theme = Sound:SoundNode("music/theme.ogg", { Looping = true, LoopStart = 8.25 })
theme.Input:Link(Sound:ToSpeaker())
theme:Play()
```

## PlayPosition

Reading `PlayPosition` gives the position of the main sound in seconds. Setting it does this:

| State | What happens |
| --- | --- |
| Stopped | The next [Play](#play) starts there. |
| Playing | The sound jumps there. luv crossfades over 10 ms, so it does not click. |
| Paused | The paused position moves there. [Resume](#resume) goes on from it. |

The value is clamped to 0 to `Length`. A value that is not a finite number errors with `PlayPosition must be a finite number of seconds`.

## Methods

### Play

```luau
node:Play()
```

Starts the main sound and fires [Started](#started).

- When the node is stopped, it starts at `PlayPosition`. From `0` it starts at full volume right away. From a later point it fades in over 10 ms.
- When the node is playing or paused, it starts again from `0`. The old sound fades out over 10 ms.

Play also resets the count that [Looped](#looped) passes.

### Stop

```luau
node:Stop()
```

Stops the main sound and every one shot. They fade out over 10 ms. `PlayPosition` goes back to `0`. Fires [Stopped](#stopped) if the node was playing or paused.

### Pause

```luau
node:Pause()
```

Pauses the main sound and keeps its position. It fades out over 10 ms. One shots fade out too, and they do not come back when you resume. Fires [Paused](#paused). Does nothing when the node is not playing.

### Resume

```luau
node:Resume()
```

| State | What happens |
| --- | --- |
| Paused | The sound fades in over 10 ms from where it paused. Fires [Resumed](#resumed). |
| Stopped | The same as [Play](#play). Fires [Started](#started). |
| Playing | Nothing. |

### PlayOneShot

```luau
node:PlayOneShot(volume: number?)
```

Plays the whole file once more, on top of anything that already plays. Use it for sounds that can overlap, like hits and steps.

- It always starts at the beginning of the file. It never loops.
- Its volume is `volume` times the `Volume` of the node. `volume` defaults to `1` and is clamped to 0 to 10.
- It uses the `PlaybackSpeed` of the node.
- It works while the node is stopped, playing or paused.
- It does not change `IsPlaying` or `PlayPosition`. It fires no signals.
- A node has room for 8 one shots at once. Sounds that are fading out use this room too. When all 8 are busy, the quietest one is cut.
- [Stop](#stop) and [Pause](#pause) fade one shots out. [Play](#play) and setting `PlayPosition` do not.

A volume that is not a finite number errors with `PlayOneShot needs a finite volume`.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local step = Sound:SoundNode("sfx/step.wav")
step.Input:Link(Sound:ToSpeaker())
step:PlayOneShot()
step:PlayOneShot(0.5)
```

## Signals

`Started`, `Stopped`, `Paused` and `Resumed` fire inside the method call. Their handlers start before the call returns. `Ended` and `Looped` come from the audio engine. They fire a few milliseconds after the moment in the sound.

### Started

```luau
node.Started: Signal<()>
```

Fires inside [Play](#play). It also fires when [Resume](#resume) starts a stopped node.

### Stopped

```luau
node.Stopped: Signal<()>
```

Fires inside [Stop](#stop) and [Sound:StopAll](sound-api.md#stopall), if the node was playing or paused. It does not fire when the sound reaches its end. [Ended](#ended) fires then.

### Paused

```luau
node.Paused: Signal<()>
```

Fires inside [Pause](#pause) and [Sound:PauseAll](sound-api.md#pauseall), when the node was playing.

### Resumed

```luau
node.Resumed: Signal<()>
```

Fires inside [Resume](#resume) and [Sound:ResumeAll](sound-api.md#resumeall), when the node was paused.

### Ended

```luau
node.Ended: Signal<()>
```

Fires when the main sound reaches the end of the file. By then `IsPlaying` is `false` and `PlayPosition` is `0`. It does not fire for a looping sound, for [Stop](#stop) or for one shots.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local fanfare = Sound:SoundNode("sfx/fanfare.wav")
fanfare.Input:Link(Sound:ToSpeaker())
fanfare:Play()
fanfare.Ended:Wait()
fanfare:Destroy()
```

### Looped

```luau
node.Looped: Signal<number>
```

Fires when the sound jumps from `LoopEnd` back to `LoopStart`. The argument counts the loops since the last [Play](#play). Pausing, resuming and setting `PlayPosition` do not reset it. The count can go up by more than 1 at once when the loop part is very short.

## Config

The config table of [Sound:SoundNode](sound-api.md#soundnode) and [Sound:FromString](sound-api.md#fromstring). Its type is `SoundNodeConfig`. Every field is optional.

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Name` | `string` | `"SoundNode"` | The name of the node. It is `"FromString"` for a FromString node. |
| `Volume` | `number` | `1` | The volume. |
| `PlaybackSpeed` | `number` | `1` | How fast the file plays. |
| `Looping` | `boolean` | `false` | Whether the loop part repeats. |
| `LoopStart` | `number` | `0` | Where the loop part starts, in seconds. |
| `LoopEnd` | `number` | `0` | Where the loop part ends, in seconds. |
| `PlayPosition` | `number` | `0` | Where the first [Play](#play) starts, in seconds. |

## FromString

[Sound:FromString](sound-api.md#fromstring) makes a node from a sound file that is already in memory, as a string or a buffer. The node has every member of a SoundNode. These things are different:

- `ClassName` and the default `Name` are `"FromString"`.
- `Asset` is always `nil`.
- Each FromString node decodes its own copy. Nodes made from the same data do not share it.
- luv finds the format from the data. It has no file name to go by.

In Luau types, `FromString` is the same type as `SoundNode`.

```luau
local Asset = import("Asset")
local Window = import("Window")

local window = Window.new({ Title = "Space Miner" })
local Sound = window:GetAPI("Sound")

local hit = Sound:FromString(Asset.LoadString("sfx/hit.wav"), { Volume = 0.6 })
hit.Input:Link(Sound:ToSpeaker())
hit:PlayOneShot()
```
