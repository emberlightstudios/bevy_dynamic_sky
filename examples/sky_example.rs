use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::math::Dir3;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::window::PrimaryWindow;
use bevy_dynamic_sky::{
    DynamicSky, MainCamera, Rotator, SkyCamera, SkyDomeMaterial, SkyDomeUniforms, SkyLayer,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(bevy_dynamic_sky::DynamicSkyPlugin::<1>)
        .init_resource::<CameraController>()
        .add_systems(Startup, setup)
        .add_systems(Update, (camera_look, resize_sky_target))
        .run();
}

#[derive(Resource, Default)]
struct CameraController {
    yaw: f32,
    pitch: f32,
}

/// Tracks the current size of the offscreen sky target so we only recreate it
/// when the window actually resizes.
#[derive(Resource)]
struct SkyTargetSize(pub (u32, u32));

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    sky_layer: Res<SkyLayer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut dome_materials: ResMut<Assets<SkyDomeMaterial>>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window>,
) {
    let layer = sky_layer.0;

    // Offscreen render target the SkyCamera writes to (and the main camera reads).
    let window = windows.single().expect("expected exactly one window");
    let sky_size = (
        window.physical_width().max(1),
        window.physical_height().max(1),
    );
    let sky_target = images.add(Image::new_target_texture(
        sky_size.0,
        sky_size.1,
        TextureFormat::Rgba8Unorm,
        None,
    ));

    // Main (viewer) camera: composites the sky target and is the sync source.
    commands.spawn((
        Camera3d::default(),
        MainCamera,
        Camera {
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::layer(0),
        DynamicSky {
            sky_target: sky_target.clone(),
        },
        Transform::from_xyz(0.0, 1.6, 5.0),
    ));

    let sphere = meshes.add(Sphere::new(1.0).mesh().uv(16, 16));

    let milkyway = asset_server.load("Milkyway.jpg");
    let moon_texture = asset_server.load("MoonMap.png");

    // --- Solar system, all on the sky layer (rendered by the SkyCamera).
    // Sun (root) -> orbital Rotator -> offset -> axial tilt -> spin Rotator ->
    // SkyCamera (leaf). The camera orbits with the earth while its rotation is
    // still synced to the main camera via the crate's `sync_sky_camera`.
    // `Rotator` `#[require]`s `Transform`, so the `Orbit`/`Spin` pivots get a
    // transform automatically; nested tuples + `children!` build the hierarchy.
    commands.spawn((
        Name::new("Sun"),
        Mesh3d(sphere.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.85, 0.2).into(),
            emissive: Color::srgb(1.0, 0.8, 0.2).into(),
            unlit: true,
            ..default()
        })),
        RenderLayers::layer(layer),
        Transform::from_scale(Vec3::splat(20.0)),
        children![
            (
                Name::new("Dome"),
                // Earth's daily rotation: the starfield slowly turns about the
                // celestial (world Y) axis. `angular_velocity` is rad/s; this is
                // a sped-up sidereal day (~21 min/rev) so it's perceptible.
                Rotator::new(Dir3::Y, 0.005),
                Mesh3d(sphere.clone()),
                MeshMaterial3d(dome_materials.add(SkyDomeMaterial {
                    base_color_texture: Some(milkyway),
                    uniforms: SkyDomeUniforms {
                        brightness: 0.1,
                        noise_scale: 4.0,
                        noise_strength: 0.15,
                        noise_offset: Vec3::ZERO,
                    },
                })),
                RenderLayers::layer(layer),
                Transform::from_scale(Vec3::splat(-1000.0)),
            ),
            (
                Name::new("Orbit"),
                Rotator::new(Dir3::Y, 0.0005),
                children![(
                    Name::new("Offset"),
                    Transform::from_xyz(200.0, 0.0, 0.0),
                    children![(
                        Name::new("Tilt"),
                        Transform::from_rotation(Quat::from_rotation_z(0.41)),
                        children![(
                            Name::new("Spin"),
                            Rotator::new(Dir3::X, 0.02),
                            children![
                                // Moon: orbits the world (the camera's position,
                                // where the ground/floor sits). `MoonOrbit` only
                                // spins; the offset lives on the `Moon` child so
                                // the parent's rotation sweeps it around.
                                (
                                    Name::new("MoonOrbit"),
                                    Rotator::new(Dir3::Y, 0.005),
                                    children![(
                                        Name::new("Moon"),
                                        Mesh3d(sphere.clone()),
                                        MeshMaterial3d(materials.add(StandardMaterial {
                                            base_color_texture: Some(moon_texture),
                                            unlit: true,
                                            ..default()
                                        })),
                                        RenderLayers::layer(layer),
                                        Transform::from_xyz(30.0, 0.0, 0.0)
                                            .with_scale(Vec3::splat(4.0)),
                                    ),]
                                ),
                                // Observer: the camera's position on the world.
                                // The latitude rotation tilts the local frame,
                                // Longitude is set by the `Spin` rotation
                                (
                                    Name::new("Observer"),
                                    Transform::from_rotation(Quat::from_rotation_x(0.6)),
                                    children![(
                                        Name::new("SkyCamera"),
                                        SkyCamera,
                                        Camera3d::default(),
                                        Camera {
                                            order: -1,
                                            clear_color: ClearColorConfig::Custom(Color::BLACK),
                                            ..default()
                                        },
                                        RenderLayers::layer(layer),
                                        Transform::default(),
                                        RenderTarget::Image(sky_target.clone().into()),
                                    ),]
                                ),
                            ],
                        )],
                    )],
                )],
            )
        ],
    ));

    // Floor on layer 0 (so the SkyCamera doesn't render it).
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(40.0, 40.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.35, 0.38),
            ..default()
        })),
        Transform::default(),
        RenderLayers::layer(0),
    ));

    // Light on layer 0 so the floor (world) is lit; the sky is unlit anyway.
    commands.spawn((
        DirectionalLight {
            illuminance: 1000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::layer(0),
    ));

    commands.insert_resource(SkyTargetSize(sky_size));
}

