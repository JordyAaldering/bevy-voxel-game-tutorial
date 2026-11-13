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
    voxel_world::{VoxelAtlas, VoxelWorld, VoxelWorldPlugin, ATLAS_COLUMNS},
};

use std::sync::Arc;

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, futures_lite::future, block_on},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
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
        .init_resource::<VoxelWorld>()
        .init_resource::<TerrainConfig>()
        .add_observer(on_chunk_data_changed)
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

fn terrain_height(config: &TerrainConfig, world_x: i32, world_z: i32) -> usize {
    let height = config.height.sample2(config.seed, world_x as f32, world_z as f32) as i32;
    (height as usize).clamp(0, CHUNK_HEIGHT - 1)
}

fn generate_chunk_height(chunk: &mut ChunkData, config: &TerrainConfig) {
    let heights = config.height.area2(config.seed, chunk.coord().x, chunk.coord().z);
    for x in 0..CHUNK_WIDTH {
        for z in 0..CHUNK_WIDTH {
            let height = (heights[x + z * CHUNK_WIDTH] as i32 as usize).clamp(0, CHUNK_HEIGHT - 1);

            // Top layer is grass
            chunk[VoxelCoord::new(x, height, z)] = VoxelType::Grass;
            // Two layers of dirt
            for y in (height - 2).max(0)..height {
                chunk[VoxelCoord::new(x, y, z)] = VoxelType::Dirt;
            }
            // Below that is stone
            for y in 0..(height - 2).max(0) {
                chunk[VoxelCoord::new(x, y, z)] = VoxelType::Stone;
            }
        }
    }
}

fn on_chunk_data_changed(
    event: On<ChunkDataChanged>,
    chunks: Query<&Chunk>,
    world: Res<VoxelWorld>,
    mut commands: Commands,
) {
    // May be None if the chunk was despawned before the event was processed
    let Ok(chunk) = chunks.get(event.entity) else {
        return;
    };

    let chunk = world.get_chunk(chunk.coord()).unwrap();

    // Ignore events for entities that have since been replaced by a reloaded chunk.
    if chunk.entity() == Some(event.entity) {
        commands.entity(event.entity).insert(NeedsGenerationCheck);
    }
}

fn poll_chunk_generation_tasks(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tasks: Query<(Entity, &Chunk, &mut ChunkGenerationTask, Option<&Mesh3d>)>,
) -> Result<(), BevyError> {
    for (entity, chunk, mut task, mesh3d) in &mut tasks {
        let Some(result) = block_on(future::poll_once(&mut task.0)) else {
            continue;
        };

        match result {
            ChunkGenerationResult::Height(generated) => {
                let data = world.get_chunk_mut(chunk.coord()).unwrap();

                assert_eq!(data.generation_phase, ChunkGenerationPhase::HeightGeneration);

                data.voxels = generated.voxels;
                data.generation_phase = data.generation_phase.next();
                world.notify_chunk(&mut commands, chunk.coord());
                world.notify_neighbors(&mut commands, chunk.coord());
            }
            ChunkGenerationResult::Mesh(mesh) => {
                match mesh3d {
                    Some(handle) => {
                        meshes.insert(&handle.0, mesh)?;
                    }
                    None => {
                        commands.entity(entity).insert(Mesh3d(meshes.add(mesh)));
                    }
                }

                let data = world.get_chunk_mut(chunk.coord()).unwrap();
                assert!(data.generation_phase >= ChunkGenerationPhase::MeshGeneration);
                data.generation_phase = data.generation_phase.next();
            }
        }

        commands.entity(entity).remove::<ChunkGenerationTask>();
    }

    Ok(())
}

fn start_chunk_generation_tasks(
    mut commands: Commands,
    world: Res<VoxelWorld>,
    config: Res<TerrainConfig>,
    atlas: Res<VoxelAtlas>,
    chunks: Query<(Entity, &Chunk), (With<NeedsGenerationCheck>, Without<ChunkGenerationTask>)>,
) {
    let pool = AsyncComputeTaskPool::get();

    for (entity, chunk) in &chunks {
        let data = &world.get_chunk(chunk.coord()).unwrap();

        let phase = data.generation_phase;
        commands.entity(entity).remove::<NeedsGenerationCheck>();

        let task = match phase {
            ChunkGenerationPhase::HeightGeneration => {
                let data = Arc::clone(data);
                let config = config.clone();
                info!("Generating height data for chunk {}", chunk.coord());
                pool.spawn(async move {
                    let mut generated = Box::new((*data).clone());
                    generate_chunk_height(&mut generated, &config);
                    ChunkGenerationResult::Height(generated)
                })
            }
            ChunkGenerationPhase::MeshGeneration | ChunkGenerationPhase::Done => {
                if !world.neighbors_ready_for(chunk.coord(), ChunkGenerationPhase::MeshGeneration) {
                    continue;
                }

                let data = Arc::clone(data);
                let neighbors = ChunkNeighbors::from(chunk.coord(), &world.chunks);
                let atlas = atlas.0.clone();
                trace!("Generating mesh for chunk {}", chunk.coord());
                pool.spawn(async move {
                    ChunkGenerationResult::Mesh(build_mesh(&data, &neighbors, &atlas))
                })
            }
        };

        commands.entity(entity).insert(ChunkGenerationTask(task));
    }
}
