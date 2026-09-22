# Your first game

This page makes a small game: a ball you move with the keyboard. It takes about five minutes.

## Make a project

```shell
luv init my-game
cd my-game
```

This makes the folder `my-game` with a `build.toml`, a `src/main.luau` and a few other files. See [Project layout](project-layout.md) for the full list.

## Open a window

Replace everything in `src/main.luau` with this:

```luau
local Window = import("Window")

local window = Window.new({
	Title = "My First Game",
	Size = udim.new(800, 600),
	BackgroundColor = color.fromRGB(20, 22, 35),
})
```

Run it:

```shell
luv test
```

A dark window opens. The game keeps running while the window is open. Close the window and the game ends.

- `import("Window")` gives you the [Window](../reference/window.md) library. Every engine library comes from `import`. See [Globals](../reference/globals.md).
- `Window.new` opens a window. The config table sets its title, size and background.
- `udim.new(800, 600)` is a [UDim](../reference/udim.md). luv uses UDim for every size and position.
- `color.fromRGB` makes a [Color](../reference/color.md).

## Draw a ball

Each window has its own APIs. You get them with `GetAPI`. Add this to the end of the script:

```luau
local Renderable = window:GetAPI("Renderable")

local ball = Renderable.new("RenderableShape", {
	Shape = enum.ShapeType.Circle,
	Position = udim.new(400, 300),
	Size = udim.new(48, 48),
	Color = color.fromHex("#ff4f7e"),
})
```

Run `luv test` again. A pink ball sits in the middle of the window.

`Position` is where the center of the ball goes, in pixels from the top left corner of the window. See [RenderableShape](../reference/renderableshape.md) for every property.

## Move it

Add this to the end of the script:

```luau
local Input = window:GetAPI("Input")
local speed = 300

window.PreFrame:BindHandler("move", function(delta: number)
	local direction = udim.zero
	if Input:IsKeyDown(enum.KeyCode.A) then
		direction -= udim.new(1, 0)
	end
	if Input:IsKeyDown(enum.KeyCode.D) then
		direction += udim.new(1, 0)
	end
	if Input:IsKeyDown(enum.KeyCode.W) then
		direction -= udim.new(0, 1)
	end
	if Input:IsKeyDown(enum.KeyCode.S) then
		direction += udim.new(0, 1)
	end
	ball.Position += direction * speed * delta
end)
```

Run it and press W, A, S and D.

- `PreFrame` is a [Signal](../reference/signal.md) that fires once per frame. `BindHandler` gives the handler a name, `"move"`, so you can remove it later with `UnBind("move")`.
- `delta` is the time since the last frame in seconds. Multiply speeds by it so the ball moves at the same speed at any frame rate.
- `Input:IsKeyDown` tells you if a key is held right now. See [Input API](../reference/input.md).

## Close with Escape

```luau
Input.KeyDown:BindHandler("quit", function(key)
	if key == enum.KeyCode.Escape then
		window:Close()
	end
end)
```

`KeyDown` fires once each time a key is pressed. Closing the last window ends the game.

## The whole script

```luau
local Window = import("Window")

local window = Window.new({
	Title = "My First Game",
	Size = udim.new(800, 600),
	BackgroundColor = color.fromRGB(20, 22, 35),
})

local Renderable = window:GetAPI("Renderable")
local Input = window:GetAPI("Input")
local speed = 300

local ball = Renderable.new("RenderableShape", {
	Shape = enum.ShapeType.Circle,
	Position = udim.new(400, 300),
	Size = udim.new(48, 48),
	Color = color.fromHex("#ff4f7e"),
})

window.PreFrame:BindHandler("move", function(delta: number)
	local direction = udim.zero
	if Input:IsKeyDown(enum.KeyCode.A) then
		direction -= udim.new(1, 0)
	end
	if Input:IsKeyDown(enum.KeyCode.D) then
		direction += udim.new(1, 0)
	end
	if Input:IsKeyDown(enum.KeyCode.W) then
		direction -= udim.new(0, 1)
	end
	if Input:IsKeyDown(enum.KeyCode.S) then
		direction += udim.new(0, 1)
	end
	ball.Position += direction * speed * delta
end)

Input.KeyDown:BindHandler("quit", function(key)
	if key == enum.KeyCode.Escape then
		window:Close()
	end
end)
```

## Pack it

```shell
luv package
```

Your finished game is now in `build/package`. It is one program you can send to a friend. See [Shipping your game](../manual/shipping.md).

## Next steps

- [Drawing](../manual/drawing.md) covers images, text and more shapes.
- [Input](../manual/input.md) covers the mouse, controllers and touch.
- [Sound](../manual/sound.md) shows how to play sounds.
- [Windows and frames](../manual/windows.md) explains the frame signals.
