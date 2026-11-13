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
mod screenshot;
mod telemetry;
mod voxel_world;

use crate::{
    chunk::{Chunk, ChunkData, ChunkDataChanged, ChunkGenerationTask, ChunkGenerationPhase, ChunkGenerationResult, NeedsDespawn, NeedsRemesh, NeedsGenerationCheck},
    chunk_neighbors::ChunkNeighbors,
    coords::{ChunkCoord, Direction, VoxelCoord, WorldCoord},
    debug_gizmos::DebugGizmosPlugin,
    editor::{EditorPlugin, EditorState},
    gamemode::{GameModePlugin, is_gamemode_spectator, is_gamemode_survival},
    meshdata::MeshData,
    noise::NoiseSettings,
    physics::PhysicsPlugin,
    player::{Player, PlayerCamera, PlayerPlugin},
    screenshot::ScreenshotPlugin,
    telemetry::TelemetryPlugin,
    voxel::VoxelType,
    voxel_world::{VoxelWorld, VoxelWorldPlugin, ATLAS_COLUMNS},
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
            DebugGizmosPlugin::default(),
            GameModePlugin::default(),
            PlayerPlugin::default(),
            PhysicsPlugin::new(is_gamemode_survival),
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
