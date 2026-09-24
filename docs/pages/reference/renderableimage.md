# RenderableImage

Inherits: [Renderable](renderable.md) < [BaseGameObject](basegameobject.md)

An image, or a part of one, drawn in a window.

## Description

Make one with [Renderable.new](renderable-api.md#new). The config must have an `Image`:

```luau
local Window = import("Window")
local Asset = import("Asset")

local window = Window.new({ Title = "Image" })
local Renderable = window:GetAPI("Renderable")

local logo = Renderable.new("RenderableImage", {
	Image = Asset.Load("logo.png"),
	Position = udim.new(200, 150),
})
print(logo.ImageSize, logo.Size)
```

It also has the [placement](renderable.md#placement) members and every member of [Renderable](renderable.md). Color tints the image. White keeps the colors of the file, and a lower alpha fades the image. Queries treat the image as its whole box.

When the config has no Size, the image gets the size of its file. Changing `Image` later keeps the current Size.

luv checks the file when you set `Image`, and `ImageSize` is known right away. The pixels load in the background, and the image shows up once they are ready. When the file turns out to be broken, luv reports `cannot draw the image 'logo.png': ...` while the game runs.

Renderables that use the same Asset object share one texture on the GPU.

## Properties

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `Image` | [Asset](asset.md) | required | The image file. See [Formats](#formats). |
| `ImageSize` | [UDim](udim.md) | none | The size of the image file in pixels. Read only. |
| `OffsetPosition` | [UDim](udim.md) | `udim.new(0, 0)` | The top left pixel of the part of the image to draw. |
| `OffsetSize` | [UDim](udim.md) | `udim.new(0, 0)` | The size of the part to draw, in pixels of the image. 0 means up to the edge of the image. |
| `ResampleMode` | [ResampleMode](enums.md#resamplemode) | `enum.ResampleMode.Smooth` | Smooth blends pixels when the image is scaled. Pixelated keeps hard pixel edges. |
| `FlipX` | `boolean` | `false` | Mirrors the image from left to right. |
| `FlipY` | `boolean` | `false` | Mirrors the image from top to bottom. |
| `HitThreshold` | `number?` | `nil` | How solid a pixel must be for a query to find it. See [Clear pixels and queries](#clear-pixels-and-queries). |

`Image` only takes an Asset. Other objects raise `Image must be an Asset`.

## Sprite sheets

OffsetPosition and OffsetSize pick one part of the image to draw. Both use pixels of the image file. OffsetPosition is the top left pixel of the part. OffsetSize is the size of the part. A 0 in OffsetSize means up to the edge of the image. The part is stretched to fill Size. Parts that reach outside the image repeat its edge pixels.

To play an animation, move OffsetPosition over the sheet:

```luau
local Window = import("Window")
local Asset = import("Asset")

local window = Window.new({ Title = "Walk" })
local Renderable = window:GetAPI("Renderable")
local hero = Renderable.new("RenderableImage", {
	Image = Asset.Load("sprites/hero.png"),
	Position = udim.new(200, 150),
	Size = udim.new(64, 64),
	OffsetSize = udim.new(16, 16),
	ResampleMode = enum.ResampleMode.Pixelated,
})

local frame, clock = 0, 0
window.PreFrame:BindHandler("walk", function(dt: number)
	clock += dt
	if clock >= 0.1 then
		clock -= 0.1
		frame = (frame + 1) % 4
		hero.OffsetPosition = udim.new(frame * 16, 0)
	end
end)
```

Use `enum.ResampleMode.Pixelated` for pixel art. Set `FlipX` to make a sprite face the other way.

## Clear pixels and queries

Without `HitThreshold` a query finds the whole box of the image, clear pixels and all.

Set `HitThreshold` to a number from 0 to 1 and queries read the image instead. A pixel counts only when its alpha is at least the threshold. `0.5` means a pixel that is at least half solid.

```luau
local hero = Renderable.new("RenderableImage", {
	Image = Asset.Load("hero.png"),
	Size = udim.new(64, 64),
	HitThreshold = 0.5,
})
```

It follows `OffsetPosition`, `OffsetSize`, `FlipX` and `FlipY`, so one frame of a sheet is read and not the whole file. Move `OffsetPosition` to the next frame and the queries move with it.

Each query reads the image its own way:

| Query | What it does |
| --- | --- |
| [QueryPoint](renderable-api.md#querypoint) | Reads the one pixel under the point. |
| [Raycast](renderable-api.md#raycast) and [RaycastAll](renderable-api.md#raycastall) | Walks the ray through the image a pixel at a time and stops at the first solid one. `Position` and `Distance` are that pixel. `Normal` stays the edge of the box the ray came in through. |
| [QueryRadius](renderable-api.md#queryradius) | Looks for a solid pixel inside the circle. |
| [QueryArea](renderable-api.md#queryarea) | Looks for a solid pixel inside the rectangle. |

Notes:

- luv reads the image file a second time to keep its alpha. That costs one byte per pixel and only images with a `HitThreshold` pay it.
- The first query after you set it waits for that read. It yields the calling coroutine like every other query.
- Set it back to `nil` to go back to the whole box.
- A number outside 0 to 1 raises `HitThreshold must be a number between 0 and 1, or nil`.
- A threshold of `0` still needs an alpha above 0, so fully clear pixels never count.

## Formats

luv looks at the content of the file to find its format. Formats with no signature, like TGA, also work when the file has the right extension.

| Format | Extensions |
| --- | --- |
| PNG | `.png` |
| JPEG | `.jpg`, `.jpeg` |
| GIF | `.gif`. Only the first frame is drawn. |
| WebP | `.webp` |
| BMP | `.bmp` |
| ICO | `.ico` |
| TIFF | `.tif`, `.tiff` |
| TGA | `.tga` |
| DDS | `.dds` |
| HDR | `.hdr` |
| OpenEXR | `.exr` |
| PNM | `.pbm`, `.pgm`, `.ppm`, `.pam` |
| QOI | `.qoi` |
| farbfeld | `.ff` |
| SVG | `.svg`, and gzipped `.svgz` |

- HDR and OpenEXR images are turned into 8 bit colors.
- An SVG is drawn once at its own size. The size comes from its `width` and `height`, or from its `viewBox`. With neither, it is 100 by 100. It can be at most 16384 pixels on each side.
- Text inside an SVG uses the fonts of the system.
- The largest image depends on the GPU. A bigger image reports `an image of WxH cannot be drawn, images must be between 1x1 and NxN pixels`, where N is the limit of the GPU.

A file luv cannot read raises an error when you set `Image`:

```text
'notes.txt' is not an image the engine can draw: the format is not recognised, images can be PNG, JPEG, GIF, WebP, BMP, ICO, TIFF, TGA, DDS, HDR, EXR, PNM, QOI, farbfeld or SVG
```

## Config

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Image` | [Asset](asset.md) | required | The image file. |
| `OffsetPosition` | [UDim](udim.md) | `udim.new(0, 0)` | The top left pixel of the part to draw. |
| `OffsetSize` | [UDim](udim.md) | `udim.new(0, 0)` | The size of the part to draw. |
| `ResampleMode` | [ResampleMode](enums.md#resamplemode) | `enum.ResampleMode.Smooth` | How the image is scaled. |
| `FlipX` | `boolean` | `false` | Mirrors the image from left to right. |
| `FlipY` | `boolean` | `false` | Mirrors the image from top to bottom. |
| `HitThreshold` | `number?` | `nil` | How solid a pixel must be for a query to find it. |

It also takes the [shared fields](renderable.md#shared-fields) and the [placement fields](renderable.md#placement-fields). Without a `Size`, the image gets the size of its file.

## Examples

A red tinted enemy that faces left:

```luau
local Window = import("Window")
local Asset = import("Asset")

local window = Window.new({ Title = "Enemy" })
local Renderable = window:GetAPI("Renderable")

Renderable.new("RenderableImage", {
	Image = Asset.Load("sprites/slime.png"),
	Position = udim.new(300, 200),
	FlipX = true,
	Color = color.new(1, 0.5, 0.5, 1),
})
```

A background that covers the whole window from its top left corner:

```luau
local Window = import("Window")
local Asset = import("Asset")

local window = Window.new({ Title = "Sky", Size = udim.new(800, 450) })
local Renderable = window:GetAPI("Renderable")

Renderable.new("RenderableImage", {
	Image = Asset.Load("sky.jpg"),
	AnchorPoint = udim.new(0, 0),
	Size = window.Size,
	ZIndex = -10,
})
```
