# Random

A random number generator with a seed.

```luau
local Random = import("Random")
```

## Description

Each generator has its own seed and state. The same seed always gives the same numbers in the same order. Use it for levels, worlds and replays that must come out the same each time.

Random is not safe for secrets like keys and tokens. Use [Crypto.RandomBytes](crypto.md#randombytes) and [Crypto.RandomUUID](crypto.md#randomuuid) for those.

A generator is not a game object. It has no `ClassName` or `Destroy`. `typeof` gives `"Random"`, and `tostring` gives text like `"Random(7)"`. Every method returns right away. None of them yield.

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [new](#new)(seed) | `Random` | no |

## Function descriptions

### new

```luau
Random.new(seed: (number | string)?): Random
```

Makes a new generator.

| seed | What happens |
| --- | --- |
| `nil` | luv picks a random seed. Read it from `Seed` if you want to use it again. |
| a number | The number is the seed. Negative numbers and fractions work. |
| a string | The string is turned into a whole number, and that number is the seed. The same string always gives the same number. |

`Random.new(rng.Seed)` makes a generator that gives the same numbers as `rng` did from the start. This also works for string seeds and random seeds. The string `"42"` and the number `42` are different seeds.

Other types are an error, like `a Random seed must be a number or a string, got boolean`. `0/0` and `math.huge` are errors too.

```luau
local Random = import("Random")

local world = Random.new("world-1")
local copy = Random.new(world.Seed)
print(world:NextInt(1, 100) == copy:NextInt(1, 100))
```

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `Seed` | `number` | The seed of this generator. For a string seed, it is the number made from the string. Read only. |

## Methods

### NextInt

```luau
rng:NextInt(min: number, max: number): number
```

Returns a whole number from `min` to `max`. Both `min` and `max` can come up.

`min` and `max` must be whole numbers from `-9007199254740992` to `9007199254740992`. Other values are an error, like `min must be a whole number, got 1.5`. A `min` above `max` is an error too: `NextInt's min (5) must not be greater than its max (1)`.

```luau
local Random = import("Random")

local rng = Random.new()
print(rng:NextInt(1, 6))
```

### NextNumber

```luau
rng:NextNumber(min: number?, max: number?): number
```

Returns a number from `min` up to, but not including, `max`.

| Call | Range |
| --- | --- |
| `rng:NextNumber()` | From 0 up to 1. |
| `rng:NextNumber(max)` | From 0 up to `max`. |
| `rng:NextNumber(min, max)` | From `min` up to `max`. |

`0/0` and `math.huge` are errors: `NextNumber needs finite numbers`.

### NextBool

```luau
rng:NextBool(chance: number?): boolean
```

Returns `true` with the given chance. `chance` is from 0 to 1 and defaults to `0.5`. A chance of `0` never gives `true`, and `1` always does. Other values are an error, like `NextBool's chance must be between 0 and 1, got 2`.

### NextSign

```luau
rng:NextSign(): number
```

Returns `1` or `-1`.

### NextGaussian

```luau
rng:NextGaussian(mean: number?, deviation: number?): number
```

Returns a number from a normal distribution. `mean` defaults to `0` and `deviation` defaults to `1`.

### NextExponential

```luau
rng:NextExponential(rate: number?): number
```

Returns a number from an exponential distribution. The result is `0` or more, and the average is `1 / rate`. `rate` defaults to `1` and must be above `0`. Other values are an error, like `NextExponential's rate must be greater than 0, got 0`.

### NextAngle

```luau
rng:NextAngle(): number
```

Returns an angle in degrees, from 0 up to 360.

### NextDirection

```luau
rng:NextDirection(): UDim
```

Returns a [UDim](udim.md) with a length of 1 that points in a random direction on the X and Y axes. `Z` is `0`.

### NextUnitVector

```luau
rng:NextUnitVector(): UDim
```

Returns a [UDim](udim.md) with a length of 1 that points in a random direction in 3D.

### NextUDim

```luau
rng:NextUDim(min: UDim, max: UDim): UDim
```

Returns a [UDim](udim.md). Its `X` is picked from `min.X` up to `max.X`. `Y` and `Z` work the same way.

### NextPointInCircle

```luau
rng:NextPointInCircle(center: UDim, radius: number): UDim
```

Returns a random point inside a circle on the X and Y axes. The points are spread evenly over the circle. `Z` is the `Z` of `center`.

```luau
local Random = import("Random")

local rng = Random.new()
local spawn = rng:NextPointInCircle(udim.new(400, 300), 50)
print(spawn.X, spawn.Y)
```

### NextPointOnCircle

```luau
rng:NextPointOnCircle(center: UDim, radius: number): UDim
```

Returns a random point on the edge of a circle on the X and Y axes. `Z` is the `Z` of `center`.

### NextColor

```luau
rng:NextColor(alpha: number?): Color
```

Returns a [Color](color.md) with a random `R`, `G` and `B`, each from 0 up to 1. `A` is `alpha`, which defaults to `1`.

### NextHue

```luau
rng:NextHue(saturation: number?, value: number?): Color
```

Returns a [Color](color.md) with a random hue. It works like `color.fromHSV` with a random `h`. `saturation` and `value` default to `1`. `A` is `1`.

### NextBytes

```luau
rng:NextBytes(count: number): string
```

Returns `count` random bytes as a string. `count` is a whole number from 0 to 67108864. Other values are an error, like `the byte count must be between 0 and 67108864, got -1`.

### NextString

```luau
rng:NextString(length: number, characters: string?): string
```

Returns a string of `length` characters picked from `characters`. The default is the letters `A` to `Z` and `a` to `z` and the digits `0` to `9`. `characters` can hold any UTF-8 text. Each character counts once, even when it takes more than one byte.

`length` is a whole number from 0 to 67108864. An empty `characters` string is an error: `NextString needs at least one character to choose from`.

```luau
local Random = import("Random")

local rng = Random.new()
print(rng:NextString(8))
print(rng:NextString(6, "0123456789"))
```

### NextUUID

```luau
rng:NextUUID(): string
```

Returns text in the form of a version 4 UUID, like `"0f8e2a44-91c3-4b7e-a2d5-6c1f9e3b7a10"`. It comes from the generator, so the same seed gives the same UUIDs. Use [Crypto.RandomUUID](crypto.md#randomuuid) for ids that nobody must guess.

### Pick

```luau
rng:Pick(list: { T }): (T?, number?)
```

Returns a random item of `list` and its index. For an empty list it returns `nil` and `nil`.

### WeightedPick

```luau
rng:WeightedPick(list: { T }, weights: { number }): (T, number)
```

Returns a random item of `list` and its index. `weights` has one weight for each item. An item with the weight `2` comes up twice as often as an item with the weight `1`. An item with the weight `0` never comes up.

| Message | Cause |
| --- | --- |
| `WeightedPick needs one weight per value, got 3 values and 2 weights` | The two lists have different lengths. |
| `weight #2 must be a number of at least 0, got -1` | A weight is below `0`, or it is `0/0` or `math.huge`. |
| `WeightedPick needs at least one weight above 0` | Every weight is `0`. |

```luau
local Random = import("Random")

local rng = Random.new()
local loot, index = rng:WeightedPick({ "common", "rare", "epic" }, { 80, 18, 2 })
print(loot, index)
```

### Shuffle

```luau
rng:Shuffle(list: { T }): { T }
```

Shuffles `list` in place and returns the same table.

### Sample

```luau
rng:Sample(list: { T }, count: number): { T }
```

Returns a new list with `count` items of `list` in random order. Each item comes from a different index. `list` does not change. `count` is from 0 to the length of `list`. Other values are an error, like `the sample size must be between 0 and 5, got 6`.

### Noise

```luau
rng:Noise(position: UDim): number
```

Returns smooth noise from -1 to 1 at a position. Close positions give close values. For 2D noise, leave `Z` at `0`.

The noise depends only on the seed. Calling other methods does not change it. At positions made of whole numbers the noise is always `0`, so scale your positions, for example by `0.05`. The pattern repeats every 256 units.

```luau
local Random = import("Random")

local rng = Random.new(5)
for x = 1, 5 do
	print(rng:Noise(udim.new(x * 0.1, 0.5)))
end
```

### FractalNoise

```luau
rng:FractalNoise(position: UDim, octaves: number?, persistence: number?, lacunarity: number?): number
```

Adds layers of [Noise](#noise) and returns a value from -1 to 1. It has more detail than Noise.

| Argument | Default | Description |
| --- | --- | --- |
| `octaves` | `4` | The number of layers. A whole number from 1 to 16. |
| `persistence` | `0.5` | Each layer has this much of the strength of the layer before it. |
| `lacunarity` | `2` | Each layer is this many times finer than the layer before it. |

`octaves` of `0` is an error: `FractalNoise needs at least 1 octave`. More than 16 is an error too, like `the octave count must be between 0 and 16, got 17`.

```luau
local Random = import("Random")

local world = Random.new("world-1")
local height = world:FractalNoise(udim.new(12 * 0.05, 30 * 0.05), 5)
print(height)
```

### Clone

```luau
rng:Clone(): Random
```

Returns a new generator with the same seed and the same state. Both give the same numbers from here on.

### Reset

```luau
rng:Reset()
```

Starts the numbers over from the seed.
