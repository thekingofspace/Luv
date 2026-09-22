# RenderableText

Inherits: [Renderable](renderable.md) < [BaseGameObject](basegameobject.md)

Text drawn with a font file.

## Description

Make one with [Renderable.new](renderable-api.md#new). The config must have a `Font`:

```luau
local Window = import("Window")
local Asset = import("Asset")

local window = Window.new({ Title = "Text" })
local Renderable = window:GetAPI("Renderable")

local label = Renderable.new("RenderableText", {
	Font = Asset.Load("fonts/Inter.ttf"),
	Text = "Hello",
	TextSize = 32,
	Position = udim.new(16, 16),
	AnchorPoint = udim.new(0, 0),
})
```

It also has the [placement](renderable.md#placement) members and every member of [Renderable](renderable.md). Color is the color of the letters.

The text sits in a box. With the default Size of `udim.new(0, 0)`, the box fits the text. See [Size and TextBounds](#size-and-textbounds).

Letters are drawn sharp at the real pixel size of the screen. When Rotation is 0, they snap to whole screen pixels.

## Properties

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `Font` | [Asset](asset.md) | required | The font file. See [Fonts](#fonts). |
| `Text` | `string` | `""` | The text. `\n` starts a new line. |
| `TextSize` | `number` | `16` | The font size in window pixels. It must be above 0. |
| `Bold` | `boolean` | `false` | Makes the letters thicker. See [Style](#style). |
| `Italic` | `boolean` | `false` | Slants the letters. |
| `Underline` | `boolean` | `false` | Draws a line under each line of text. |
| `Strikethrough` | `boolean` | `false` | Draws a line through each line of text. |
| `TextXAlignment` | [TextXAlignment](enums.md#textxalignment) | `enum.TextXAlignment.Left` | Where each line sits across the box. See [Alignment](#alignment). |
| `TextYAlignment` | [TextYAlignment](enums.md#textyalignment) | `enum.TextYAlignment.Top` | Where the text sits from top to bottom in the box. |
| `TextWrapped` | `boolean` | `false` | Breaks long lines to fit the width of the box. See [Wrapping](#wrapping). |
| `LineHeight` | `number` | `1` | Multiplies the space between lines. It must be 0 or more. |
| `LetterSpacing` | `number` | `0` | Extra pixels after each character. It can be below 0. |
| `BackgroundColor` | [Color](color.md) | `color.transparent` | Fills the box behind the text. |
| `StrokeColor` | [Color](color.md) | `color.black` | The color of the outline around each letter. |
| `StrokeThickness` | `number` | `0` | The width of the outline in pixels. 0 draws no outline. |
| `TextBounds` | [UDim](udim.md) | none | The size of the text in pixels. Read only. |

## Fonts

`Font` takes an [Asset](asset.md) of a TrueType (`.ttf`) or OpenType (`.otf`) font. For a font collection (`.ttc` or `.otc`), luv uses the first font in the file. A file that is not a font raises an error like `'logo.png' is not a font file the engine can read`.

A RenderableText uses one font. There is no fallback font, so characters that the font does not have show its missing character glyph. Letters are drawn in one color.

## Size and TextBounds

`TextBounds` is the size of the text in pixels. Its width is the width of the widest line. Its height is the height of one line times the number of lines. It is read only.

The box of the text is Size. A 0 in Size.X makes the box as wide as TextBounds.X. A 0 in Size.Y makes it as tall as TextBounds.Y. AnchorPoint, Rotation, BackgroundColor and queries all use this box. Text is not cut off at the edge of the box.

This title stays centered in the window, whatever the text is:

```luau
local Window = import("Window")
local Asset = import("Asset")

local window = Window.new({ Title = "Center", Size = udim.new(800, 450) })
local Renderable = window:GetAPI("Renderable")

local title = Renderable.new("RenderableText", {
	Font = Asset.Load("fonts/Inter.ttf"),
	Text = "GAME OVER",
	TextSize = 48,
	Position = window.Size / 2,
})
print(title.TextBounds)
```

## Wrapping

When TextWrapped is on and Size.X is above 0, lines break at spaces so they fit that width. A word that is longer than the width stays whole. A `\n` in Text always starts a new line.

```luau
local Window = import("Window")
local Asset = import("Asset")

local window = Window.new({ Title = "Wrap" })
local Renderable = window:GetAPI("Renderable")

Renderable.new("RenderableText", {
	Font = Asset.Load("fonts/Inter.ttf"),
	Text = "Move with W and S. Press Space to serve the ball.",
	TextWrapped = true,
	Size = udim.new(200, 0),
	Position = udim.new(200, 150),
})
```

## Alignment

TextXAlignment moves each line across the width of the box. TextYAlignment moves all lines inside the height of the box. They only change something when the box is bigger than the text. With a Size.X of 0, the box is as wide as the widest line, so TextXAlignment still lines up the shorter lines.

| TextXAlignment | Each line sits at |
| --- | --- |
| `Left` | The left edge. This is the default. |
| `Center` | The middle. |
| `Right` | The right edge. |

| TextYAlignment | The text sits at |
| --- | --- |
| `Top` | The top edge. This is the default. |
| `Center` | The middle. |
| `Bottom` | The bottom edge. |

## Style

| Property | What it does |
| --- | --- |
| `Bold` | Makes the letters thicker and adds a little space after each one. TextBounds grows to match. |
| `Italic` | Slants the letters. TextBounds does not change. |
| `Underline` | Draws a line under each line of text, in Color. The font sets its height and thickness. |
| `Strikethrough` | Draws a line through each line of text, in Color. |
| `LineHeight` | `1` is the normal line height of the font. `1.5` adds half a line of space. |
| `LetterSpacing` | Adds this many pixels after each character. |
| `StrokeThickness` | Draws an outline around each letter in StrokeColor. It sits under the letters and grows outward by about this many pixels. |
| `BackgroundColor` | Fills the whole box behind the text. |

luv makes Bold and Italic itself, so they work with any font.

A value out of range raises an error like `TextSize must be a number greater than 0`, `LineHeight must be a number of at least 0` or `LetterSpacing must be a finite number`.

luv keeps the letters it has drawn in one texture. When a lot of different text at many sizes is on screen at once, the texture can run out of room. luv then reports `too much text is visible at once, some glyphs could not be drawn`.

## Config

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Font` | [Asset](asset.md) | required | The font file. |
| `Text` | `string` | `""` | The text. |
| `TextSize` | `number` | `16` | The font size in window pixels. |
| `Bold` | `boolean` | `false` | Thicker letters. |
| `Italic` | `boolean` | `false` | Slanted letters. |
| `Underline` | `boolean` | `false` | A line under the text. |
| `Strikethrough` | `boolean` | `false` | A line through the text. |
| `TextXAlignment` | [TextXAlignment](enums.md#textxalignment) | `enum.TextXAlignment.Left` | Where each line sits across the box. |
| `TextYAlignment` | [TextYAlignment](enums.md#textyalignment) | `enum.TextYAlignment.Top` | Where the text sits from top to bottom. |
| `TextWrapped` | `boolean` | `false` | Breaks long lines. |
| `LineHeight` | `number` | `1` | The space between lines. |
| `LetterSpacing` | `number` | `0` | Extra pixels after each character. |
| `BackgroundColor` | [Color](color.md) | `color.transparent` | Fills the box. |
| `StrokeColor` | [Color](color.md) | `color.black` | The outline color. |
| `StrokeThickness` | `number` | `0` | The outline width in pixels. |

It also takes the [shared fields](renderable.md#shared-fields) and the [placement fields](renderable.md#placement-fields). `Size` defaults to `udim.new(0, 0)`, which fits the box to the text.

## Examples

A score label with an outline:

```luau
local Window = import("Window")
local Asset = import("Asset")

local window = Window.new({ Title = "Score" })
local Renderable = window:GetAPI("Renderable")

local score = 0
local label = Renderable.new("RenderableText", {
	Font = Asset.Load("fonts/Inter.ttf"),
	Text = "SCORE 0",
	TextSize = 32,
	Position = udim.new(16, 16),
	AnchorPoint = udim.new(0, 0),
	StrokeThickness = 2,
})

score += 10
label.Text = `SCORE {score}`
```
