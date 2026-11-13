use super::{EditorCamera, EditorState};

use bevy::{
    prelude::*,
    camera::visibility::RenderLayers,
    camera_controller::pan_orbit_camera::controller::component::PanOrbitCamera,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    window::PrimaryWindow,
};
use bevy_inspector_egui::bevy_egui::{EguiGlobalSettings, PrimaryEguiContext};

pub fn spawn_editor_camera(
    mut commands: Commands,
    mut egui_settings: ResMut<EguiGlobalSettings>,
    editor_state: Res<EditorState>,
) {
    // Egui gets its own camera; the game cameras render to images shown inside the editor
    egui_settings.auto_create_primary_context = false;
    commands.spawn((
        PrimaryEguiContext,
        RenderLayers::none(),
        Camera2d,
        Camera {
            order: 1,
            ..default()
        },
    ));

    commands.spawn((
        EditorCamera,
        Camera3d::default(),
        Camera { is_active: false, ..default() },
        PanOrbitCamera::default(),
        editor_state.editor_camera_target(),
        Projection::from(PerspectiveProjection {
            fov: 70f32.to_radians(),
            ..default()
        }),
        Transform::from_xyz(1.5, 60.0, 40.0)
            .looking_at(vec3(1.5, 20.0, 1.5), Vec3::Y),
    ));
}

/// Right mouse button orbits, middle mouse button pans and the scroll wheel zooms, all
/// relative to the editor view. The cursor is never captured.
pub fn editor_camera_input(
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    editor: Res<EditorState>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut camera: Single<&mut PanOrbitCamera, With<EditorCamera>>,
) {
    let hovered = editor.is_editor_view_active()
        && window
            .cursor_position()
            .is_some_and(|pos| editor.editor_view_contains(pos));

    let zoom = if hovered {
        match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y * 150.0,
            MouseScrollUnit::Pixel => scroll.delta.y,
        }
    } else {
        0.0
    };

    let zoom_finished = camera.current_motion.is_zooming_only()
        && camera
            .current_motion
            .inputs()
            .is_some_and(|inputs| inputs.zoom_velocity_abs(camera.smoothing.zoom.mul_f32(2.0)) <= 0.0);
    if zoom_finished || mouse.any_just_released([MouseButton::Right, MouseButton::Middle]) {
        camera.end_move();
    }

    if hovered {
        if mouse.just_pressed(MouseButton::Right) {
            camera.start_orbit(None);
        } else if mouse.just_pressed(MouseButton::Middle) {
            camera.start_pan(None);
        } else if zoom != 0.0 && !camera.current_motion.is_user_controlled() {
            camera.start_zoom(None);
        }
    }

    let dragging = mouse.any_pressed([MouseButton::Right, MouseButton::Middle]);
    camera.send_screenspace_input(if dragging { motion.delta } else { Vec2::ZERO });
    camera.send_zoom_input(zoom);
}
