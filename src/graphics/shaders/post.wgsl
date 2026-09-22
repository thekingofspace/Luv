@vertex
fn vs_post(@builtin(vertex_index) vertex: u32) -> VertexOutput {
    let corner = vec2<f32>(f32((vertex << 1u) & 2u), f32(vertex & 2u));
    var out: VertexOutput;
    out.position = vec4<f32>(corner.x * 2.0 - 1.0, 1.0 - corner.y * 2.0, 0.0, 1.0);
    out.uv = corner;
    out.color = vec4<f32>(1.0);
    out.local = corner * frame.resolution;
    out.size = frame.resolution;
    out.instance = 0u;
    return out;
}
