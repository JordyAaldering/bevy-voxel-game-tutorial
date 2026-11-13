mod chunk;
mod coords;
mod debug_gizmos;
mod meshdata;
mod physics;
mod player;
mod screenshot;
mod voxel_world;

use crate::{
    chunk::ChunkData,
    coords::{ChunkCoord, Direction, VoxelCoord, WorldCoord},
    debug_gizmos::DebugGizmosPlugin,
    meshdata::MeshData,
    physics::PhysicsPlugin,
    player::{Player, PlayerPlugin},
    screenshot::ScreenshotPlugin,
    voxel::VoxelType,
    voxel_world::{ChunkAtlas, ChunkMaterial, VoxelWorld, VoxelWorldPlugin, ATLAS_COLUMNS},
};

use better_default::Default;
use bevy::{
    prelude::*,
    math::CompassQuadrant,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_inspector_egui::{
    bevy_egui::prelude::*,
    quick::WorldInspectorPlugin,
};

pub const CHUNK_WIDTH: usize = 3;
pub const CHUNK_HEIGHT: usize = 4;

#[derive(Clone, Copy, Default, PartialEq, Eq, Resource)]
enum GameMode {
    #[default]
    Survival,
    Spectator,
}

mod voxel {
    use crate::{Direction, ATLAS_COLUMNS};

    #[derive(Clone, Copy)]
    pub enum VoxelType {
        Air,
        Grass,
        Dirt,
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
            WorldInspectorPlugin::default(),
            ScreenshotPlugin::default(),
            DebugGizmosPlugin::default(),
            PlayerPlugin::default(),
            PhysicsPlugin::new(is_gamemode_survival),
            VoxelWorldPlugin::default(),
        ))
        .init_resource::<GameMode>()
        .add_systems(Startup, (
            global_illumination.spawn(),
            spawn_world,
        ))
        .add_systems(Update, (
            toggle_gamemode.run_if(is_focused),
        ))
        .run()
}

fn is_focused(cursor: Single<&CursorOptions, With<PrimaryWindow>>) -> bool {
    cursor.grab_mode == CursorGrabMode::Locked
}

fn is_gamemode_survival(mode: Res<GameMode>) -> bool {
    *mode == GameMode::Survival
}

fn is_gamemode_spectator(mode: Res<GameMode>) -> bool {
    *mode == GameMode::Spectator
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

fn spawn_world(
    mut commands: Commands,
    chunk_atlas: Res<ChunkAtlas>,
    chunk_material: Res<ChunkMaterial>,
    layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut world: ResMut<VoxelWorld>,
) {
    for z in -1..=1 {
        for x in -1..=1 {
            let coord = ChunkCoord::new(x, z);
            let chunk = generate_chunk(coord);
            world.add_chunk(coord, chunk);
        }
    }

    let atlas = layouts.get(&chunk_atlas.0).unwrap();

    for (&coord, chunk) in world.iter() {
        commands.spawn((
            Name::new(format!("Chunk {coord}")),
            Transform::from_translation(coord.position()),
            Mesh3d(meshes.add(build_mesh(&world, chunk, coord, atlas))),
            MeshMaterial3d(chunk_material.0.clone()),
        ));
    }
}

fn generate_chunk(coord: ChunkCoord) -> ChunkData {
    let mut chunk = ChunkData::new(coord);

    for z in 0..CHUNK_WIDTH {
        let max_y = (coord.x + coord.z) as usize % CHUNK_HEIGHT;
        for y in 0..=max_y {
            for x in 0..CHUNK_WIDTH {
                let pos = VoxelCoord::new(x, y, z);
                chunk[pos] = if y == max_y {
                    VoxelType::Grass
                } else {
                    VoxelType::Dirt
                };
            }
        }
    }

    chunk
}

fn build_mesh(
    world: &VoxelWorld,
    chunk: &ChunkData,
    chunk_pos: ChunkCoord,
    atlas: &TextureAtlasLayout,
) -> Mesh {
    let mut mesh_data = MeshData::new(atlas);

    for (voxel_pos, voxel) in chunk.iter()
        .filter(|(_, v)| !matches!(v, VoxelType::Air))
    {
        for direction in Direction::ALL {
            if is_face_visible(world, chunk, chunk_pos, voxel_pos, direction) {
                let origin = voxel_pos.local_position();
                let texture = voxel.texture(direction);
                mesh_data.add_quad(direction, origin, texture);
            }
        }
    }

    mesh_data.build()
}

fn is_face_visible(
    world: &VoxelWorld,
    chunk: &ChunkData,
    chunk_pos: ChunkCoord,
    voxel_pos: VoxelCoord,
    direction: Direction,
) -> bool {
    if let Some(neighbor) = chunk.get_neighbor_voxel(voxel_pos, direction) {
        return neighbor.is_transparent();
    }

    // Chunks only exist horizontally, always show the top-most but never the bottom-most faces
    let Ok(compass) = direction.try_into() else {
        return direction == Direction::Top;
    };

    // If the chunk does not exist, we assume it is empty and show the face
    let Some(neighbor_chunk) = world.get_chunk(chunk_pos + compass) else {
        return true;
    };

    let neighbor_pos = match compass {
        CompassQuadrant::North => VoxelCoord { z: 0, ..voxel_pos },
        CompassQuadrant::East  => VoxelCoord { x: 0, ..voxel_pos },
        CompassQuadrant::South => VoxelCoord { z: CHUNK_WIDTH - 1, ..voxel_pos },
        CompassQuadrant::West  => VoxelCoord { x: CHUNK_WIDTH - 1, ..voxel_pos },
    };

    neighbor_chunk[neighbor_pos]
        .is_transparent()
}

fn toggle_gamemode(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut player: Single<&mut Player>,
    mut mode: ResMut<GameMode>,
) {
    if keyboard.just_pressed(KeyCode::KeyF) {
        *mode = match *mode {
            GameMode::Survival => GameMode::Spectator,
            GameMode::Spectator => GameMode::Survival,
        };
        // Don't retain old vertical momentum
        player.velocity = Vec3::ZERO;
    }
}
