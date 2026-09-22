# Drawing

Everything you see in a window is a renderable. This page shows how to draw shapes, images and text, how to place and order them, and how to find them again.

## Getting the API

Each window has its own [Renderable API](../reference/renderable-api.md). Get it with [GetAPI](../reference/window.md#getapi):

```luau
local Window = import("Window")

local window = Window.new({ Title = "Drawing", Size = udim.new(800, 450) })
local Renderable = window:GetAPI("Renderable")
```

Every renderable you make with it draws in that window. The window keeps it until you call `Destroy` or the window closes. You do not have to keep a reference to it.

Changes to a renderable apply right away. luv sends them to the GPU once per frame.

## Shapes

A [RenderableShape](../reference/renderableshape.md) draws a rectangle, a circle or one of six other shapes:

```luau
local Window = import("Window")

local window = Window.new({ Title = "Shapes", Size = udim.new(800, 450) })
local Renderable = window:GetAPI("Renderable")

Renderable.new("RenderableShape", {
	Position = udim.new(300, 225),
	Size = udim.new(120, 80),
	Color = color.new(0.2, 0.6, 1, 1),
})
Renderable.new("RenderableShape", {
	Shape = enum.ShapeType.Circle,
	Position = udim.new(500, 225),
	Size = udim.new(90, 90),
	Color = color.new(1, 1, 1, 0),
	StrokeColor = color.new(1, 0.8, 0.2, 1),
	StrokeThickness = 4,
})
```

`StrokeThickness` draws an outline inside the edge of the shape. With a Color alpha of 0, only the outline shows. See [Shapes](../reference/renderableshape.md#shapes) for every shape type.

## Images

A [RenderableImage](../reference/renderableimage.md) draws an image from your assets:

```luau
local Window = import("Window")
local Asset = import("Asset")

local window = Window.new({ Title = "Images", Size = udim.new(800, 450) })
local Renderable = window:GetAPI("Renderable")

local logo = Renderable.new("RenderableImage", {
	Image = Asset.Load("logo.png"),
	Position = udim.new(400, 225),
})
print(logo.ImageSize)
```

Without a Size, the image gets the size of its file. luv reads PNG, JPEG, GIF, WebP, BMP, SVG and more. See [Formats](../reference/renderableimage.md#formats).

The file loads in the background. The image shows up once it is ready.

## Sprite sheets

`OffsetPosition` and `OffsetSize` pick one part of an image. Both use pixels of the image file. Move `OffsetPosition` to play an animation:

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

`ResampleMode.Pixelated` keeps the hard edges of pixel art. `FlipX` makes the sprite face the other way.

## Text

A [RenderableText](../reference/renderabletext.md) draws text with a font file from your assets:

```luau
local Window = import("Window")
local Asset = import("Asset")

local window = Window.new({ Title = "Text" })
local Renderable = window:GetAPI("Renderable")

local score = 0
local label = Renderable.new("RenderableText", {
	Font = Asset.Load("fonts/Inter.ttf"),
	Text = "Score 0",
	TextSize = 24,
	Position = udim.new(16, 16),
	AnchorPoint = udim.new(0, 0),
})
score += 10
label.Text = `Score {score}`
print(label.TextBounds)
```

`TextBounds` is the size of the text in pixels. With the default Size of `udim.new(0, 0)`, the box of the text fits the text. Give it a width and turn on `TextWrapped` to break long lines. See [Wrapping](../reference/renderabletext.md#wrapping).

## Placement

Shapes, images and text share five members: `Position`, `Size`, `AnchorPoint`, `Rotation` and `Color`. See [Placement](../reference/renderable.md#placement).

- Position and Size are in window pixels. (0, 0) is the top left corner of the window. Y grows down.
- AnchorPoint says which point of the renderable sits at Position. The default `udim.new(0.5, 0.5)` is the center. `udim.new(0, 0)` is the top left corner.
- Rotation is in degrees and turns clockwise around the anchor point.

This puts a square in each corner of the window:

```luau
local Window = import("Window")

local window = Window.new({ Title = "Corners", Size = udim.new(800, 450) })
local Renderable = window:GetAPI("Renderable")

for _, anchor in { udim.new(0, 0), udim.new(1, 0), udim.new(0, 1), udim.new(1, 1) } do
	Renderable.new("RenderableShape", {
		Position = anchor * window.Size,
		AnchorPoint = anchor,
		Size = udim.new(40, 40),
	})
end
```

## Drawing order

Renderables with a higher `ZIndex` draw on top. When two have the same ZIndex, the one made later draws on top.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Layers", Size = udim.new(800, 450) })
local Renderable = window:GetAPI("Renderable")

Renderable.new("RenderableShape", { Position = udim.new(430, 245), Color = color.new(1, 0, 0, 1), ZIndex = 1 })
Renderable.new("RenderableShape", { Position = udim.new(400, 225), Color = color.new(0, 0, 1, 1) })
```

The red square draws over the blue one, even though it was made first.

## Blend modes

`BlendMode` sets how a renderable mixes with what is behind it. `Additive` is good for glows and sparks. `Multiply` darkens what is behind it. `Opaque` skips mixing, which suits a background that fills a rectangle. See [Blend modes](../reference/renderable.md#blend-modes).

```luau
local Window = import("Window")

local window = Window.new({ Title = "Glow" })
local Renderable = window:GetAPI("Renderable")

Renderable.new("RenderableShape", {
	Shape = enum.ShapeType.Circle,
	Position = udim.new(200, 150),
	Size = udim.new(120, 120),
	Color = color.new(0.1, 0.85, 1, 0.4),
	BlendMode = enum.BlendMode.Additive,
})
```

## Hiding and removing

Set `SinkUpdates` to `false` to hide a renderable. A hidden renderable is not drawn and queries do not find it. You can still change it. Set it back to `true` to show it again.

Call `Destroy` to remove a renderable for good.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Blink" })
local Renderable = window:GetAPI("Renderable")
local light = Renderable.new("RenderableShape", { Position = udim.new(200, 150) })

window.PreFrame:BindHandler("blink", function()
	light.SinkUpdates = os.clock() % 1 < 0.5
end)
```

## Moving many objects

Setting a property is cheap. luv collects the changes and sends them to the GPU once per frame. [Bulk.BulkUpdate](../reference/bulk.md#bulkupdate) sets many properties on many objects in one call:

```luau
local Window = import("Window")
local Bulk = import("Bulk")

local window = Window.new({ Title = "Wave", Size = udim.new(800, 450) })
local Renderable = window:GetAPI("Renderable")

local dots: { RenderableShape } = {}
for index = 1, 100 do
	dots[index] = Renderable.new("RenderableShape", { Shape = enum.ShapeType.Circle, Size = udim.new(6, 6) })
end

window.PreFrame:BindHandler("wave", function()
	local updates = {}
	for index, dot in dots do
		updates[dot] = { Position = udim.new(index * 8, 225 + math.sin(os.clock() * 3 + index * 0.2) * 100) }
	end
	Bulk.BulkUpdate(updates)
end)
```

## Finding objects

The Renderable API can find renderables at a point, in a rectangle, in a circle or along a ray. Each query yields the calling coroutine until the answer is ready. Results come topmost first.

Queries find shapes by their real outline, and images and text by their box. They never find hidden renderables or the custom `"Renderable"` class. See [Renderable API](../reference/renderable-api.md) for every rule.

This prints the name of the renderable under the mouse when you click:

```luau
local Window = import("Window")

local window = Window.new({ Title = "Pick" })
local Renderable = window:GetAPI("Renderable")
local Mouse = window:GetAPI("Mouse")

Renderable.new("RenderableShape", { Name = "Box", Position = udim.new(200, 150) })

Mouse.ButtonDown:BindHandler("pick", function(_, position: UDim)
	local hits = Renderable.QueryPoint(position)
	if hits[1] then
		print("Clicked", hits[1].Name)
	end
end)
```

A raycast finds the first renderable along a line. The ray starts at the origin and reaches as far as the direction is long:

```luau
local Window = import("Window")

local window = Window.new({ Title = "Ray" })
local Renderable = window:GetAPI("Renderable")

Renderable.new("RenderableShape", { Name = "Wall", Position = udim.new(300, 100), Size = udim.new(20, 200) })
local hit = Renderable.Raycast(udim.new(0, 100), udim.new(600, 0))
if hit then
	print(hit.Renderable.Name, hit.Position, hit.Normal, hit.Distance)
end
```

This prints `Wall`, the point where the ray hit the left edge of the wall, the direction that edge faces and the distance of 290 pixels.
