# Renderable API

Makes shapes, images and text in a window, and finds them with queries and raycasts.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Game" })
local Renderable = window:GetAPI("Renderable")
```

## Description

Every window has its own Renderable API. The renderables it makes belong to that window. They only draw there, and its queries only find renderables of that window.

The window keeps its renderables alive. A renderable stays until you call [Destroy](basegameobject.md#destroy) on it or the window closes. You do not need to keep a reference to it. Closing the window destroys all of them. After that, [new](#new) raises `renderables cannot be created because the window is closed`.

Positions and sizes are in window pixels, the same units as the window size. The point (0, 0) is the top left corner of the window. X grows to the right and Y grows down. See [Placement](renderable.md#placement).

The query functions find renderables by their shape:

- A [RenderableShape](renderableshape.md) counts only inside its outline. A Circle counts only inside the circle.
- A [RenderableImage](renderableimage.md) or a [RenderableText](renderabletext.md) counts as its whole box.
- Position, Size, AnchorPoint and Rotation all count. A point on the edge counts as inside.
- Color, transparency, stroke and shaders do not matter. A fully transparent shape is still found.

Queries never find:

- The custom `"Renderable"` class.
- [PostProcess](postprocess.md) objects.
- Renderables with `SinkUpdates` set to `false`.
- Renderables with a width or a height of 0.

A query sees every change you made before the call, even before the first frame is drawn. Each query yields the calling coroutine until the answer is ready.

QueryPoint, QueryArea and QueryRadius return the topmost renderable first. That is the one with the highest ZIndex. When two have the same ZIndex, the one made later comes first. Raycast results are sorted by distance.

## Functions

| Function | Returns | Yields |
| --- | --- | --- |
| [new](#new)(className, config) | the new renderable | no |
| [GetRenderables](#getrenderables)() | `{ Renderable }` | no |
| [QueryPoint](#querypoint)(point, params) | `{ Renderable }` | yes |
| [QueryArea](#queryarea)(center, size, rotation, params) | `{ Renderable }` | yes |
| [QueryRadius](#queryradius)(center, radius, params) | `{ Renderable }` | yes |
| [Raycast](#raycast)(origin, direction, params) | [RaycastResult](#raycastresult)`?` | yes |
| [RaycastAll](#raycastall)(origin, direction, params) | `{ RaycastResult }` | yes |

## Function descriptions

### new

```luau
Renderable.new(className: string, config: { [string]: any }?): Renderable
```

Makes a renderable of the given class and returns it. It does not yield.

| Class name | Makes | Required config |
| --- | --- | --- |
| `"Renderable"` | A [Renderable](renderable.md#the-renderable-class) that only your shaders draw | none |
| `"RenderableShape"` | A [RenderableShape](renderableshape.md) | none |
| `"RenderableImage"` | A [RenderableImage](renderableimage.md) | `Image` |
| `"RenderableText"` | A [RenderableText](renderabletext.md) | `Font` |

`config` sets properties when the renderable is made. Any property you can set works as a key. The `Shaders` key takes a list of shaders and loads them in order, like calling [LoadShader](renderable.md#loadshader) for each one. Each class page lists its keys in a Config table. The keys that every class shares are in [Config](renderable.md#config).

luv sets `Image`, `Font` and `Shape` first and the other keys after them. A RenderableImage made without a `Size` gets the size of its image.

It errors when:

- The class name is wrong: `'Sprite' is not a renderable class, expected Renderable, RenderableShape, RenderableImage or RenderableText`.
- A RenderableImage has no `Image`: `a RenderableImage needs an Image asset in its config`.
- A RenderableText has no `Font`: `a RenderableText needs a Font asset in its config`.
- A key is not a member of the class: `Font is not a valid member of RenderableShape 'RenderableShape'`.
- A value has the wrong type, like a `Size` that is not a [UDim](udim.md).
- The window is closed.

When a config value fails, the new renderable is destroyed before the error is raised.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Ball" })
local Renderable = window:GetAPI("Renderable")

local ball = Renderable.new("RenderableShape", {
	Name = "Ball",
	Shape = enum.ShapeType.Circle,
	Position = udim.new(200, 150),
	Size = udim.new(32, 32),
	Color = color.new(1, 0.8, 0.2, 1),
})
```

### GetRenderables

