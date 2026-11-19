mod debug_gizmos;
mod player;
mod scene;
mod screenshot;
mod settings;

use crate::{
    debug_gizmos::*,
    player::*,
    scene::*,
    screenshot::*,
    settings::*,
};

use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_inspector_egui::{
    bevy_egui::prelude::*,
    quick::{ResourceInspectorPlugin, WorldInspectorPlugin},
};

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            EguiPlugin::default(),
            WorldInspectorPlugin::default(),
            ScreenshotPlugin::default(),
            SettingsPlugin::default(),
            ResourceInspectorPlugin::<InputSettings>::default(),
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

fn cube() -> impl Scene {
    bsn! {
        #Cube
        Transform::from_xyz(0.5, 0.5, 0.5)
        Mesh3d(asset_value(Cuboid::from_length(1.0)))
        MeshMaterial3d<StandardMaterial>(asset_value(Color::linear_rgb(0.2, 0.7, 0.3)))
    }
}
