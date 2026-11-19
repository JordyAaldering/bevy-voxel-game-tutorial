mod debug_gizmos;
mod coords;
mod meshdata;
mod player;
mod scene;
mod screenshot;
mod settings;

use crate::{
    debug_gizmos::*,
    coords::*,
    meshdata::*,
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
    quick::WorldInspectorPlugin,
};

pub const CHUNK_WIDTH: usize = 3;
pub const CHUNK_HEIGHT: usize = 4;

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            EguiPlugin::default(),
            WorldInspectorPlugin::default(),
            ScreenshotPlugin::default(),
            SettingsPlugin::default(),
            DebugGizmosPlugin { wireframe: true, ..default() },
            PlayerPlugin::default(),
        ))
        .add_systems(Startup, (
            global_illumination.spawn(),
            voxel.spawn(),
        ))
        .run()
}

fn is_focused(cursor: Single<&CursorOptions, With<PrimaryWindow>>) -> bool {
    cursor.grab_mode == CursorGrabMode::Locked
}

fn voxel() -> impl Scene {
    bsn! {
        #Voxel
        Transform
        Mesh3d(asset_value(build_mesh()))
        MeshMaterial3d<StandardMaterial>(asset_value(Color::linear_rgb(0.2, 0.7, 0.3)))
    }
}

fn build_mesh() -> Mesh {
    let mut mesh = MeshData::default();

    for direction in Direction::ALL {
        mesh.add_quad(direction, Vec3::ZERO);
    }

    mesh.build()
}
