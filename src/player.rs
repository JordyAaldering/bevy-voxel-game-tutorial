#[cfg(since07)]
use crate::is_focused;
#[cfg(since07)]
use crate::{is_gamemode_spectator, is_gamemode_survival};
#[cfg(since08)]
use crate::{VoxelWorld, meshdata, raycast::{self, VoxelHit}, VoxelType};
#[cfg(since09)]
use crate::{TerrainConfig, CHUNK_HEIGHT};
#[cfg(since12)]
use crate::game_view_captured;

#[cfg(since02)]
use better_default::Default;
use bevy::{
    prelude::*,
    input::mouse::AccumulatedMouseMotion,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
#[cfg(since02)]
use bevy_inspector_egui::{prelude::*, InspectorOptions};
#[cfg(all(since02, until11))]
use bevy_inspector_egui::bevy_egui::input::EguiWantsInput;
#[cfg(only02)]
use bevy_inspector_egui::quick::ResourceInspectorPlugin;

#[cfg(only01)]
const MOVE_SPEED: f32 = 5.0;
#[cfg(only01)]
const MOUSE_SENSITIVITY: f32 = 0.0025;

#[cfg(since07)]
const MAX_PITCH: f32 = 1.4;
#[cfg(since07)]
const PLAYER_EYE_HEIGHT: f32 = 1.6;

#[cfg(since08)]
const INTERACTION_DISTANCE: f32 = 5.0;

#[derive(Default)]
pub struct PlayerPlugin;

#[cfg(since02)]
#[derive(Default, Resource, Reflect, InspectorOptions)]
#[reflect(Resource, InspectorOptions)]
struct CameraSettings {
    #[default(0.0025)]
    #[inspector(min = 0.0001, max = 0.01)]
    mouse_sensitivity: f32,
    #[default(5.0)]
    #[inspector(min = 0.1, max = 20.0)]
    move_speed: f32,
    #[cfg(since07)]
    #[default(7.0)]
    #[inspector(min = 0.1, max = 20.0)]
    jump_strenght: f32,
}

#[cfg(since07)]
#[derive(Clone, Default, Component)]
pub struct Player {
    pub velocity: Vec3,
    pub grounded: bool,
}

#[cfg(since07)]
#[derive(Clone, Copy, Default, Component)]
pub struct PlayerCamera;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(since02)]
        app.init_resource::<CameraSettings>().register_type::<CameraSettings>();

        #[cfg(only02)]
        app.add_plugins(ResourceInspectorPlugin::<CameraSettings>::default());

        #[cfg(until08)]
        app.add_systems(Startup, player.spawn());

        #[cfg(since08)]
        app.add_systems(Update, (
            draw_selected_voxel,
            break_place_block,
        ));

        #[cfg(since09)]
        app.add_systems(Startup, spawn_player);

        app.add_systems(Update, toggle_cursor);

        #[cfg(until11)]
        app.add_systems(Update, (
            (
                player_look,
                #[cfg(until06)]
                move_spectator,
                #[cfg(since07)]
                move_spectator.run_if(is_gamemode_spectator),
                #[cfg(since07)]
                move_survival.run_if(is_gamemode_survival),
            ).run_if(is_focused),
        ));

        #[cfg(since12)]
        app.add_systems(Update, (
            (
                player_look,
                #[cfg(since07)]
                move_spectator.run_if(is_gamemode_spectator),
                #[cfg(since07)]
                move_survival.run_if(is_gamemode_survival),
            ).run_if(is_focused.and_then(game_view_captured)),
        ));
    }
}

