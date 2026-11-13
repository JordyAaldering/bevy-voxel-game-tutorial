mod debug_gizmos;
mod player;
mod screenshot;

use crate::{
    debug_gizmos::DebugGizmosPlugin,
    player::PlayerPlugin,
    screenshot::ScreenshotPlugin,
};

use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_inspector_egui::{
    bevy_egui::prelude::*,
    quick::WorldInspectorPlugin,
};

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            EguiPlugin::default(),
            WorldInspectorPlugin::default(),
            ScreenshotPlugin::default(),
            DebugGizmosPlugin { wireframe: true, ..default() },
            PlayerPlugin::default(),
        ))
        .add_systems(Startup, (
            global_illumination.spawn(),
            cube.spawn(),
        ))
        .run()
}

fn is_focused(cursor: Single<&CursorOptions, With<PrimaryWindow>>) -> bool {
    cursor.grab_mode == CursorGrabMode::Locked
}

fn global_illumination() -> impl Scene {
    bsn! {
        DirectionalLight {
            shadow_maps_enabled: true,
            shadow_normal_bias: 1.0,
            shadow_depth_bias: 0.05,
        }
        Transform::from_rotation(
            Quat::from_euler(EulerRot::XYZ, 6.0, -7.0, 0.0)
        )
    }
}

fn cube() -> impl Scene {
    bsn! {
        #Cube
        Transform::from_xyz(0.5, 0.5, 0.5)
        Mesh3d(asset_value(Cuboid::from_length(1.0)))
        MeshMaterial3d<StandardMaterial>(asset_value(Color::linear_rgb(0.2, 0.7, 0.3)))
    }
}
