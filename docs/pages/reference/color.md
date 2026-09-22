# Color

A color with red, green, blue and alpha. Every color in luv is a Color.

```luau
local orange = color.fromRGB(255, 128, 0)
```

## Description

A Color holds four numbers named `R`, `G`, `B` and `A`. They normally go from 0 to 1. For `A`, 1 is fully opaque and 0 is fully transparent.

luv does not clamp the numbers. Math can push them below 0 or above 1. [ToHex](#tohex) and [ToRGB](#torgb) clamp when they convert.

A Color never changes. Its fields are read only, and setting one raises an error. Math on a Color gives you a new Color.

`==` compares the numbers. Two colors with the same numbers are still different keys in a table.

You can send a Color with [Messenger](messenger.md) and use it inside a parallel block. The other thread gets a copy. [Serde](serde.md) encodes a Color as a table with `R`, `G`, `B` and `A` keys.

`typeof` returns `"Color"`.

## Constructors

### new

```luau
color.new(r: number?, g: number?, b: number?, a: number?): Color
```

Makes a color from numbers between 0 and 1. `r`, `g` and `b` default to 0. `a` defaults to 1. So `color.new()` is opaque black.

### fromRGB

```luau
color.fromRGB(r: number, g: number, b: number, a: number?): Color
```

Makes a color from numbers between 0 and 255. `a` defaults to 255. luv divides each number by 255.

### fromHex

```luau
color.fromHex(hex: string): Color
```

Makes a color from hex text. The `#` in front is optional. Spaces around the text are ignored. Letters can be upper or lower case.

| Format | Example | Alpha |
| --- | --- | --- |
| `#RGB` | `"#f80"` | Opaque |
| `#RGBA` | `"#f808"` | From the text |
| `#RRGGBB` | `"#ff8800"` | Opaque |
| `#RRGGBBAA` | `"#ff880080"` | From the text |

In the short formats each digit counts twice, so `"#f80"` is the same as `"#ff8800"`.

Any other text raises an error like `'#12' is not a hex color, expected #RGB, #RGBA, #RRGGBB or #RRGGBBAA`.

### fromHSV

```luau
color.fromHSV(h: number, s: number, v: number, a: number?): Color
```

Makes a color from hue, saturation and value, each between 0 and 1. The hue wraps around, so 1 is the same as 0 and 1.25 is the same as 0.25. `a` defaults to 1.

```luau
local red = color.new(1, 0, 0)
local orange = color.fromRGB(255, 128, 0)
local sky = color.fromHex("#87ceeb")
local green = color.fromHSV(1 / 3, 1, 1)
print(red:ToHex(), orange:ToHex(), sky:ToHex(), green:ToHex())
```

This prints:

```text
#ff0000	#ff8000	#87ceeb	#00ff00
```

## Constants

| Name | Value |
| --- | --- |
| `color.white` | `Color(1, 1, 1, 1)` |
| `color.black` | `Color(0, 0, 0, 1)` |
| `color.transparent` | `Color(0, 0, 0, 0)` |

## Properties

| Name | Type | Description |
| --- | --- | --- |
| `R` | `number` | Red. Read only. |
| `G` | `number` | Green. Read only. |
| `B` | `number` | Blue. Read only. |
| `A` | `number` | Alpha. Read only. |

## Methods

### Lerp

```luau
value:Lerp(goal: Color, alpha: number): Color
```

Returns a color between this one and `goal`. It blends all four numbers, alpha included. An `alpha` of 0 gives this color and 1 gives `goal`. `alpha` is not clamped.

### ToHex

```luau
value:ToHex(includeAlpha: boolean?): string
```

Returns the color as lowercase `#rrggbb` text. Pass `true` to get `#rrggbbaa`. Each number is clamped to 0 to 1 first, then turned into a whole number from 0 to 255.

### ToRGB

```luau
value:ToRGB(): (number, number, number, number)
```

Returns `r`, `g`, `b` and `a` as whole numbers from 0 to 255. Each number is clamped and rounded.

### ToHSV

```luau
value:ToHSV(): (number, number, number)
```

Returns the hue, saturation and value. It does not return alpha. Grays have a hue of 0.

```luau
local orange = color.fromRGB(255, 128, 0)
print(orange:ToHex(), orange:ToHex(true))
print(orange:ToRGB())
print(color.fromRGB(0, 255, 0):ToHSV())
print(color.black:Lerp(color.white, 0.5):ToHex())
```

This prints:

```text
#ff8000	#ff8000ff
255	128	0	255
0.3333333333333333	1	1
#808080
```

## Operators

In this table `a` and `b` are colors and `n` is a number.

| Operation | Result |
| --- | --- |
| `a + b` | Adds the matching numbers, alpha included. |
| `a - b` | Subtracts the matching numbers, alpha included. |
| `a * b` | Multiplies the matching numbers, alpha included. |
| `a / b` | Divides the matching numbers, alpha included. |
| `a * n` or `n * a` | Multiplies `R`, `G` and `B` by `n`. Alpha stays the same. |
| `a / n` | Divides `R`, `G` and `B` by `n`. Alpha stays the same. |
| `n / a` | Divides `n` by `R`, `G` and `B`. Alpha stays the same. |
| `a == b` | `true` when all four numbers match. |

Math with a plain number skips alpha. `+` and `-` need a color on both sides, so `a + 1` raises an error. Color has no `-a`, `<`, `%`, `//` or `^`.

```luau
local faded = color.new(1, 1, 1, 0.5)
print(faded * 0.5)
print(color.new(1, 0.5, 1, 1) * color.new(0.5, 0.5, 0, 1))
print(color.new(0.25, 0, 0, 0.5) + color.new(0.25, 0.5, 0, 0.5))
```

This prints:

```text
Color(0.5, 0.5, 0.5, 0.5)
Color(0.5, 0.25, 0, 1)
Color(0.5, 0.5, 0, 1)
```

## tostring

`tostring` gives `Color(r, g, b, a)`. Whole numbers print without a decimal point.