#[cfg(until01)]
fn toggle_cursor(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    // Release the cursor when escape is pressed
    if keyboard.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
    // Recapture when a click occurs
    if mouse.just_pressed(MouseButton::Left) {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

#[cfg(all(since02, until11))]
fn toggle_cursor(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    egui_wants_input: Res<EguiWantsInput>,
) {
    // Release the cursor when escape is pressed
    if keyboard.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
    // Recapture when a click occurs outside egui
    if mouse.just_pressed(MouseButton::Left)
        && !egui_wants_input.wants_pointer_input()
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

#[cfg(since12)]
fn toggle_cursor(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    window: Single<&Window, With<PrimaryWindow>>,
    editor_state: Res<crate::EditorState>,
) {
    // Release the cursor when escape is pressed, or when the game view is no longer the active tab
    if keyboard.just_pressed(KeyCode::Escape)
        || (
            cursor.grab_mode != CursorGrabMode::None
            && !(editor_state.is_game_view_active() && editor_state.is_game_view_visible())
        )
    {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }

    // Capture only when clicking inside the game view
    if mouse.just_pressed(MouseButton::Left)
        && editor_state.is_game_view_active()
        && window.cursor_position().is_some_and(|position| editor_state.game_view_contains(position))
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

#[cfg(until06)]
fn player() -> impl Scene {
    let position = {
        #[cfg(until03)] { vec3(1.5, 2.0, 4.0) }
        #[cfg(since04)] { vec3(5.0, 6.0, 11.0) }
    };

    bsn!{
        #Player
        Camera3d
        Transform::from_translation(position)
            .looking_at(vec3(0.5, 0.5, 0.5), Vec3::Y)
        Projection::Perspective(PerspectiveProjection {
            fov: 0.9,
        })
        Msaa::Off
    }
}

#[cfg(all(since07, until08))]
fn player() -> impl Scene {
    bsn! {
        #Player
        Player
        Transform::from_xyz(4.5, 5.0, 4.5)
        Projection::Perspective(PerspectiveProjection {
            fov: 0.9
        })
        Msaa::Off
        InheritedVisibility
        Children [
            PlayerCamera
            Camera3d
            Transform::from_xyz(0.0, PLAYER_EYE_HEIGHT, 0.0)
        ]
    }
}

#[cfg(since09)]
fn spawn_player(
    mut commands: Commands,
    config: Res<TerrainConfig>,
    #[cfg(since12)] editor_state: Res<crate::EditorState>,
) {
    let world_height = config.height.sample2(config.seed, 0.0, 0.0) as usize;
    let world_height = world_height.clamp(0, CHUNK_HEIGHT - 1);

    commands.spawn((
            Name::new("Player"),
            Player::default(),
            Projection::Perspective(PerspectiveProjection {
                fov: 0.9,
                ..default()
            }),
            Msaa::Off,
            InheritedVisibility::default(),
            Transform::from_xyz(0.5, world_height as f32 + 1.0, 0.5),
        ))
        .with_children(|commands| {
            commands.spawn((
                PlayerCamera,
                Camera3d::default(),
                Transform::from_xyz(0.0, PLAYER_EYE_HEIGHT, 0.0),
                #[cfg(since12)]
                editor_state.game_camera_target(),
            ));
        });
}

#[cfg(until06)]
fn player_look(
    mouse: Res<AccumulatedMouseMotion>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
    #[cfg(since02)] settings: Res<CameraSettings>,
) {
    let delta = mouse.delta;

    let sensitivity = {
        #[cfg(until01)] { MOUSE_SENSITIVITY }
        #[cfg(since02)] { settings.mouse_sensitivity }
    };

    if delta != Vec2::ZERO {
        camera.rotate_y(-delta.x * sensitivity);
        camera.rotate_local_x(-delta.y * sensitivity);
    }
}

#[cfg(since07)]
fn player_look(
    mouse: Res<AccumulatedMouseMotion>,
    mut camera: Single<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
    mut player: Single<&mut Transform, (With<Player>, Without<PlayerCamera>)>,
    settings: Res<CameraSettings>,
) {
    let delta = mouse.delta;

    if delta != Vec2::ZERO {
        // Horizontal movement rotates the entire player around the world Y axis
        player.rotate_y(-delta.x * settings.mouse_sensitivity);
        // Vertical movement rotates only the camera around its local X axis
        let (yaw, mut pitch, roll) = camera.rotation.to_euler(EulerRot::YXZ);

        pitch -= delta.y * settings.mouse_sensitivity;
        pitch = pitch.clamp(-MAX_PITCH, MAX_PITCH);

        camera.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, roll);
    }
}

#[cfg(until06)]
fn move_spectator(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
    #[cfg(since02)] settings: Res<CameraSettings>,
) {
    let delta = vec3(
        f32::from(keyboard.pressed(KeyCode::KeyD)) - f32::from(keyboard.pressed(KeyCode::KeyA)),
        f32::from(keyboard.pressed(KeyCode::Space)) - f32::from(keyboard.pressed(KeyCode::ShiftLeft)),
        f32::from(keyboard.pressed(KeyCode::KeyW)) - f32::from(keyboard.pressed(KeyCode::KeyS)),
    );

    let speed = {
        #[cfg(not(since02))] { MOVE_SPEED }
        #[cfg(since02)] { settings.move_speed }
    };

    if delta != Vec3::ZERO {
        let direction = delta.x * camera.right() + delta.y * Vec3::Y + delta.z * camera.forward();
        camera.translation += speed * time.delta_secs() * direction.normalize();
    }
}

#[cfg(since07)]
fn move_spectator(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    player: Single<(&mut Transform, &mut Player)>,
    settings: Res<CameraSettings>,
) {
    let (mut transform, mut player) = player.into_inner();

    player.velocity = Vec3::ZERO;
    player.grounded = false;

    let delta = vec3(
        f32::from(keyboard.pressed(KeyCode::KeyD)) - f32::from(keyboard.pressed(KeyCode::KeyA)),
        f32::from(keyboard.pressed(KeyCode::Space)) - f32::from(keyboard.pressed(KeyCode::ShiftLeft)),
        f32::from(keyboard.pressed(KeyCode::KeyW)) - f32::from(keyboard.pressed(KeyCode::KeyS)),
    );

    if delta != Vec3::ZERO {
        let direction = delta.x * transform.right() + delta.y * Vec3::Y + delta.z * transform.forward();
        transform.translation += settings.move_speed * time.delta_secs() * direction.normalize();
    }
}

#[cfg(since07)]
fn move_survival(
    keyboard: Res<ButtonInput<KeyCode>>,
    player: Single<(&mut Player, &Transform)>,
    settings: Res<CameraSettings>,
) {
    let (mut player, transform) = player.into_inner();

    let dx = f32::from(keyboard.pressed(KeyCode::KeyD)) - f32::from(keyboard.pressed(KeyCode::KeyA));
    let dz = f32::from(keyboard.pressed(KeyCode::KeyW)) - f32::from(keyboard.pressed(KeyCode::KeyS));

    if dx != 0.0 || dz != 0.0 {
        let movement = dx * transform.right() + dz * transform.forward();
        let movement = movement.normalize_or_zero();
        player.velocity.x = movement.x * settings.move_speed;
        player.velocity.z = movement.z * settings.move_speed;
    } else {
        player.velocity.x = 0.0;
        player.velocity.z = 0.0;
    }

    if player.grounded && keyboard.just_pressed(KeyCode::Space) {
        player.velocity.y = settings.jump_strenght;
        player.grounded = false;
    }
}

#[cfg(since08)]
fn draw_selected_voxel(
    mut gizmos: Gizmos,
    world: Res<VoxelWorld>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
) {
    let Some(VoxelHit { coord, direction }) = raycast::find_voxel(
        |pos| world.is_solid(pos),
        camera.translation(),
        camera.forward(),
        INTERACTION_DISTANCE,
    ) else {
        return;
    };

    let origin = coord.position();
    let vertices = meshdata::quad_vertices(direction, origin);

    gizmos.line(vertices[0], vertices[1], Color::WHITE);
    gizmos.line(vertices[1], vertices[3], Color::WHITE);
    gizmos.line(vertices[3], vertices[2], Color::WHITE);
    gizmos.line(vertices[2], vertices[0], Color::WHITE);
}

#[cfg(since08)]
fn break_place_block(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut world: ResMut<VoxelWorld>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
) {
    let do_break = mouse.just_pressed(MouseButton::Left);
    let do_place = mouse.just_pressed(MouseButton::Right);
    if !(do_break || do_place) {
        return;
    }

    let origin = camera.translation();
    let look = camera.forward();

    if let Some(VoxelHit { coord, direction }) = raycast::find_voxel(
        |pos| world.is_solid(pos),
        origin,
        look,
        INTERACTION_DISTANCE,
    ) {
        info!("Hit {:?} at {:?}", coord, direction);
        if do_break {
            world.set_voxel(&mut commands, coord, VoxelType::Air);
        } else /* if do_place */ {
            let neighbor = coord + direction;
            world.set_voxel(&mut commands, neighbor, VoxelType::Dirt);
        }
    }
}