```luau
Renderable.GetRenderables(): { Renderable }
```

Returns every renderable of the window in the order they were made. Hidden renderables are in the list too. [PostProcess](postprocess.md) objects are not.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Count" })
local Renderable = window:GetAPI("Renderable")

Renderable.new("RenderableShape")
print(#Renderable.GetRenderables())
```

### QueryPoint

```luau
Renderable.QueryPoint(point: UDim, params: QueryParams?): { Renderable }
```

Returns every renderable that contains `point`, topmost first. It yields the calling coroutine.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Pick" })
local Renderable = window:GetAPI("Renderable")
local Mouse = window:GetAPI("Mouse")

Mouse.ButtonDown:BindHandler("pick", function(_, position: UDim)
	local hits = Renderable.QueryPoint(position)
	if hits[1] then
		print("Clicked", hits[1].Name)
	end
end)
```

### QueryArea

```luau
Renderable.QueryArea(center: UDim, size: UDim, rotation: number?, params: QueryParams?): { Renderable }
Renderable.QueryArea(center: UDim, size: UDim, params: QueryParams): { Renderable }
```

Returns every renderable that overlaps a rectangle, topmost first. The rectangle is `size` big and centered on `center`. `rotation` turns it clockwise around its center, in degrees. The default is 0. When you do not need a rotation, you can pass `params` as the third argument. It yields the calling coroutine.

A third argument that is not a number, a table or `nil` raises `QueryArea takes an optional rotation and query params, got string`.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Area" })
local Renderable = window:GetAPI("Renderable")

for _, renderable in Renderable.QueryArea(udim.new(400, 300), udim.new(200, 50), 45) do
	print(renderable.Name)
end
```

### QueryRadius

```luau
Renderable.QueryRadius(center: UDim, radius: number, params: QueryParams?): { Renderable }
```

Returns every renderable that overlaps a circle around `center`, topmost first. `radius` is in pixels and must be 0 or more. It yields the calling coroutine.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Blast" })
local Renderable = window:GetAPI("Renderable")

for _, renderable in Renderable.QueryRadius(udim.new(300, 200), 80) do
	renderable:Destroy()
end
```

### Raycast

```luau
Renderable.Raycast(origin: UDim, direction: UDim, params: QueryParams?): RaycastResult?
```

Casts a ray from `origin` and returns the first hit, or `nil` when it hits nothing. The ray ends at `origin + direction`, so the length of `direction` is how far it reaches. It yields the calling coroutine.

The ray must come into a renderable from outside to hit it. A renderable that contains `origin` is not hit.

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

### RaycastAll

```luau
Renderable.RaycastAll(origin: UDim, direction: UDim, params: QueryParams?): { RaycastResult }
```

Works like [Raycast](#raycast), but returns every hit along the ray. The nearest hit comes first. When two hits have the same distance, the topmost comes first. It yields the calling coroutine.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Rays" })
local Renderable = window:GetAPI("Renderable")

for _, result in Renderable.RaycastAll(udim.new(0, 100), udim.new(600, 0)) do
	print(result.Renderable.Name, result.Distance)
end
```

## QueryParams

Filters for every query function. Both fields are optional.

| Name | Type | Description |
| --- | --- | --- |
| `Include` | `{ Renderable }?` | When set, only these renderables can be returned. |
| `Exclude` | `{ Renderable }?` | These renderables are never returned. |

A list that holds something else raises an error like `Exclude must only hold renderables, got string`.

```luau
local Window = import("Window")

local window = Window.new({ Title = "Filter" })
local Renderable = window:GetAPI("Renderable")

local player = Renderable.new("RenderableShape", { Position = udim.new(100, 100) })
local params: QueryParams = { Exclude = { player } }
local nearby = Renderable.QueryRadius(udim.new(100, 100), 150, params)
print(#nearby)
```

## RaycastResult

What [Raycast](#raycast) and [RaycastAll](#raycastall) return for each hit.

| Name | Type | Description |
| --- | --- | --- |
| `Renderable` | [Renderable](renderable.md) | The renderable that was hit. |
| `Position` | [UDim](udim.md) | Where the ray hit, in window pixels. Z is 0. |
| `Normal` | [UDim](udim.md) | The direction the hit edge faces, with a length of 1. Z is 0. |
| `Distance` | `number` | How far the hit is from `origin`, in pixels. |
