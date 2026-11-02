use std::ops::{Index, IndexMut};

use bevy::prelude::*;

use crate::{chunk_mesh::MeshData, face::{Face, FaceDirection}, texture_atlas::TextureAtlas, voxel::VoxelPosition, world::VoxelWorld};

use super::VoxelType;

pub const CHUNK_WIDTH: usize = 32;
pub const CHUNK_HEIGHT: usize = 64;

#[derive(Clone)]
pub struct ChunkData {
    position: ChunkPosition,
    voxels: [VoxelType; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH],
    /// `None` while the chunk is unloaded: the data is kept, but there is no entity.
    entity: Option<Entity>,
}

/// ECS side of a chunk. The voxel data itself lives in [`VoxelWorld`], keyed by this position.
#[derive(Component, Clone, Copy, Debug)]
pub struct Chunk {
    pub position: ChunkPosition,
}

#[derive(Component)]
pub struct NeedsRemesh;

#[derive(Component)]
pub struct NeedsDespawn;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkPosition {
    pub x: i32,
    pub z: i32,
}

impl Chunk {
    pub fn new(position: ChunkPosition) -> Self {
        Self { position }
    }
}

impl ChunkData {
    pub fn new(position: ChunkPosition, entity: Entity) -> Self {
        Self {
            position,
            voxels: [VoxelType::Air; _],
            entity: Some(entity),
        }
    }

    pub fn entity(&self) -> Option<Entity> {
        self.entity
    }

    pub fn set_entity(&mut self, entity: Entity) {
        debug_assert!(self.entity.is_none(), "ChunkData already has an entity");
        self.entity = Some(entity);
    }

    pub fn clear_entity(&mut self) {
        self.entity = None;
    }

    pub fn position(&self) -> ChunkPosition {
        self.position
    }

    pub fn build_mesh(&self, world: &VoxelWorld) -> Mesh {
        let atlas = TextureAtlas::new(4, 4);
        let mut mesh = MeshData::new(atlas);

        for z in 0..CHUNK_WIDTH {
        for y in 0..CHUNK_HEIGHT {
        for x in 0..CHUNK_WIDTH {
            let pos = VoxelPosition { x, y, z };
            let voxel = self.voxels[pos.to_index()];

            if voxel == VoxelType::Air {
                continue;
            }

            let origin = pos.origin();

            for direction in [
                FaceDirection::Right,
                FaceDirection::Left,
                FaceDirection::Top,
                FaceDirection::Bottom,
                FaceDirection::Front,
                FaceDirection::Back,
            ] {
                if let Some(neighbor) = self.get_neighbor_voxel(pos, direction) {
                    if !neighbor.is_transparent() {
                        continue;
                    }
                } else {
                    // If chunk.get_neighbor_voxel returns None, it means that the neighbor is out of bounds.
                    // In that case, we need to check the neighboring chunk in the world
                    let (dx, dz) = match direction {
                        FaceDirection::Right => (1, 0),
                        FaceDirection::Left => (-1, 0),
                        FaceDirection::Front => (0, 1),
                        FaceDirection::Back => (0, -1),
                        // We are at the build limit, so we always have to show this face
                        FaceDirection::Top => (0, 0),
                        FaceDirection::Bottom => (0, 0),
                    };

                    if dx != 0 || dz != 0 {
                        if let Some(neighbor_chunk) = world.get_chunk_neighbor(self.position, dx, dz) {
                            let (x, y, z) = (pos.x, pos.y, pos.z);
                            let (nx, ny, nz) = match direction {
                                FaceDirection::Right => (0, y, z),
                                FaceDirection::Left => (CHUNK_WIDTH - 1, y, z),
                                FaceDirection::Front => (x, y, 0),
                                FaceDirection::Back => (x, y, CHUNK_WIDTH - 1),
                                FaceDirection::Top => unreachable!(),
                                FaceDirection::Bottom => unreachable!(),
                            };
                            let neighbor_pos = VoxelPosition::new(nx, ny, nz);
                            if !neighbor_chunk[neighbor_pos].is_transparent() {
                                continue;
                            }
                        } else {
                            // No neighboring chunk, so assume that we don't need to render this face
                            continue;
                        }
                    }
                }

                let face = Face::new(origin, direction);
                let texture = voxel.texture(direction);
                mesh.add_face(face, texture);
            }
        }
        }
        }

        mesh.build()
    }

    pub fn get_neighbor_voxel(&self, pos: VoxelPosition, direction: FaceDirection) -> Option<VoxelType> {
        let VoxelPosition { x, y, z } = pos;

        let (x, y, z) = match direction {
            FaceDirection::Right => (x + 1, y, z),
            FaceDirection::Left => (x.wrapping_sub(1), y, z),
            FaceDirection::Top => (x, y + 1, z),
            FaceDirection::Bottom => (x, y.wrapping_sub(1), z),
            FaceDirection::Front => (x, y, z + 1),
            FaceDirection::Back => (x, y, z.wrapping_sub(1)),
        };

        if x < CHUNK_WIDTH && y < CHUNK_HEIGHT && z < CHUNK_WIDTH {
            let pos = VoxelPosition { x, y, z };
            let idx = pos.to_index();
            Some(self.voxels[idx])
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (VoxelPosition, VoxelType)> {
        (0..CHUNK_WIDTH).flat_map(move |z| {
            (0..CHUNK_HEIGHT).flat_map(move |y| {
                (0..CHUNK_WIDTH).map(move |x| {
                    let pos = VoxelPosition { x, y, z };
                    let voxel = self.voxels[pos.to_index()];
                    (pos, voxel)
                })
            })
        })
    }
}

impl Index<VoxelPosition> for ChunkData {
    type Output = VoxelType;

    fn index(&self, pos: VoxelPosition) -> &Self::Output {
        &self.voxels[pos.to_index()]
    }
}

impl IndexMut<VoxelPosition> for ChunkData {
    fn index_mut(&mut self, pos: VoxelPosition) -> &mut Self::Output {
        &mut self.voxels[pos.to_index()]
    }
}

impl ChunkPosition {
    pub fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    /// This chunk's origin in world space.
    pub fn origin(self) -> Vec3 {
        vec3(
            self.x as f32 * CHUNK_WIDTH as f32,
            0.0,
            self.z as f32 * CHUNK_WIDTH as f32,
        )
    }
}
