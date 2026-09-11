struct SkyDomeUniforms {
    noise_scale: f32,
    noise_strength: f32,
    background_brightness: f32,
    noise_brightness: f32,
    noise_speed: f32,
    time: f32,
};

@group(3) @binding(0) var<uniform> uniforms: SkyDomeUniforms;
@group(3) @binding(1) var background_texture: texture_2d<f32>;
@group(3) @binding(2) var background_sampler: sampler;
@group(3) @binding(3) var starfield_texture: texture_2d<f32>;
@group(3) @binding(4) var starfield_sampler: sampler;

// Mirrors the vertex output of bevy's default mesh vertex shader (see
// `bevy_pbr::render::forward_io::VertexOutput`) so this fragment entry point is
// interface-compatible without importing `forward_io`. On bevy 0.19 a
// file-loaded (or even embedded) shader in this crate CANNOT import bevy_pbr's
// embedded modules (cross-crate WESL imports are unsupported until 0.20), so the
// input struct is duplicated here and kept in sync with that layout. `time` is
// fed from the CPU via `uniforms.time` (see `animate_dome`).
struct SkyDomeFragmentInput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

// Cheap 2D hash -> [0, 1).
fn hash2(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453);
}

// Single-octave 2D value noise in [0, 1].
fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i + vec2<f32>(0.0, 0.0));
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

@fragment
fn fragment(in: SkyDomeFragmentInput) -> @location(0) vec4<f32> {
    // Cheap 2D noise sampled in UV space, drifted over time via `uniforms.time`.
    let n_uv = in.uv * uniforms.noise_scale + vec2<f32>(uniforms.time * uniforms.noise_speed);
    let noise = noise2(n_uv);

    // Stars: starfield sample mixed (multiplied) with the noise.
    let star_sample = textureSample(starfield_texture, starfield_sampler, in.uv);
    let stars = star_sample * noise;

    // Background: dimmable, never noise-modulated.
    let bg = textureSample(background_texture, background_sampler, in.uv)
        * uniforms.background_brightness;

    return mix(stars, bg, stars.r);
}
