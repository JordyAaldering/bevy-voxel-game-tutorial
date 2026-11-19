mod chunk;
mod chunk_neighbors;
mod coords;
mod debug_gizmos;
mod editor;
mod gamemode;
mod meshdata;
mod noise;
mod physics;
mod player;
mod raycast;
mod scene;
mod screenshot;
mod settings;
mod telemetry;
mod voxel_world;

use crate::{
    chunk::*,
    chunk_neighbors::*,
    coords::*,
    debug_gizmos::*,
    editor::*,
    gamemode::*,
    meshdata::*,
    noise::*,
    physics::*,
    player::*,
    raycast::*,
    scene::*,
    screenshot::*,
    settings::*,
    telemetry::*,
    voxel::*,
    voxel_world::*,
};

use bevy::{prelude::*, window::{CursorGrabMode, CursorOptions, PrimaryWindow}};
use bevy_inspector_egui::bevy_egui::prelude::*;

pub const CHUNK_WIDTH: usize = 32;
pub const CHUNK_HEIGHT: usize = 64;

#[derive(Clone, Resource)]
struct TerrainConfig {
    seed: i32,
    height: NoiseSettings,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            height: NoiseSettings {
                amplitude: 10.0,
                frequency: 0.01,
                bias: 20.0,
                ..default()
            },
        }
    }
}

mod voxel {
    use crate::{Direction, ATLAS_COLUMNS};

    #[derive(Clone, Copy)]
    pub enum VoxelType {
        Air,
        Grass,
        Dirt,
        Stone,
    }

    impl VoxelType {
        pub fn is_transparent(self) -> bool {
            matches!(self, VoxelType::Air)
        }

        pub fn is_solid(self) -> bool {
            !matches!(self, VoxelType::Air)
        }

        pub fn texture(self, direction: Direction) -> usize {
            fn atlas_index(x: usize, y: usize) -> usize {
                x + (y * ATLAS_COLUMNS as usize)
            }

            match self {
                Self::Grass => {
                    match direction {
                        Direction::Top => atlas_index(5, 1),
                        Direction::Bottom => atlas_index(7, 5),
                        _ => atlas_index(7, 4),
                    }
                },
                Self::Dirt => atlas_index(7, 5),
                Self::Stone => atlas_index(3, 4),
                _ => unreachable!(),
            }
        }
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest()),
            EguiPlugin::default(),
            EditorPlugin,
            TelemetryPlugin,
            ScreenshotPlugin::default(),
            SettingsPlugin::default(),
            DebugGizmosPlugin::default(),
            GameModePlugin::default(),
            PlayerPlugin::default(),
            PhysicsPlugin::default(),
            VoxelWorldPlugin::default(),
        ))
        .init_resource::<TerrainConfig>()
        .add_systems(Startup, (
            global_illumination.spawn(),
        ))
        .run()
}

fn is_focused(cursor: Single<&CursorOptions, With<PrimaryWindow>>) -> bool {
    cursor.grab_mode == CursorGrabMode::Locked
}

/// The game view only receives input while it has captured the cursor.
fn game_view_captured(
    editor_state: Res<EditorState>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
) -> bool {
    editor_state.is_game_view_active()
        && editor_state.is_game_view_visible()
        && cursor.grab_mode == CursorGrabMode::Locked
}
