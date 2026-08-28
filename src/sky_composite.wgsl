struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn sky_vertex(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle (clip space).
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    let p = positions[vertex_index];
    var out: VertexOutput;
    out.position = vec4<f32>(p, 0.0, 1.0);
    // Map clip space to texture UVs (Bevy textures are top-left origin).
    out.uv = vec2<f32>((p.x + 1.0) * 0.5, (1.0 - p.y) * 0.5);
    return out;
}

@group(0) @binding(0) var sky_texture: texture_2d<f32>;
@group(0) @binding(1) var sky_sampler: sampler;

@fragment
fn sky_fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(sky_texture, sky_sampler, in.uv);
}
