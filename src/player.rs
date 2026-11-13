use std::marker::PhantomData;

#[cfg(feature = "chapter02")]
use better_default::Default;
use bevy::{
    prelude::*,
    input::mouse::AccumulatedMouseMotion,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
#[cfg(feature = "chapter02")]
use bevy_inspector_egui::{
    prelude::*,
    bevy_egui::input::EguiWantsInput,
    InspectorOptions,
};
#[cfg(all(feature = "chapter02", not(feature = "chapter03")))]
use bevy_inspector_egui::quick::ResourceInspectorPlugin;

#[cfg(not(feature = "chapter02"))]
const MOVE_SPEED: f32 = 5.0;
#[cfg(not(feature = "chapter02"))]
const MOUSE_SENSITIVITY: f32 = 0.0025;

pub struct PlayerPlugin<F, M> {
    is_focused: F,
    _marker: PhantomData<fn() -> M>,
}

#[cfg(feature = "chapter02")]
#[derive(Default, Resource, Reflect, InspectorOptions)]
#[reflect(Resource, InspectorOptions)]
struct CameraSettings {
    #[default(5.0)]
    #[inspector(min = 0.1, max = 20.0)]
    move_speed: f32,
    #[default(0.0025)]
    #[inspector(min = 0.0001, max = 0.01)]
    mouse_sensitivity: f32,
}

impl<F, M> PlayerPlugin<F, M>
where
    F: SystemCondition<M>,
{
    pub fn new(is_focused: F) -> Self {
        Self {
            is_focused,
            _marker: PhantomData,
        }
    }
}

impl<F, M> Plugin for PlayerPlugin<F, M>
where
    F: Clone + SystemCondition<M> + Send + Sync + 'static,
    M: 'static
{
    fn build(&self, app: &mut App) {
        #[cfg(feature = "chapter02")]
        app.init_resource::<CameraSettings>().register_type::<CameraSettings>();
        #[cfg(all(feature = "chapter02", not(feature = "chapter03")))]
        app.add_plugins(ResourceInspectorPlugin::<CameraSettings>::default());

        app.add_systems(Startup, player.spawn())
            .add_systems(Update, (
                toggle_cursor,
                (
                    player_look,
                    player_move,
                )
                .run_if(self.is_focused.clone()),
            ));
    }
}

fn player() -> impl Scene {
    let position = {
        #[cfg(not(feature = "chapter04"))] { vec3(1.5, 2.0, 4.0) }
        #[cfg(feature = "chapter04")] { vec3(5.0, 6.0, 11.0) }
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

fn player_look(
    mouse: Res<AccumulatedMouseMotion>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
    #[cfg(feature = "chapter02")] settings: Res<CameraSettings>,
) {
    let delta = mouse.delta;

    let sensitivity = {
        #[cfg(not(feature = "chapter02"))] { MOUSE_SENSITIVITY }
        #[cfg(feature = "chapter02")] { settings.mouse_sensitivity }
    };

    if delta != Vec2::ZERO {
        camera.rotate_y(-delta.x * sensitivity);
        camera.rotate_local_x(-delta.y * sensitivity);
    }
}

fn player_move(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
    #[cfg(feature = "chapter02")] settings: Res<CameraSettings>,
) {
    let delta = vec3(
        f32::from(keyboard.pressed(KeyCode::KeyD)) - f32::from(keyboard.pressed(KeyCode::KeyA)),
        f32::from(keyboard.pressed(KeyCode::Space)) - f32::from(keyboard.pressed(KeyCode::ShiftLeft)),
        f32::from(keyboard.pressed(KeyCode::KeyW)) - f32::from(keyboard.pressed(KeyCode::KeyS)),
    );

    let speed = {
        #[cfg(not(feature = "chapter02"))] { MOVE_SPEED }
        #[cfg(feature = "chapter02")] { settings.move_speed }
    };

    if delta != Vec3::ZERO {
        let direction = delta.x * camera.right() + delta.y * Vec3::Y + delta.z * camera.forward();
        camera.translation += speed * time.delta_secs() * direction.normalize();
    }
}

#[cfg(not(feature = "chapter02"))]
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

#[cfg(feature = "chapter02")]
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
