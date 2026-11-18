#[cfg(since02)]
use super::CameraSettings;
#[cfg(since07)]
use super::{Player, PlayerCamera};

use bevy::{
    prelude::*,
    input::mouse::AccumulatedMouseMotion,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

#[cfg(only01)]
const MOVE_SPEED: f32 = 5.0;
#[cfg(only01)]
const MOUSE_SENSITIVITY: f32 = 0.0025;

#[cfg(since07)]
const MAX_PITCH: f32 = 1.4;

#[cfg(since08)]
const INTERACTION_DISTANCE: f32 = 5.0;

pub(super) fn register(app: &mut App) {
    app.add_systems(PreUpdate, toggle_cursor);

    #[cfg(since08)]
    app.add_systems(Update, (
        draw_selected_voxel,
    ));

    #[cfg(until06)]
    app.add_systems(Update, (
        (
            player_look,
            move_spectator,
        ).run_if(crate::is_focused),
    ));

    #[cfg(all(since07, until11))]
    app.add_systems(Update, (
        (
            player_look,
            #[cfg(since08)] break_place_block,
            move_spectator.run_if(crate::is_gamemode_spectator),
            move_survival.run_if(crate::is_gamemode_survival),
        ).run_if(crate::is_focused),
    ));

    #[cfg(since12)]
    app.add_systems(Update, (
        (
            player_look,
            break_place_block,
            move_spectator.run_if(crate::is_gamemode_spectator),
            move_survival.run_if(crate::is_gamemode_survival),
        ).run_if(crate::is_focused.and_then(crate::game_view_captured)),
    ));
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
    egui_wants_input: Res<bevy_inspector_egui::bevy_egui::input::EguiWantsInput>,
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

#[cfg(all(since12, until19))]
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

#[cfg(since20)]
fn toggle_cursor(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    editor_state: Res<crate::EditorState>,
    window: Single<&Window, With<PrimaryWindow>>,
    inventory: Res<crate::CreativeInventoryState>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
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
        && !inventory.open
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
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
    world: Res<crate::VoxelWorld>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
) {
    let Some(crate::VoxelHit { coord, direction }) = crate::raycast::find_voxel(
        |pos| world.is_solid(pos),
        camera.translation(),
        camera.forward(),
        INTERACTION_DISTANCE,
    ) else {
        return;
    };

    let origin = coord.position();
    let vertices = crate::meshdata::quad_vertices(direction, origin);

    gizmos.line(vertices[0], vertices[1], Color::WHITE);
    gizmos.line(vertices[1], vertices[3], Color::WHITE);
    gizmos.line(vertices[3], vertices[2], Color::WHITE);
    gizmos.line(vertices[2], vertices[0], Color::WHITE);
}

#[cfg(all(since08, until19))]
fn break_place_block(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut world: ResMut<crate::VoxelWorld>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
) {
    let do_break = mouse.just_pressed(MouseButton::Left);
    let do_place = mouse.just_pressed(MouseButton::Right);
    if !(do_break || do_place) {
        return;
    }

    let origin = camera.translation();
    let look = camera.forward();

    if let Some(crate::VoxelHit { coord, direction }) = crate::raycast::find_voxel(
        |pos| world.is_solid(pos),
        origin,
        look,
        INTERACTION_DISTANCE,
    ) {
        info!("Hit {:?} at {:?}", coord, direction);
        if do_break {
            #[cfg(until18)]
            let voxel = crate::VoxelType::Air;
            #[cfg(since19)]
            let voxel = crate::VoxelType::AIR;
            world.set_voxel(&mut commands, coord, voxel);
        } else /* if do_place */ {
            #[cfg(until18)]
            let voxel = crate::VoxelType::Dirt;
            #[cfg(since19)]
            let voxel = crate::VoxelType::from_terrain(crate::TerrainType::Dirt);
            let neighbor = coord + direction;
            if world.get_voxel(neighbor).is_some() {
                world.set_voxel(&mut commands, neighbor, voxel);
            }
        }
    }
}

#[cfg(since20)]
fn break_place_block(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut world: ResMut<crate::VoxelWorld>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    hotbar: Res<crate::Hotbar>,
) {
    let do_break = mouse.just_pressed(MouseButton::Left);
    let do_place = mouse.just_pressed(MouseButton::Right);
    if !(do_break || do_place) {
        return;
    }

    let origin = camera.translation();
    let look = camera.forward();

    if let Some(crate::VoxelHit { coord, direction }) = crate::raycast::find_voxel(
        |pos| world.is_solid(pos),
        origin,
        look,
        INTERACTION_DISTANCE,
    ) {
        info!("Hit {:?} at {:?}", coord, direction);
        if do_break {
            world.set_voxel(&mut commands, coord, crate::VoxelType::AIR);
        } else /* if do_place */ {
            let neighbor = coord + direction;
            if world.get_voxel(neighbor).is_some() {
                let voxel_type: crate::VoxelType = hotbar.selected().into();
                if voxel_type == crate::VoxelType::AIR {
                    // Can't place air, so don't do anything
                    return;
                }

                world.set_voxel(&mut commands, neighbor, voxel_type);
            }
        }
    }
}
