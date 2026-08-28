use bevy::pbr::Material;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;

/// Uniforms driving the dome's look. Kept as a single uniform block so the
/// shader only needs one binding for all tunables.
#[derive(ShaderType, Clone, Default)]
pub struct SkyDomeUniforms {
    /// Spatial frequency of the noise (higher = finer grain).
    pub noise_scale: f32,
    /// How strongly the noise modulates brightness (0 = none).
    pub noise_strength: f32,
    /// Overall multiplier on the base color (0 = black, 1 = unchanged).
    pub brightness: f32,
    /// Offset into the noise field; shift over time to animate.
    pub noise_offset: Vec3,
}

/// Custom unlit dome material. Samples `base_color_texture`, then fades by
/// `brightness` and modulates brightness with a cheap procedural 3D noise so
/// the effect is seamless on the sphere (and never touches the Moon, which
/// keeps its own `StandardMaterial`).
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SkyDomeMaterial {
    #[texture(1)]
    #[sampler(2)]
    pub base_color_texture: Option<Handle<Image>>,
    #[uniform(0)]
    pub uniforms: SkyDomeUniforms,
}

impl Default for SkyDomeMaterial {
    fn default() -> Self {
        Self {
            base_color_texture: None,
            uniforms: SkyDomeUniforms::default(),
        }
    }
}

impl Material for SkyDomeMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_dynamic_sky/sky_dome.wgsl".into()
    }
}
