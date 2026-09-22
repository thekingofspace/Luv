# Render hooks

A render hook is a native function that luv calls every frame, right before it draws. It can write shader data and textures for one object and set how much that object draws. The `cube` and `cube-rust` examples draw a spinning 3D cube this way.

You need a native plugin for this. See [Native plugins](native-plugins.md) first.

## Setting a hook

Every renderable has two properties for hooks. So does a [PostProcess](../reference/postprocess.md).

| Property | Accepts | What it does |
| --- | --- | --- |
| `RenderHook` | [Pointer](../reference/pointer.md), [NativeFunction](../reference/nativefunction.md), [Callback](../reference/callback.md) or `nil` | The function luv calls. Get it with [GetSymbol](../reference/library.md#getsymbol). `nil` removes the hook. |
| `RenderHookData` | A Pointer, NativeFunction, Callback, plugin object or `nil` | The address luv passes to the hook as `user_data`. `nil` passes NULL. |

- You can set both in the config of `Renderable.new`, or later.
- Reading either property gives a [Pointer](../reference/pointer.md).
- While they are set, luv keeps the library, the memory or the plugin object alive.
- A null pointer for `RenderHook` errors with `RenderHook cannot be a null Pointer`.
- Other values error with `RenderHook must be a Pointer, NativeFunction, Callback or nil, got number`.

Use an exported native function as the hook. A Callback works too, but then the renderer waits for Luau every frame.

## When and where the hook runs

- The hook runs on the renderer thread of the window, not on the thread that runs Luau.
- It runs once for each object that has a hook, every frame the window draws.
- It runs before luv uploads the shader data of the frame. So what it writes shows up in the same frame.
- Hooks run in `ZIndex` order, then in the order the objects were made.
- luv holds its GPU queue lock while hooks run.

Luau keeps running while a hook runs. Luau code can change the memory of `RenderHookData` while the hook reads it. Guard data that both sides change with a lock. The pong particle system below does this.

## The context

The hook gets a `LuvRenderContext*`. It is only valid while the hook runs.

| Field | What it holds |
| --- | --- |
| `user_data` | The address from `RenderHookData`, or NULL. |
| `time` | Seconds since the window opened. |
| `delta` | Seconds since the last frame. |
| `frame` | The frame number. |
| `width`, `height` | The size of the window, in the same units as UDim positions. |
| `scale` | The display scale of the window. |
| `position`, `size`, `anchor`, `rotation` | The placement of a RenderableShape, RenderableImage or RenderableText. `rotation` is in degrees. They are 0 for a plain Renderable. |
| `vulkan` | The Vulkan handles of luv, or NULL. |

Every field is listed on [LuvRenderContext](../reference/native-c.md#luvrendercontext).

## Writing shader data

`write_data` copies bytes into shader data of the object. The name can be a binding, a field of a uniform or storage buffer, or a path like `params.tint`. luv looks for it in every shader loaded on the object.

This shader has a uniform buffer and a texture:

```wgsl
struct Params {
    tint: vec4<f32>,
}

@group(1) @binding(0) var<uniform> params: Params;
@group(1) @binding(1) var pattern: texture_2d<f32>;
```

The hook writes the `tint` field:

```c
float tint[4] = {1.0f, 0.0f, 1.0f, 1.0f};
context->write_data(context, "tint", 0, tint, sizeof tint);
```

- `offset` counts bytes from the start of the named data.
- The bytes must follow the WGSL memory layout.
- A storage buffer that ends with an array without a length grows to fit, up to 256 MiB.
- It returns `LUV_OK`, or an error code like `LUV_UNKNOWN_NAME`. See [write_data](../reference/native-c.md#write-data).

A failed write is also reported as a game error, once for each message. For example: `a RenderHook wrote 'missing', but shader 'hooked' has no data named 'missing', it declares params, pattern`.

## Writing textures

`write_texture` uploads RGBA pixels, 4 bytes each, to a `texture_2d<f32>` binding. luv makes a new texture when the size changes.

```c
unsigned char pattern[16] = {255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255};
context->write_texture(context, "pattern", 2, 2, pattern);
```

See [write_texture](../reference/native-c.md#write-texture) for the return codes.

## Draw counts

`set_draw_counts` sets how many vertices and instances a plain [Renderable](../reference/renderable.md) draws. Make the object with `VertexCount = 0`, and the hook decides how much gets drawn each frame. It does nothing for shapes, images, text and post processes. It does not change the `VertexCount` and `InstanceCount` that Luau reads.

## A full example

This hook draws a spinning triangle. It reads its settings from `RenderHookData`.

The shader reads the vertices from a storage buffer:

```wgsl
struct TriangleVertex {
    position: vec4<f32>,
    color: vec4<f32>,
}

struct TriangleOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@group(1) @binding(0) var<storage, read> vertices: array<TriangleVertex, 3>;

@vertex
fn triangle_vertex(@builtin(vertex_index) index: u32) -> TriangleOutput {
    let vertex = vertices[index];
    var out: TriangleOutput;
    out.position = vec4<f32>(
        vertex.position.x / frame.resolution.x * 2.0 - 1.0,
        1.0 - vertex.position.y / frame.resolution.y * 2.0,
        0.0,
        1.0,
    );
    out.color = vertex.color;
    return out;
}

@fragment
fn triangle_fragment(in: TriangleOutput) -> @location(0) vec4<f32> {
    return in.color;
}
```

The hook fills `vertices` and sets the draw counts. The C and the Rust version do the same thing:

```c title="native/triangle.c"
#include <math.h>
#include "luv.h"

typedef struct Vertex {
    float position[4];
    float color[4];
} Vertex;

typedef struct Settings {
    float center[2];
    float size;
    float speed;
} Settings;

LUV_EXPORT void triangle_render(LuvRenderContext* context) {
    const Settings* settings = (const Settings*)context->user_data;
    float cx = settings ? settings->center[0] : context->width * 0.5f;
    float cy = settings ? settings->center[1] : context->height * 0.5f;
    float size = settings ? settings->size : 100.0f;
    float spin = (float)context->time * (settings ? settings->speed : 1.0f);
    Vertex vertices[3];
    for (int index = 0; index < 3; index++) {
        float angle = spin + index * 2.0943951f;
        Vertex* vertex = &vertices[index];
        vertex->position[0] = cx + cosf(angle) * size;
        vertex->position[1] = cy + sinf(angle) * size;
        vertex->position[2] = 0.0f;
        vertex->position[3] = 1.0f;
        vertex->color[0] = index == 0 ? 1.0f : 0.2f;
        vertex->color[1] = index == 1 ? 1.0f : 0.2f;
        vertex->color[2] = index == 2 ? 1.0f : 0.2f;
        vertex->color[3] = 1.0f;
    }
    if (context->write_data(context, "vertices", 0, vertices, sizeof vertices) == LUV_OK) {
        context->set_draw_counts(context, 3, 1);
    }
}
```

```rust title="native/triangle/src/lib.rs"
#[path = "../../luv.rs"]
mod luv;

use luv::{LUV_OK, LuvRenderContext};

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    position: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
struct Settings {
    center: [f32; 2],
    size: f32,
    speed: f32,
}

#[unsafe(no_mangle)]
pub extern "C" fn triangle_render(context: *mut LuvRenderContext) {
    let Some(context) = (unsafe { context.as_mut() }) else {
        return;
    };
    let (center, size, speed) = match context.user_data::<Settings>() {
        Some(settings) => (settings.center, settings.size, settings.speed),
        None => ([context.width * 0.5, context.height * 0.5], 100.0, 1.0),
    };
    let spin = context.time as f32 * speed;
    let vertices: [Vertex; 3] = std::array::from_fn(|index| {
        let angle = spin + index as f32 * std::f32::consts::TAU / 3.0;
        let mut color = [0.2, 0.2, 0.2, 1.0];
        color[index] = 1.0;
        Vertex {
            position: [center[0] + angle.cos() * size, center[1] + angle.sin() * size, 0.0, 1.0],
            color,
        }
    });
    if context.write_data(c"vertices", 0, &vertices) == LUV_OK {
        context.set_draw_counts(3, 1);
    }
}
```

The game puts the shader in `assets/triangle.wgsl`. It makes the settings with a [StructType](../reference/structtype.md) that matches `Settings`, and moves the center when the window changes size:

```luau
local Asset = import("Asset")
local DLL = import("DLL")
local Shader = import("Shader")
local Window = import("Window")

local window = Window.new({ Title = "Triangle", Size = udim.new(640, 480) })
local Renderable = window:GetAPI("Renderable")
local native = DLL.Load("./triangle")

local Vec2 = DLL.Struct({ { "x", "f32" }, { "y", "f32" } })
local Settings = DLL.Struct({ { "center", Vec2 }, { "size", "f32" }, { "speed", "f32" } })
local settings = Settings:New({ center = window.Size / 2, size = 120, speed = 2 })
local shader = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("triangle.wgsl") }, { Name = "triangle" }))

Renderable.new("Renderable", {
	Name = "Triangle",
	Shaders = { shader },
	VertexCount = 0,
	RenderHook = native:GetSymbol("triangle_render"),
	RenderHookData = settings,
})

window.WindowUpdate:BindHandler("recenter", function(size: UDim)
	settings:Write(Vec2, size / 2, Settings:Offset("center"))
end)
```

- `window.Size / 2` fills the `center` struct, because a [UDim](../reference/udim.md) fills structs of float fields.
- `settings:Write` changes the memory that the hook reads. The next frame uses the new center.
- `VertexCount = 0` leaves the draw counts to the hook.

## Particles from pong

The pong example keeps its particles in a plugin object and draws them with a render hook. The object itself is the `RenderHookData`:

```luau
local Asset = import("Asset")
local DLL = import("DLL")
local Shader = import("Shader")
local Window = import("Window")

local window = Window.new({ Title = "Sparks", Size = udim.new(960, 600) })
local Renderable = window:GetAPI("Renderable")
local fx = DLL.Load("./particles")
local particles = fx.Exports.ParticleSystem.new()
particles.Drag = 2.4

local shader = Shader.Compile(Shader.Combine({ Shader.Prelude, Asset.Load("shaders/particles.wgsl") }, { Name = "particles" }))
Renderable.new("Renderable", {
	Name = "Particles",
	Shaders = { shader },
	VertexCount = 0,
	BlendMode = enum.BlendMode.Additive,
	RenderHook = fx:GetSymbol("particles_render"),
	RenderHookData = particles,
})

particles:Burst(udim.new(480, 300), 46, color.new(0.1, 0.85, 1, 1), 520, 0, 1.7, 0.55, 5)
```

The shader reads the particles from a storage buffer that ends with an array without a length:

```wgsl
struct Particle {
    position: vec2<f32>,
    size: f32,
    fade: f32,
    color: vec4<f32>,
}

struct Particles {
    items: array<Particle>,
}

@group(1) @binding(0) var<storage, read> particles: Particles;
```

The hook moves the particles, writes the live ones to `items` and draws one quad of 6 vertices for each of them:

```c
LUV_EXPORT void particles_render(LuvRenderContext* context) {
    ParticleSystem* system = (ParticleSystem*)context->user_data;
    if (!system || !system->ready) {
        context->set_draw_counts(context, 0, 0);
        return;
    }
    float delta = (float)context->delta;
    if (delta < 0.0f) {
        delta = 0.0f;
    }
    if (delta > 0.1f) {
        delta = 0.1f;
    }
    lock_enter(&system->lock);
    float damping = expf(-system->drag * delta);
    int32_t alive = 0;
    for (int32_t index = 0; index < system->count; index++) {
        Particle particle = system->items[index];
        particle.age += delta;
        if (particle.age >= particle.life) {
            continue;
        }
        particle.vx *= damping;
        particle.vy = particle.vy * damping + system->gravity * delta;
        particle.x += particle.vx * delta;
        particle.y += particle.vy * delta;
        system->items[alive] = particle;
        float progress = particle.age / particle.life;
        GpuParticle* upload = &system->upload[alive];
        upload->position[0] = particle.x;
        upload->position[1] = particle.y;
        upload->size = particle.size * (1.0f - particle.shrink * progress);
        upload->fade = (1.0f - progress) * (1.0f - progress);
        upload->color[0] = particle.r;
        upload->color[1] = particle.g;
        upload->color[2] = particle.b;
        upload->color[3] = particle.a;
        alive++;
    }
    system->count = alive;
    lock_leave(&system->lock);
    if (alive > 0) {
        context->write_data(context, "items", 0, system->upload, (uint64_t)alive * sizeof(GpuParticle));
    }
    context->set_draw_counts(context, 6, (uint32_t)alive);
}
```

- `user_data` is the data of the `ParticleSystem` object, because the object is the `RenderHookData`.
- `Burst` and `Trail` are `LUV_INLINE` methods. They run on the Luau thread while the hook runs on the renderer thread. So both take the same lock. `lock_enter` and `lock_leave` wrap an SRWLOCK on Windows and a pthread mutex on Linux.
- `items` grows to fit however many particles are alive.
- The full source is in `examples/pong/native/particles.c` in the luv repository.
