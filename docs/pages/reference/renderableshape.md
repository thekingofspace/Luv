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

## Stroke

The outline is drawn inside the edge of the shape. It covers the outer `StrokeThickness` pixels of the fill, so the shape does not grow. StrokeThickness must be 0 or more, or it raises `StrokeThickness must be a number of at least 0`.

A fill Color with an alpha of 0 leaves only the outline.

## Config

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `Shape` | [ShapeType](enums.md#shapetype) | `enum.ShapeType.Rectangle` | The outline. |
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
