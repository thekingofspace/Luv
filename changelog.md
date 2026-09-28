# 0.1.12

## Sound

- `Sound:Bake` plays a sound through a list of modifiers once, ahead of time, and keeps the result. Playing it costs the same as playing a plain file. 24 sounds through a Reverb, an Echo and a Chorus went from 16.5 percent of the mixer's time to 1.8 percent.
- A `BakedSound` plays through `Sound:SoundNode`, can be baked again, and `GetBytes` hands it back as a WAV file so it can be saved and loaded next time.
- Bakes can add a tail for reverb and echo, normalize to a level, trim, change speed, and keep one channel when both sides match.
- Modifiers with nothing coming in now rest and cost almost nothing. 500 waiting modifiers went from 14 percent of the mixer's time to 1.3 percent. Fades and echo tails still finish.
- `Sound.MaxVoices` caps how many sounds a window plays at once. A new sound stops the oldest one with the lowest `Priority`, and `Play` returns `false` when nothing can make room.
- `Sound:Preload` decodes sounds ahead of time so they start at once, and `Sound:Unload` lets them go.
- `Sound:GetStats` and `Sound:ResetStats`, with `Load`, `BusiestBlock`, `Peak`, `ClippedBlocks`, `Nodes`, `RestingNodes` and the voice counts. `Load`, `Peak`, `ClippedBlocks` and `Voices` are properties of the Sound API too.
- When luv cannot reach the mixer in time it now fades out over about a millisecond instead of cutting to silence.

## Modifiers

- 11 new modifiers: `AllPass`, `DcBlock`, `SoftClip`, `AutoGain`, `Expander`, `Exciter`, `AutoWah`, `Haas`, `AutoPan`, `Transient` and `Spectrum`.
- `SoftClip` rounds off peaks before they crackle, and `DcBlock` removes an offset that eats headroom.
- `AutoGain` holds a sound near one level and reports `CurrentGain`.
- `Spectrum` measures the sound in up to 32 bands, with `GetLevels` and `GetFrequencies` for visualizers.

## Shipping

- The error box of a packed game groups errors that repeat and lists the most common first, with how many times each happened. Before, it listed the first five, so an error that repeated every frame could hide behind earlier ones.

## Coroutines

- A new `task` global with `task.wait`, `task.spawn`, `task.defer` and `task.delay`.
- `task.create` makes a coroutine you can start again and again, and takes a flag to allow only one run at a time.
- A new `promise` global. `promise.new` runs a body with `resolve` and `reject` on a coroutine, and the promise it returns has `AndThen`, `Catch`, `Finally`, `Await`, `AwaitStatus` and `Cancel`.
- `promise.all` and `promise.race` wait on a whole list. `promise.call`, `promise.resolve`, `promise.reject`, `promise.delay` and `promise.is` are there too.
- A new `switch` global. `switch.new` builds a table of names to functions that is looked up in Rust, and each call runs its case on a new coroutine.
- luv reuses the coroutines it makes for engine calls, so a switch, a promise step and a signal invoke all cost less than they did.
