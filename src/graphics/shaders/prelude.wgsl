struct Frame {
    resolution: vec2<f32>,
    scale: f32,
    time: f32,
    delta: f32,
    frame: u32,
    padding: vec2<f32>,
}

struct Instance {
    position: vec2<f32>,
    size: vec2<f32>,
    anchor: vec2<f32>,
    rotation: f32,
    kind: u32,
    color: vec4<f32>,
    stroke_color: vec4<f32>,
    uv: vec4<f32>,
    shape: u32,
    stroke: f32,
    flags: u32,
    object: u32,
}

struct Object {
    position: vec2<f32>,
    size: vec2<f32>,
    anchor: vec2<f32>,
    rotation: f32,
    shape: u32,
    color: vec4<f32>,
    kind: u32,
    z_index: f32,
    id_low: u32,
    id_high: u32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) local: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) @interpolate(flat) instance: u32,
}

const NO_OBJECT: u32 = 0xffffffffu;
const NO_SHAPE: u32 = 0xffffffffu;

const SHAPE_RECTANGLE: u32 = 0u;
const SHAPE_CIRCLE: u32 = 1u;
const SHAPE_TRIANGLE: u32 = 2u;
const SHAPE_RIGHT_TRIANGLE: u32 = 3u;
const SHAPE_DIAMOND: u32 = 4u;
const SHAPE_PENTAGON: u32 = 5u;
const SHAPE_HEXAGON: u32 = 6u;
const SHAPE_OCTAGON: u32 = 7u;

const KIND_NONE: u32 = 0u;
const KIND_RENDERABLE: u32 = 1u;
const KIND_SHAPE: u32 = 2u;
const KIND_IMAGE: u32 = 3u;
const KIND_TEXT: u32 = 4u;

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> instances: array<Instance>;
@group(0) @binding(2) var image: texture_2d<f32>;
@group(0) @binding(3) var image_sampler: sampler;
@group(0) @binding(4) var<storage, read> objects: array<Object>;
@group(0) @binding(5) var backdrop: texture_2d<f32>;

var<private> SHAPE_POINTS: array<vec2<f32>, 33> = array<vec2<f32>, 33>(
    vec2<f32>(-0.5, -0.5),
    vec2<f32>(0.5, -0.5),
    vec2<f32>(0.5, 0.5),
    vec2<f32>(-0.5, 0.5),
    vec2<f32>(0.0, -0.5),
    vec2<f32>(0.5, 0.5),
    vec2<f32>(-0.5, 0.5),
    vec2<f32>(-0.5, -0.5),
    vec2<f32>(0.5, 0.5),
    vec2<f32>(-0.5, 0.5),
    vec2<f32>(0.0, -0.5),
    vec2<f32>(0.5, 0.0),
    vec2<f32>(0.0, 0.5),
    vec2<f32>(-0.5, 0.0),
    vec2<f32>(0.0, -0.5),
    vec2<f32>(0.5, -0.11803399),
    vec2<f32>(0.30901699, 0.5),
    vec2<f32>(-0.30901699, 0.5),
    vec2<f32>(-0.5, -0.11803399),
    vec2<f32>(0.0, -0.5),
    vec2<f32>(0.5, -0.25),
    vec2<f32>(0.5, 0.25),
    vec2<f32>(0.0, 0.5),
    vec2<f32>(-0.5, 0.25),
    vec2<f32>(-0.5, -0.25),
    vec2<f32>(0.20710678, -0.5),
    vec2<f32>(0.5, -0.20710678),
    vec2<f32>(0.5, 0.20710678),
    vec2<f32>(0.20710678, 0.5),
    vec2<f32>(-0.20710678, 0.5),
    vec2<f32>(-0.5, 0.20710678),
    vec2<f32>(-0.5, -0.20710678),
    vec2<f32>(-0.20710678, -0.5),
);

fn shape_range(shape: u32) -> vec2<u32> {
    switch shape {
        case 0u: {
            return vec2<u32>(0u, 4u);
        }
        case 2u: {
            return vec2<u32>(4u, 3u);
        }
        case 3u: {
            return vec2<u32>(7u, 3u);
        }
        case 4u: {
            return vec2<u32>(10u, 4u);
        }
        case 5u: {
            return vec2<u32>(14u, 5u);
        }
        case 6u: {
            return vec2<u32>(19u, 6u);
        }
        case 7u: {
            return vec2<u32>(25u, 8u);
        }
        default: {
            return vec2<u32>(0u, 0u);
        }
    }
}

fn shape_point(shape: u32, index: u32, size: vec2<f32>) -> vec2<f32> {
    return SHAPE_POINTS[shape_range(shape).x + index] * size;
}

fn rotate2d(value: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>(value.x * c - value.y * s, value.x * s + value.y * c);
}

fn world_position(fragment: vec4<f32>) -> vec2<f32> {
    return fragment.xy / frame.scale;
}

fn object_local(object: Object, point: vec2<f32>) -> vec2<f32> {
    return rotate2d(point - object.position, -object.rotation) + (object.anchor - vec2<f32>(0.5)) * object.size;
}

fn object_world(object: Object, local: vec2<f32>) -> vec2<f32> {
    return object.position + rotate2d(local - (object.anchor - vec2<f32>(0.5)) * object.size, object.rotation);
}

fn shape_distance(shape: u32, size: vec2<f32>, point: vec2<f32>) -> f32 {
    if shape == SHAPE_CIRCLE {
        let radii = max(abs(size) * 0.5, vec2<f32>(1e-6));
        let k0 = length(point / radii);
        let k1 = length(point / (radii * radii));
        return k0 * (k0 - 1.0) / max(k1, 1e-6);
    }
    let range = shape_range(shape);
    if shape == NO_SHAPE || range.y == 0u {
        return 1e30;
    }
    var nearest = 1e30;
    var sign = 1.0;
    var previous = SHAPE_POINTS[range.x + range.y - 1u] * size;
    for (var index = 0u; index < range.y; index++) {
        let current = SHAPE_POINTS[range.x + index] * size;
        let edge = previous - current;
        let offset = point - current;
        let t = clamp(dot(offset, edge) / max(dot(edge, edge), 1e-12), 0.0, 1.0);
        let gap = offset - edge * t;
        nearest = min(nearest, dot(gap, gap));
        let crossing = vec3<bool>((point.y >= current.y), (point.y < previous.y), (edge.x * offset.y > edge.y * offset.x));
        if all(crossing) || all(!crossing) {
            sign = -sign;
        }
        previous = current;
    }
    return sign * sqrt(nearest);
}

fn object_exists(index: u32) -> bool {
    return index < arrayLength(&objects) && objects[index].kind != KIND_NONE;
}

fn object_distance(index: u32, point: vec2<f32>) -> f32 {
    if !object_exists(index) {
        return 1e30;
    }
    let object = objects[index];
    return shape_distance(object.shape, object.size, object_local(object, point));
}

fn object_contains(index: u32, point: vec2<f32>) -> bool {
    return object_distance(index, point) <= 0.0;
}
