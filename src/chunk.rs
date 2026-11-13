#[cfg(since13)]
mod phase;

#[cfg(since13)]
pub use phase::*;

use crate::*;

#[allow(unused_imports)]
use std::path::PathBuf;

#[allow(unused_imports)]
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

#[cfg(since10)]
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct NeedsDespawn;

#[derive(Clone)]
#[derive(wincode::SchemaWrite, wincode::SchemaRead)]
pub struct ChunkData {
    #[allow(unused)]
    coord: ChunkCoord,
    voxels: [VoxelType; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH],
    #[cfg(since08)]
    #[wincode(skip)]
    entity: Option<Entity>,
    #[cfg(since13)]
    pub generation_phase: ChunkGenerationPhase,
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

    pub fn set_voxels(&mut self, other: ChunkData) {
        self.voxels = other.voxels;
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
            #[cfg(since13)]
            generation_phase: ChunkGenerationPhase::HeightGeneration,
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

    #[cfg(since10)]
    pub fn clear_entity(&mut self) {
        self.entity = None;
    }

    /// Builds the mesh from owned data only, so it can run on another thread.
    pub fn build_mesh(
        &self,
        neighbors: &ChunkNeighbors,
        #[cfg(until14)] atlas: &TextureAtlasLayout,
        #[cfg(since15)] chunk_pos: ChunkCoord,
        #[cfg(since15)] atlas: &VoxelAtlas,
    ) -> Mesh {
        #[cfg(until14)]
        let mut mesh = MeshData::new(atlas);
        #[cfg(since15)]
        let mut mesh = MeshData::new(atlas.layout());

        for (voxel_pos, voxel) in self.iter()
            .filter(|(_, v)| !matches!(v, VoxelType::Air))
        {
            for direction in Direction::ALL {
                if self.is_face_visible(neighbors, voxel_pos, direction) {
                    let origin = voxel_pos.local_position();

                    #[cfg(until14)]
                    let texture = voxel.texture(direction);
                    #[cfg(since15)]
                    let texture = atlas.texture(voxel, direction, variant_seed(chunk_pos, voxel_pos));

                    mesh.add_quad(direction, origin, texture);
                }
            }
        }

        mesh.build()
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

#[cfg(since14)]
impl ChunkData {
    fn path(coord: ChunkCoord) -> PathBuf {
        Path::new(SAVE_DIR).join(format!("chunk_{}_{}.bin", coord.x, coord.z))
    }

    pub fn save(&self) {
        let result = wincode::serialize(self)
            .map_err(|e| e.to_string())
            .and_then(|bytes| write_atomic(&Self::path(self.coord), &bytes).map_err(|e| e.to_string()));
        if let Err(error) = result {
            error!("Failed to save chunk {}: {error}", self.coord);
        }
    }

    /// Returns `None` if there is no saved chunk, or it could not be read.
    pub fn load(coord: ChunkCoord, entity: Entity) -> Option<Self> {
        let bytes = match fs::read(Self::path(coord)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(error) => {
                error!("Failed to read chunk {coord}: {error}");
                return None;
            }
        };

        match wincode::deserialize::<Self>(&bytes) {
            Ok(mut chunk) if chunk.coord == coord => {
                chunk.entity = Some(entity);
                // Meshes are not saved, so a loaded chunk always has to be meshed again
                chunk.generation_phase = chunk.generation_phase.min(ChunkGenerationPhase::MeshGeneration);
                Some(chunk)
            }
            Ok(_) => {
                error!("Saved chunk file for {coord} contains a different chunk");
                None
            }
            Err(error) => {
                error!("Failed to decode chunk {coord}: {error}");
                None
            }
        }
    }
}

/// Deterministic per-position hash, so a voxel keeps its variant when remeshed or reloaded.
#[cfg(since15)]
fn variant_seed(chunk_pos: ChunkCoord, voxel_pos: VoxelCoord) -> u32 {
    let x = chunk_pos.x.wrapping_mul(CHUNK_WIDTH as i32).wrapping_add(voxel_pos.x as i32);
    let z = chunk_pos.z.wrapping_mul(CHUNK_WIDTH as i32).wrapping_add(voxel_pos.z as i32);
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1)
        ^ (voxel_pos.y as u32).wrapping_mul(0x85EB_CA77)
        ^ (z as u32).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 13)
}
