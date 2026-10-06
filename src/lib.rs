use bevy::asset::{AssetServer, Handle, embedded_asset, load_embedded_asset};
use bevy::camera::{MainPassResolutionOverride, Viewport};
use bevy::core_pipeline::core_3d::main_opaque_pass_3d;
use bevy::core_pipeline::schedule::Core3d;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{sampler, texture_2d};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::sync_world::RenderEntity;
use bevy::render::texture::GpuImage;
use bevy::render::view::{ExtractedView, Msaa, ViewTarget};
use bevy::render::{
    Extract, ExtractSchedule, GpuResourceAppExt, Render, RenderApp, RenderStartup, RenderSystems,
};

/// The render layer the sky scene lives on. Derived from the `DynamicSkyPlugin`
/// const generic so consumers and the crate agree on the same value.
#[derive(Resource)]
pub struct SkyLayer(pub usize);

mod dome_material;
pub use dome_material::{SkyDomeMaterial, SkyDomeUniforms};

/// System set for the dynamic sky's `PostUpdate` bookkeeping (e.g. syncing the
/// [`SkyCamera`] to the main camera). Exposed so the app composer can order
/// other camera/transform work relative to it without the crates knowing about
/// each other, by anchoring to `TransformSystems::Propagate`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DynamicSkySystems;

pub struct DynamicSkyPlugin<const SKY_LAYER: usize = 1>;

impl<const SKY_LAYER: usize> Default for DynamicSkyPlugin<SKY_LAYER> {
    fn default() -> Self {
        Self
    }
}

impl<const SKY_LAYER: usize> Plugin for DynamicSkyPlugin<SKY_LAYER> {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "sky_composite.wgsl");

        app.add_plugins(bevy::pbr::MaterialPlugin::<SkyDomeMaterial>::default());

        app.insert_resource(SkyLayer(SKY_LAYER));

        app.add_systems(Update, rotate_objects);
        app.add_systems(PostUpdate, sync_sky_camera.in_set(DynamicSkySystems));
        app.configure_sets(
            PostUpdate,
            DynamicSkySystems.before(bevy::transform::TransformSystems::Propagate),
        );

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_gpu_resource::<SpecializedRenderPipelines<SkyPipeline>>()
            .add_systems(RenderStartup, init_sky_pipeline)
            .add_systems(ExtractSchedule, extract_sky)
            .add_systems(
                Render,
                (
                    prepare_sky_pipelines.in_set(RenderSystems::Prepare),
                    prepare_sky_bind_groups
                        .in_set(RenderSystems::PrepareBindGroups)
                        .after(prepare_sky_pipelines),
                ),
            )
            .add_systems(Core3d, draw_sky_composite.before(main_opaque_pass_3d));
    }
}

/// Marks the main camera that should have the sky composited behind it.
///
/// `sky_target` is the offscreen render target that the [`SkyCamera`] writes to
/// and that this camera reads from (the composite pass blits it as the background).
#[derive(Component, Clone)]
#[require(Camera3d)]
pub struct DynamicSky {
    pub sky_target: Handle<Image>,
}

/// Marker for the second camera that renders the sky scene (sky sphere +
/// celestial meshes) on [`SkyLayer`] into the offscreen `DynamicSky::sky_target`.
///
/// Spawning this camera (and the sky scene it renders) is the consumer's
/// responsibility, since the exact setup differs per project.
#[derive(Component, Default, Clone)]
pub struct SkyCamera;

/// Marks the camera the [`SkyCamera`] should be synced to (its rotation drives
/// the sky dome's orientation). This is decoupled from [`DynamicSky`] so the same
/// [`sync_sky_camera`] system works whether or not the main camera also
/// composites a sky target — consumers just add this to their primary camera.
#[derive(Component, Default)]
pub struct MainCamera;

/// Spins the entity's transform around a fixed axis each frame.
///
/// `axis` is the (normalized) rotation axis in the entity's local space and
/// `angular_velocity` is in radians per second.
#[derive(Component, Clone)]
#[require(Transform)]
pub struct Rotator {
    pub axis: Dir3,
    pub angular_velocity: f32,
}

impl Rotator {
    pub fn new(axis: Dir3, angular_velocity: f32) -> Self {
        Self {
            axis,
            angular_velocity,
        }
    }
}

impl Default for Rotator {
    fn default() -> Self {
        Self {
            axis: Dir3::Y,
            angular_velocity: 0.0,
        }
    }
}

/// Advances every [`Rotator`] by `angular_velocity * delta` radians about its `axis`.
pub fn rotate_objects(time: Res<Time>, mut query: Query<(&Rotator, &mut Transform)>) {
    let delta = time.delta_secs();
    for (rotator, mut transform) in &mut query {
        transform.rotate_axis(rotator.axis, rotator.angular_velocity * delta);
    }
}

