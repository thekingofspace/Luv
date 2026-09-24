# RenderableShape

Inherits: [Renderable](renderable.md) < [BaseGameObject](basegameobject.md)

A shape with a fill color and an optional outline.

## Description

Make one with [Renderable.new](renderable-api.md#new):

```luau
local Window = import("Window")

local window = Window.new({ Title = "Shape" })
local Renderable = window:GetAPI("Renderable")

local box = Renderable.new("RenderableShape", {
	Position = udim.new(200, 150),
	Size = udim.new(120, 80),
	Color = color.new(0.2, 0.6, 1, 1),
})
```

It also has the [placement](renderable.md#placement) members and every member of [Renderable](renderable.md). The default Size is 100 by 100. Color is the fill color.

The shape fills the box that Size makes. Its edges are smooth. Queries use the real outline of the shape, turned by Rotation.

## Properties

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `Shape` | [ShapeType](enums.md#shapetype) | `enum.ShapeType.Rectangle` | The outline. See [Shapes](#shapes). |
| `Outline` | `{ UDim }?` | `nil` | Your own outline. It wins over Shape. See [Your own outline](#your-own-outline). |
| `StrokeColor` | [Color](color.md) | `color.black` | The color of the outline. |
| `StrokeThickness` | `number` | `0` | The width of the outline in pixels. 0 draws no outline. |

## Shapes

| ShapeType | Outline |
| --- | --- |
| `Rectangle` | The whole box. |
| `Circle` | An oval that touches the middle of each edge of the box. It is a circle when Size is square. |
| `Triangle` | One corner at the top center and a flat bottom edge. |
| `RightTriangle` | Corners at the top left, the bottom left and the bottom right. The square corner is at the bottom left. |
| `Diamond` | Corners at the middle of each edge of the box. |
| `Pentagon` | Five sides with one corner at the top center. |
| `Hexagon` | Six sides with corners at the top center and the bottom center. |
| `Octagon` | Eight sides. They all have the same length when Size is square. |

## Your own outline

`Outline` takes a list of [UDims](udim.md) and uses them as the corners of the shape, in order. Set it back to `nil` to go back to `Shape`.

Each point is a fraction of the box, not a pixel count. `-0.5` is the left or top edge, `0` is the middle and `0.5` is the right or bottom edge. The shape keeps its form when you change Size, the same way the built in shapes do.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Arrow" })
local Renderable = window:GetAPI("Renderable")

Renderable.new("RenderableShape", {
	Position = udim.new(200, 150),
	Size = udim.new(80, 80),
	Color = color.new(1, 0.7, 0.2, 1),
	Outline = {
		udim.new(-0.5, -0.5),
		udim.new(0, -0.5),
		udim.new(0, 0),
		udim.new(0.5, 0),
		udim.new(0.5, 0.5),
		udim.new(-0.5, 0.5),
	},
})
```

The shape can bend inwards. Queries follow the outline you gave, so a point in the dent of that L shape is not inside it, and a ray goes through the dent. Drawing and queries always agree.

Rules:

- It needs at least 3 points, or it raises `an Outline needs at least 3 points, got 2`.
- It holds at most 255 points, or it raises `an Outline can hold at most 255 points, got 300`.
- Every point must be a UDim, or it raises `Outline must only hold UDims, got number`. The Z of each UDim is not used.
- The numbers must be finite, or it raises `Outline points must only hold finite numbers`.
- An empty list is the same as `nil`.
- Points that cross over themselves draw and query in a way that follows the crossings. Keep the outline simple for a result you can predict.

Reading `Outline` gives back the list you set, or `nil`.

## Stroke

The outline is drawn inside the edge of the shape. It covers the outer `StrokeThickness` pixels of the fill, so the shape does not grow. StrokeThickness must be 0 or more, or it raises `StrokeThickness must be a number of at least 0`.

A fill Color with an alpha of 0 leaves only the outline.

## Config

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Shape` | [ShapeType](enums.md#shapetype) | `enum.ShapeType.Rectangle` | The outline. |
| `Outline` | `{ UDim }?` | `nil` | Your own outline. It wins over Shape. |
| `StrokeColor` | [Color](color.md) | `color.black` | The color of the outline. |
| `StrokeThickness` | `number` | `0` | The width of the outline in pixels. |

It also takes the [shared fields](renderable.md#shared-fields) and the [placement fields](renderable.md#placement-fields). `Size` defaults to `udim.new(100, 100)`.

## Examples

A ring with no fill:

```luau
local Window = import("Window")

local window = Window.new({ Title = "Ring" })
local Renderable = window:GetAPI("Renderable")

Renderable.new("RenderableShape", {
	Shape = enum.ShapeType.Circle,
	Position = udim.new(200, 150),
	Size = udim.new(90, 90),
	Color = color.new(1, 1, 1, 0),
	StrokeColor = color.new(1, 0.8, 0.2, 1),
	StrokeThickness = 4,
})
```

Every shape type in a row, spinning:

```luau
local Window = import("Window")

local window = Window.new({ Title = "Shapes", Size = udim.new(800, 200) })
local Renderable = window:GetAPI("Renderable")

local shapes: { RenderableShape } = {}
for index, shape in enum.ShapeType:GetEnumItems() do
	shapes[index] = Renderable.new("RenderableShape", { Shape = shape, Position = udim.new(index * 90, 100), Size = udim.new(70, 70) })
end

window.PreFrame:BindHandler("spin", function(dt: number)
	for _, item in shapes do
		item.Rotation += 45 * dt
	end
end)
```
