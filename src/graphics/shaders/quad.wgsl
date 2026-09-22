const INSTANCE_IMAGE: u32 = 1u;
const INSTANCE_GLYPH: u32 = 2u;
const FLAG_PIXELATED: u32 = 1u;

@vertex
fn vs_main(@builtin(vertex_index) vertex: u32, @builtin(instance_index) index: u32) -> VertexOutput {
    let item = instances[index];
    let corner = vec2<f32>(f32((0x32u >> vertex) & 1u), f32((0x2cu >> vertex) & 1u));
    let world = item.position + rotate2d((corner - item.anchor) * item.size, item.rotation);
    var out: VertexOutput;
    out.position = vec4<f32>(
        world.x / frame.resolution.x * 2.0 - 1.0,
        1.0 - world.y / frame.resolution.y * 2.0,
        0.0,
        1.0,
    );
    out.uv = mix(item.uv.xy, item.uv.zw, corner);
    out.color = item.color;
    out.local = (corner - vec2<f32>(0.5)) * item.size;
    out.size = item.size;
    out.instance = index;
    return out;
}

fn sample_image(item: Instance, uv: vec2<f32>) -> vec4<f32> {
    if (item.flags & FLAG_PIXELATED) != 0u {
        let dimensions = vec2<i32>(textureDimensions(image));
        let texel = clamp(vec2<i32>(floor(uv * vec2<f32>(dimensions))), vec2<i32>(0), dimensions - vec2<i32>(1));
        return textureLoad(image, texel, 0);
    }
    return textureSampleLevel(image, image_sampler, uv, 0.0);
}

fn shade_shape(item: Instance, local: vec2<f32>, pixel: f32) -> vec4<f32> {
    let distance = shape_distance(item.shape, item.size, local);
    let coverage = clamp(0.5 - distance / pixel, 0.0, 1.0);
    var color = item.color;
    if item.stroke > 0.0 {
        let inner = clamp(0.5 - (distance + item.stroke) / pixel, 0.0, 1.0);
        let fill = vec4<f32>(item.color.rgb * item.color.a, item.color.a);
        let stroke = vec4<f32>(item.stroke_color.rgb * item.stroke_color.a, item.stroke_color.a);
        let mixed = mix(stroke, fill, inner);
        color = vec4<f32>(mixed.rgb / max(mixed.a, 1e-6), mixed.a);
    }
    return vec4<f32>(color.rgb, color.a * coverage);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let pixel = max(length(fwidth(in.local)) * 0.70710678, 1e-4);
    let item = instances[in.instance];
    var color: vec4<f32>;
    switch item.kind {
        case INSTANCE_IMAGE: {
            color = sample_image(item, in.uv) * in.color;
        }
        case INSTANCE_GLYPH: {
            color = vec4<f32>(in.color.rgb, in.color.a * textureSampleLevel(image, image_sampler, in.uv, 0.0).r);
        }
        default: {
            color = shade_shape(item, in.local, pixel);
        }
    }
    return vec4<f32>(color.rgb * color.a, color.a);
}
