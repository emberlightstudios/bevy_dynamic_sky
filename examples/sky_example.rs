use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::math::Dir3;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::time::{Timer, TimerMode};
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
        .add_systems(
            Update,
            (
                camera_look,
                resize_sky_target,
                spawn_sky_scene,
                animate_dome,
            ),
        )
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

/// Handle to the dome material so we can advance its `uniforms.time` each frame.
#[derive(Resource)]
struct DomeMaterial(Handle<SkyDomeMaterial>);

/// Handle to the offscreen sky render target.
#[derive(Resource)]
struct SkyTargetImage(Handle<Image>);

/// Counts down before the sky scene (dome + celestial bodies + floor/light) is
/// spawned. We spawn only the main camera at startup and load the rest a few
/// seconds later. (On bevy 0.19 this is purely a load-order preference — the
/// self-contained dome shader needs no deferral, but the structure was requested
/// and works fine.)
#[derive(Resource)]
struct SkySpawnTimer(Timer);

/// Ensures the deferred sky scene is spawned exactly once.
#[derive(Resource, Default)]
struct SkySpawned(bool);

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut dome_materials: ResMut<Assets<SkyDomeMaterial>>,
    mut images: ResMut<Assets<Image>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
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

    // Build the dome material up front so we can keep a handle for the deferred
    // spawn and animate its `uniforms.time` each frame.
    let dome_material = dome_materials.add(SkyDomeMaterial {
        background_texture: Some(asset_server.load("Milkyway.jpg")),
        starfield_texture: Some(asset_server.load("StarField.jpg")),
        uniforms: SkyDomeUniforms {
            background_brightness: 1.,
            noise_brightness: 1.0,
            noise_scale: 1.0,
            noise_strength: 1.0,
            noise_speed: 0.0025,
            time: 0.0,
        },
    });

    commands.insert_resource(DomeMaterial(dome_material));
    commands.insert_resource(SkyTargetImage(sky_target.clone()));
    commands.insert_resource(SkyTargetSize(sky_size));
    commands.insert_resource(SkySpawnTimer(Timer::from_seconds(3.0, TimerMode::Once)));
    commands.insert_resource(SkySpawned(false));

    // Main (viewer) camera ONLY at startup. It composites the sky target and is
    // the sync source for the (not-yet-spawned) SkyCamera.
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

    // The sphere mesh is created here only so the asset is warmed; the deferred
    // spawn rebuilds its own handle (cheap, idempotent).
    let _sphere = meshes.add(Sphere::new(1.0).mesh().uv(16, 16));
}

/// Spawns the full sky scene (dome + solar system + floor/light) once the startup
/// timer elapses. Everything here is deferred so the heavy scene loads after the
/// viewer camera is up.
fn spawn_sky_scene(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<SkySpawnTimer>,
    mut spawned: ResMut<SkySpawned>,
    dome: Res<DomeMaterial>,
    sky_target: Res<SkyTargetImage>,
    sky_layer: Res<SkyLayer>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if spawned.0 {
        return;
    }
    timer.0.tick(time.delta());
    if !(timer.0.elapsed() >= timer.0.duration()) {
        return;
    }
    spawned.0 = true;

    let layer = sky_layer.0;
    let sphere = meshes.add(Sphere::new(1.0).mesh().uv(16, 16));
    let moon_texture = asset_server.load("MoonMap.png");
    let dome_material = dome.0.clone();

    // --- Solar system, all on the sky layer (rendered by the SkyCamera).
    // Sun (root) -> orbital Rotator -> offset -> axial tilt -> spin Rotator ->
    // SkyCamera (leaf). The camera orbits with the earth while its rotation is
    // still synced to the main camera via the crate's `sync_sky_camera`. `Rotator`
    // `#[require]`s `Transform`, so the `Orbit`/`Spin` pivots get a transform
    // automatically; nested tuples + `children!` build the hierarchy.
    commands.spawn((
        Name::new("Sun"),
        Transform::IDENTITY,
        children![
            (
                Name::new("Dome"),
                // Earth's daily rotation: the starfield slowly turns about the
                // celestial (world Y) axis. `angular_velocity` is rad/s; this is
                // a sped-up sidereal day (~21 min/rev) so it's perceptible.
                Rotator::new(Dir3::Y, 0.005),
                Mesh3d(sphere.clone()),
                MeshMaterial3d(dome_material.clone()),
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
                                    ),],
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
                                        RenderTarget::Image(sky_target.0.clone().into()),
                                    ),],
                                ),
                            ],
                        ),],
                    ),],
                ),],
            ),
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
}

/// Advances the dome material's `uniforms.time` each frame. This mirrors
/// `bevy_pbr`'s `globals.time` without importing its embedded shaders — on bevy
/// 0.19 a shader in this crate cannot resolve `bevy_pbr` module imports
/// (cross-crate WESL imports are unsupported until 0.20). Works on every bevy
/// version, so it's the "final" form.
fn animate_dome(
    time: Res<Time>,
    dome: Res<DomeMaterial>,
    mut dome_materials: ResMut<Assets<SkyDomeMaterial>>,
) {
    if let Some(mut material) = dome_materials.get_mut(&dome.0) {
        material.uniforms.time = time.elapsed_secs();
    }
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