/// Aligns each [`SkyCamera`]'s rotation with the main camera (the one marked
/// with [`MainCamera`]) so the sky dome tracks the viewer's look direction.
/// Runs in `PostUpdate` so any main-camera transform changes applied during
/// `Update` are already in place before the sync.
pub fn sync_sky_camera(
    main_cameras: Query<&Transform, (With<MainCamera>, Without<SkyCamera>)>,
    mut sky_cameras: Query<&mut Transform, With<SkyCamera>>,
) {
    let Some(main_transform) = main_cameras.iter().next() else {
        return;
    };
    for mut sky_transform in &mut sky_cameras {
        sky_transform.rotation = main_transform.rotation;
    }
}

#[derive(Component, Clone)]
struct ExtractedSky {
    sky_target: Handle<Image>,
}

#[derive(Component)]
struct SkyPipelineId(CachedRenderPipelineId);

#[derive(Component)]
struct SkyBindGroup(BindGroup);

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
struct SkyPipelineKey {
    target_format: TextureFormat,
    samples: u32,
}

#[derive(Resource)]
struct SkyPipeline {
    layout: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
    sampler: Sampler,
}

impl SpecializedRenderPipeline for SkyPipeline {
    type Key = SkyPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            label: Some("dynamic_sky_pipeline".into()),
            layout: vec![self.layout.clone()],
            vertex: VertexState {
                shader: self.shader.clone(),
                entry_point: Some("sky_vertex".into()),
                ..default()
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                entry_point: Some("sky_fragment".into()),
                targets: vec![Some(ColorTargetState {
                    format: key.target_format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState {
                count: key.samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            ..default()
        }
    }
}

fn extract_sky(mut commands: Commands, main_cams: Extract<Query<(RenderEntity, &DynamicSky)>>) {
    for (render_entity, dynamic_sky) in &main_cams {
        commands.entity(render_entity).insert(ExtractedSky {
            sky_target: dynamic_sky.sky_target.clone(),
        });
    }
}

fn init_sky_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
) {
    let shader = load_embedded_asset!(asset_server.as_ref(), "sky_composite.wgsl");
    let layout = BindGroupLayoutDescriptor::new(
        "dynamic_sky_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
            ),
        ),
    );
    let sampler = render_device.create_sampler(&SamplerDescriptor {
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    commands.insert_resource(SkyPipeline {
        layout,
        shader,
        sampler,
    });
}

fn prepare_sky_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<SkyPipeline>>,
    pipeline: Res<SkyPipeline>,
    cameras: Query<(Entity, &ExtractedView, &Msaa), With<ExtractedSky>>,
) {
    for (entity, view, msaa) in &cameras {
        let pipeline_id = pipelines.specialize(
            &pipeline_cache,
            &pipeline,
            SkyPipelineKey {
                target_format: view.target_format,
                samples: msaa.samples(),
            },
        );
        commands.entity(entity).insert(SkyPipelineId(pipeline_id));
    }
}

fn prepare_sky_bind_groups(
    mut commands: Commands,
    pipeline: Res<SkyPipeline>,
    images: Res<RenderAssets<GpuImage>>,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    views: Query<(Entity, &ExtractedSky)>,
) {
    for (entity, extracted) in &views {
        let Some(image) = images.get(&extracted.sky_target) else {
            commands.entity(entity).remove::<SkyBindGroup>();
            continue;
        };

        let bind_group = render_device.create_bind_group(
            "dynamic_sky_bind_group",
            &pipeline_cache.get_bind_group_layout(&pipeline.layout),
            &BindGroupEntries::sequential((&image.texture_view, &pipeline.sampler)),
        );

        commands.entity(entity).insert(SkyBindGroup(bind_group));
    }
}

fn draw_sky_composite(
    view: ViewQuery<(
        &ExtractedCamera,
        &ViewTarget,
        &SkyPipelineId,
        &SkyBindGroup,
        Option<&MainPassResolutionOverride>,
    )>,
    pipeline_cache: Res<PipelineCache>,
    mut ctx: RenderContext,
) {
    let (camera, target, sky_pipeline_id, sky_bind_group, resolution_override) = view.into_inner();

    let Some(pipeline) = pipeline_cache.get_render_pipeline(sky_pipeline_id.0) else {
        return;
    };

    let command_encoder = ctx.command_encoder();

    let mut pass = command_encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("dynamic_sky_composite"),
        color_attachments: &[Some(target.get_color_attachment())],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });

    if let Some(viewport) =
        Viewport::from_viewport_and_override(camera.viewport.as_ref(), resolution_override)
    {
        pass.set_viewport(
            viewport.physical_position.x as f32,
            viewport.physical_position.y as f32,
            viewport.physical_size.x as f32,
            viewport.physical_size.y as f32,
            viewport.depth.start,
            viewport.depth.end,
        );
    }

    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &sky_bind_group.0, &[]);
    pass.draw(0..3, 0..1);
}
