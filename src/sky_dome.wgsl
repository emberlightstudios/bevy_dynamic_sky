struct SkyDomeUniforms {
    noise_scale: f32,
    noise_strength: f32,
    brightness: f32,
    noise_offset: vec3<f32>,
};

@group(3) @binding(0) var<uniform> uniforms: SkyDomeUniforms;
@group(3) @binding(1) var base_color_texture: texture_2d<f32>;
@group(3) @binding(2) var base_color_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

// Cheap hash -> [0, 1).
fn hash3(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.3183099 + vec3<f32>(0.1, 0.2, 0.3));
    q = q + dot(q, q.yzx + 19.19);
    return fract((q.x + q.y) * q.z);
}

// 3D value noise in [0, 1].
fn value_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let n000 = hash3(i + vec3<f32>(0.0, 0.0, 0.0));
    let n100 = hash3(i + vec3<f32>(1.0, 0.0, 0.0));
    let n010 = hash3(i + vec3<f32>(0.0, 1.0, 0.0));
    let n110 = hash3(i + vec3<f32>(1.0, 1.0, 0.0));
    let n001 = hash3(i + vec3<f32>(0.0, 0.0, 1.0));
    let n101 = hash3(i + vec3<f32>(1.0, 0.0, 1.0));
    let n011 = hash3(i + vec3<f32>(0.0, 1.0, 1.0));
    let n111 = hash3(i + vec3<f32>(1.0, 1.0, 1.0));
    let x00 = mix(n000, n100, u.x);
    let x10 = mix(n010, n110, u.x);
    let x01 = mix(n001, n101, u.x);
    let x11 = mix(n011, n111, u.x);
    let y0 = mix(x00, x10, u.y);
    let y1 = mix(x01, x11, u.y);
    return mix(y0, y1, u.z);
}

// Two-octave fbm — light but a little richer than a single sample.
fn fbm(p: vec3<f32>) -> f32 {
    var v = 0.0;
    var amp = 0.5;
    var q = p;
    for (var o = 0; o < 2; o = o + 1) {
        v = v + amp * value_noise(q);
        q = q * 2.0;
        amp = amp * 0.5;
    }
    return v;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let base = textureSample(base_color_texture, base_color_sampler, in.uv);
    // Use the surface normal as a seamless 3D coordinate (independent of world
    // translation); good enough for "doesn't have to be fancy" noise.
    let dir = normalize(in.world_normal);
    let noise = fbm(dir * uniforms.noise_scale + uniforms.noise_offset);
    let shade = uniforms.brightness * mix(1.0, noise, uniforms.noise_strength);
    return vec4<f32>(base.rgb * shade, base.a);
}
