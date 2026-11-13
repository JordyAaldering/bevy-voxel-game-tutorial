use crate::is_focused;
#[cfg(since07)]
use crate::{is_gamemode_spectator, is_gamemode_survival};

#[cfg(since02)]
use better_default::Default;
use bevy::{
    prelude::*,
    input::mouse::AccumulatedMouseMotion,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
#[cfg(since02)]
use bevy_inspector_egui::{
    prelude::*,
    bevy_egui::input::EguiWantsInput,
    InspectorOptions,
};
#[cfg(only02)]
use bevy_inspector_egui::quick::ResourceInspectorPlugin;

#[cfg(only01)]
const MOVE_SPEED: f32 = 5.0;
#[cfg(only01)]
const MOUSE_SENSITIVITY: f32 = 0.0025;

#[cfg(since07)]
const PLAYER_EYE_HEIGHT: f32 = 1.6;
#[cfg(since07)]
const MAX_PITCH: f32 = 1.4;

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
struct PlayerCamera;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(since02)]
        app.init_resource::<CameraSettings>().register_type::<CameraSettings>();
        #[cfg(only02)]
        app.add_plugins(ResourceInspectorPlugin::<CameraSettings>::default());

        app.add_systems(Startup, player.spawn())
            .add_systems(Update, (
                toggle_cursor,
                (
                    player_look,
                    #[cfg(until06)]
                    move_spectator,
                    #[cfg(since07)]
                    move_spectator.run_if(is_gamemode_spectator),
                    #[cfg(since07)]
                    move_survival.run_if(is_gamemode_survival),
                )
                .run_if(is_focused),
            ));
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

#[cfg(since07)]
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

#[cfg(not(since02))]
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

#[cfg(since02)]
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
