mod chunk;
mod coords;
mod debug_gizmos;
mod meshdata;
mod player;
mod screenshot;
mod voxel_world;

use crate::{
    chunk::ChunkData,
    coords::{ChunkCoord, Direction, VoxelCoord},
    debug_gizmos::DebugGizmosPlugin,
    meshdata::MeshData,
    player::PlayerPlugin,
    screenshot::ScreenshotPlugin,
    voxel::VoxelType,
    voxel_world::{ChunkMaterial, VoxelWorld, VoxelWorldPlugin},
};

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

mod voxel {
    use crate::{Direction, TextureAtlasCoord};

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

        pub fn texture(self, direction: Direction) -> TextureAtlasCoord {
            match self {
                Self::Grass => {
                    match direction {
                        Direction::Top => TextureAtlasCoord { x: 5, y: 1 },
                        Direction::Bottom => TextureAtlasCoord { x: 7, y: 5 },
                        _ => TextureAtlasCoord { x: 7, y: 4 },
                    }
                },
                Self::Dirt => TextureAtlasCoord { x: 7, y: 5 },
                _ => unreachable!(),
            }
        }
    }
}

struct TextureAtlas {
    width: usize,
    height: usize,
}

struct TextureAtlasCoord {
    x: usize,
    y: usize,
}

impl TextureAtlas {
    #[cfg(false)]
    fn uvs(&self, _: TextureAtlasCoord) -> [Vec2; 4] {
        [
            vec2(0.0, 1.0),
            vec2(1.0, 1.0),
            vec2(0.0, 0.0),
            vec2(1.0, 0.0),
        ]
    }

    #[cfg(true)]
    fn uvs(&self, tile: TextureAtlasCoord) -> [Vec2; 4] {
        let tile_width = 1.0 / self.width as f32;
        let tile_height = 1.0 / self.height as f32;
        let x = tile.x as f32 * tile_width;
        let y = tile.y as f32 * tile_height;
        // Note that the Y axis is flipped in UV coordinates, so we need to flip the Y coordinate of the tile
        [
            vec2(x, y + tile_height),              // Bottom left
            vec2(x + tile_width, y + tile_height), // Bottom right
            vec2(x, y),                            // Top left
            vec2(x + tile_width, y),               // Top right
        ]
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
            PlayerPlugin::new(is_focused),
            VoxelWorldPlugin::default(),
        ))
        .add_systems(Startup, (
            global_illumination.spawn(),
            spawn_world,
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

fn spawn_world(
    mut commands: Commands,
    chunk_material: Res<ChunkMaterial>,
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

    for (&coord, chunk) in world.iter() {
        commands.spawn((
            Name::new(format!("Chunk {coord}")),
            Transform::from_translation(coord.position()),
            Mesh3d(meshes.add(build_mesh(&world, chunk, coord))),
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
) -> Mesh {
    let atlas = TextureAtlas { width: 9, height: 10 };
    let mut mesh = MeshData::default();

    for (voxel_pos, voxel) in chunk.iter()
        .filter(|(_, v)| !matches!(v, VoxelType::Air))
    {
        for direction in Direction::iter() {
            if is_face_visible(world, chunk, chunk_pos, voxel_pos, direction) {
                let origin = voxel_pos.local_position();
                let uvs = atlas.uvs(voxel.texture(direction));
                mesh.add_quad(direction, origin, uvs);
            }
        }
    }

    mesh.build()
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
