use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

/// Uniforms driving the dome's look. Kept as a single uniform block so the
/// shader only needs one binding for all tunables.
#[derive(ShaderType, Clone, Default, Debug)]
pub struct SkyDomeUniforms {
    /// Spatial frequency of the noise (higher = finer grain).
    pub noise_scale: f32,
    /// How strongly the noise modulates brightness (0 = none).
    pub noise_strength: f32,
    /// Overall multiplier on the fixed background (`base_color_texture`).
    pub background_brightness: f32,
    /// Multiplier on the noise-modulated layer (`noise_color_texture`),
    /// independent of the background dimmer above.
    pub noise_brightness: f32,
    /// Speed at which the noise field drifts (drives the twinkle).
    pub noise_speed: f32,
    /// Elapsed time in seconds, written each frame by the consumer (mirrors
    /// `bevy_pbr`'s `globals.time` without importing its embedded modules, which
    /// is unsupported for cross-crate WESL shaders on bevy 0.19).
    pub time: f32,
}

/// Custom unlit dome material. Samples `base_color_texture`, then fades by
/// `brightness` and modulates brightness with a cheap procedural 3D noise so
/// the effect is seamless on the sphere (and never touches the Moon, which
/// keeps its own `StandardMaterial`).
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct SkyDomeMaterial {
    /// Fixed background layer. Sampled directly and dimmed by `uniforms.brightness`;
    /// never modulated by noise.
    #[texture(1)]
    #[sampler(2)]
    pub background_texture: Option<Handle<Image>>,
    /// Layer that is actually modulated by the procedural noise field. Its
    /// contribution is scaled by `uniforms.brightness` (shared dimmer) and
    /// `uniforms.noise_strength` (noise modulation amount).
    #[texture(3)]
    #[sampler(4)]
    pub starfield_texture: Option<Handle<Image>>,
    #[uniform(0)]
    pub uniforms: SkyDomeUniforms,
}

impl Default for SkyDomeMaterial {
    fn default() -> Self {
        Self {
            background_texture: None,
            starfield_texture: None,
            uniforms: SkyDomeUniforms::default(),
        }
    }
}

impl Material for SkyDomeMaterial {
    fn fragment_shader() -> ShaderRef {
        "sky_dome.wesl".into()
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}
