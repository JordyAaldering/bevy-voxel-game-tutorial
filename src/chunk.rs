use crate::{
    ChunkCoord,
    Direction,
    VoxelCoord,
    VoxelType,
    CHUNK_WIDTH,
    CHUNK_HEIGHT,
};
#[cfg(since08)]
use crate::{ChunkNeighbors, MeshData};

#[cfg(since08)]
use bevy::prelude::*;

/// ECS component of a chunk.
#[cfg(since08)]
#[derive(Component)]
pub struct Chunk {
    coord: ChunkCoord,
}

/// Marker component for a chunk that needs to be remeshed.
/// This is added when a chunk is modified, and removed when a mesh generation task is spawned.
/// If a chunk is modified whilst its mesh is being generated, it will remain dirty and be remeshed again once the task is finished.
#[cfg(since08)]
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct NeedsRemesh;

#[derive(Clone)]
pub struct ChunkData {
    #[allow(unused)]
    coord: ChunkCoord,
    voxels: [VoxelType; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH],
    #[cfg(since08)]
    entity: Option<Entity>,
}

#[cfg(since08)]
impl Chunk {
    pub fn new(coord: ChunkCoord) -> Self {
        Self { coord }
    }

    pub fn coord(&self) -> ChunkCoord {
        self.coord
    }
}

impl ChunkData {
    #[cfg(until07)]
    pub fn new(coord: ChunkCoord) -> Self {
        Self {
            coord,
            voxels: [VoxelType::Air; _],
        }
    }

    pub fn get_neighbor_voxel(&self, pos: VoxelCoord, direction: Direction) -> Option<VoxelType> {
        let neighbor_pos = pos + direction;
        if neighbor_pos.in_bounds() {
            Some(self[neighbor_pos])
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (VoxelCoord, VoxelType)> {
        (0..CHUNK_WIDTH).flat_map(move |z| {
            (0..CHUNK_HEIGHT).flat_map(move |y| {
                (0..CHUNK_WIDTH).map(move |x| {
                    let pos = VoxelCoord::new(x, y, z);
                    (pos, self[pos])
                })
            })
        })
    }
}

#[cfg(since08)]
impl ChunkData {
    pub fn new(coord: ChunkCoord, entity: Entity) -> Self {
        Self {
            coord,
            voxels: [VoxelType::Air; _],
            entity: Some(entity),
        }
    }

    pub fn coord(&self) -> ChunkCoord {
        self.coord
    }

    pub fn entity(&self) -> Option<Entity> {
        return self.entity;
    }

    pub fn set_entity(&mut self, entity: Entity) {
        debug_assert!(self.entity.is_none());
        self.entity = Some(entity);
    }

    /// Builds the mesh from owned data only, so it can run on another thread.
    pub fn build_mesh(
        &self,
        neighbors: &ChunkNeighbors,
        atlas: &TextureAtlasLayout,
    ) -> Mesh {
        let mut mesh_data = MeshData::new(atlas);

        for (voxel_pos, voxel) in self.iter()
            .filter(|(_, v)| !matches!(v, VoxelType::Air))
        {
            for direction in Direction::ALL {
                if self.is_face_visible(neighbors, voxel_pos, direction) {
                    let origin = voxel_pos.local_position();
                    let texture = voxel.texture(direction);
                    mesh_data.add_quad(direction, origin, texture);
                }
            }
        }

        mesh_data.build()
    }

    fn is_face_visible(
        &self,
        neighbors: &ChunkNeighbors,
        voxel_pos: VoxelCoord,
        direction: Direction,
    ) -> bool {
        use bevy::math::CompassQuadrant;

        if let Some(neighbor) = self.get_neighbor_voxel(voxel_pos, direction) {
            return neighbor.is_transparent();
        }

        // Chunks only exist horizontally, always show the top-most but never the bottom-most faces
        let Ok(compass) = direction.try_into() else {
            return direction == Direction::Top;
        };

        // If the chunk does not exist, we assume it is empty and show the face
        let Some(neighbor_chunk) = &neighbors[compass] else {
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

}

impl std::ops::Index<VoxelCoord> for ChunkData {
    type Output = VoxelType;

    fn index(&self, pos: VoxelCoord) -> &Self::Output {
        &self.voxels[pos.index()]
    }
}

impl std::ops::IndexMut<VoxelCoord> for ChunkData {
    fn index_mut(&mut self, pos: VoxelCoord) -> &mut Self::Output {
        &mut self.voxels[pos.index()]
    }
}