fn camera_look(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut controller: ResMut<CameraController>,
    mut camera: Single<&mut Transform, With<MainCamera>>,
) {
    let speed = 1.5 * time.delta_secs();

    if keys.pressed(KeyCode::KeyA) {
        controller.yaw += speed;
    }
    if keys.pressed(KeyCode::KeyD) {
        controller.yaw -= speed;
    }
    if keys.pressed(KeyCode::KeyW) {
        controller.pitch += speed;
    }
    if keys.pressed(KeyCode::KeyS) {
        controller.pitch -= speed;
    }
    controller.pitch = controller.pitch.clamp(-1.54, 1.54);

    camera.rotation = Quat::from_euler(EulerRot::YXZ, controller.yaw, controller.pitch, 0.0);
}

/// Recreate the offscreen sky target at the new window size and point the
/// cameras at it. Polled each frame (Bevy 0.19 has no `EventReader`); cheap
/// because it only acts when the size actually changes.
fn resize_sky_target(
    mut images: ResMut<Assets<Image>>,
    mut sky_size: ResMut<SkyTargetSize>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut sky_cam_target: Query<&mut RenderTarget, With<SkyCamera>>,
    mut main_dynamic_sky: Query<&mut DynamicSky, Without<SkyCamera>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let new_size = (
        window.physical_width().max(1),
        window.physical_height().max(1),
    );
    if new_size == sky_size.0 {
        return;
    }

    let new_target = images.add(Image::new_target_texture(
        new_size.0,
        new_size.1,
        TextureFormat::Rgba8Unorm,
        None,
    ));

    if let Ok(mut target) = sky_cam_target.single_mut() {
        *target = RenderTarget::Image(new_target.clone().into());
    }
    if let Ok(mut dynamic_sky) = main_dynamic_sky.single_mut() {
        dynamic_sky.sky_target = new_target;
    }
    sky_size.0 = new_size;
}
