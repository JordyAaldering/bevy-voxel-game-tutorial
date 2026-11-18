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

#[cfg(since19)]
#[derive(Component)]
pub struct ChunkMeshes {
    pub voxels: Entity,
    pub terrain: Entity,
}

#[cfg(since19)]
#[derive(Component)]
pub struct VoxelMesh;

#[cfg(since19)]
#[derive(Component)]
pub struct TerrainMesh;

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
#[cfg_attr(since14, derive(wincode::SchemaWrite, wincode::SchemaRead))]
pub struct ChunkData {
    #[allow(unused)]
    pub coord: ChunkCoord,
    voxels: [VoxelType; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH],
    #[cfg(since08)]
    #[cfg_attr(since14, wincode(skip))]
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

    #[cfg(since13)]
    pub fn set_voxels(&mut self, other: ChunkData) {
        self.voxels = other.voxels;
    }

    #[cfg(since19)]
    pub fn set_voxel(&mut self, pos: VoxelCoord, voxel: VoxelType) {
        self[pos] = voxel;
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
            #[cfg(until18)]
            voxels: [VoxelType::Air; _],
            #[cfg(since19)]
            voxels: [VoxelType::AIR; _],
            entity: Some(entity),
            #[cfg(since13)]
            generation_phase: ChunkGenerationPhase::HeightGeneration,
        }
    }

    pub fn coord(&self) -> ChunkCoord {
        self.coord
    }

    pub fn entity(&self) -> Option<Entity> {
        self.entity
    }

    pub fn has_entity(&self) -> bool {
        self.entity.is_some()
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
    #[cfg(until18)]
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

    #[cfg(since19)]
    pub fn build_mesh(
        &self,
        neighbors: &ChunkNeighbors,
        chunk_pos: ChunkCoord,
        atlas: &VoxelAtlas,
    ) -> Mesh {
        let mut voxel_mesh = MeshData::new(atlas.layout());

        for (voxel_pos, voxel) in self.iter()
            .filter(|(_, v)| v.is_block())
        {
            for direction in Direction::ALL {
                if self.is_face_visible(neighbors, voxel_pos, direction) {
                    let origin = voxel_pos.local_position();
                    let texture = atlas.texture(voxel, direction, variant_seed(chunk_pos, voxel_pos));
                    voxel_mesh.add_quad(direction, origin, texture);
                }
            }
        }

        voxel_mesh.build()
    }

    #[cfg(since19)]
    pub fn build_terrain_mesh(
        &self,
        neighbors: &ChunkNeighbors,
        atlas: &VoxelAtlas,
    ) -> Mesh {
        // self.build_density_terrain_mesh(neighbors, atlas)
        self.build_binary_terrain_mesh(neighbors, atlas)
    }

    #[cfg(since19)]
    fn build_binary_terrain_mesh(&self, neighbors: &ChunkNeighbors, atlas: &VoxelAtlas) -> Mesh {
        use crate::marching::OFFSETS;

        let north = neighbors.north.as_deref().expect("Terrain meshing requires the north neighbor");
        let north_east = neighbors.north_east.as_deref().expect("Terrain meshing requires the north-east neighbor");
        let east = neighbors.east.as_deref().expect("Terrain meshing requires the east neighbor");

        let sample = |x: usize, y: usize, z: usize| {
            if y == CHUNK_HEIGHT {
                return None;
            }

            let chunk = match (x == CHUNK_WIDTH, z == CHUNK_WIDTH) {
                (false, false) => self,
                (false, true) => north,
                (true, false) => east,
                (true, true) => north_east,
            };

            Some((chunk, VoxelCoord::new(x % CHUNK_WIDTH, y, z % CHUNK_WIDTH)))
        };

        let index = |x: usize, y: usize, z: usize| {
            x + (CHUNK_WIDTH + 1) * (y + (CHUNK_HEIGHT + 1) * z)
        };

        let mut terrain_field = vec![false; (CHUNK_WIDTH + 1) * (CHUNK_HEIGHT + 1) * (CHUNK_WIDTH + 1)];
        for z in 0..=CHUNK_WIDTH {
            for y in 0..=CHUNK_HEIGHT {
                for x in 0..=CHUNK_WIDTH {
                    terrain_field[index(x, y, z)] = sample(x, y, z)
                        .is_some_and(|(chunk, pos)| chunk[pos].is_terrain());
                }
            }
        }

        let mut mesh = MeshData::new(atlas.layout());
        for z in 0..CHUNK_WIDTH {
            for y in 0..CHUNK_HEIGHT {
                for x in 0..CHUNK_WIDTH {
                    let corners = OFFSETS.map(|(dx, dy, dz)| {
                        let (x, y, z) = (x + dx, y + dy, z + dz);
                        (vec3(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5), terrain_field[index(x, y, z)])
                    });

                    if corners.iter().all(|corner| corner.1)
                        || corners.iter().all(|corner| !corner.1)
                    {
                        continue;
                    }

                    let samples = OFFSETS.map(|(dx, dy, dz)| sample(x + dx, y + dy, z + dz));
                    mesh.add_binary_marching_cube(corners, |corner, direction| {
                        let (chunk, pos) = samples[corner]
                            .expect("A solid terrain sample must reference a voxel");
                        let voxel = chunk[pos];
                        Some(atlas.texture(voxel, direction, variant_seed(chunk.coord, pos)))
                    });
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
            #[cfg(since19)]
            if neighbor.is_terrain() {
                return true;
            }
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

        let neighbor = neighbor_chunk[neighbor_pos];
        #[cfg(since19)]
        if neighbor.is_terrain() {
            return true;
        }
        neighbor.is_transparent()
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

#[cfg(all(test, since19))]
mod binary_terrain_tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    use std::sync::Arc;

    #[test]
    fn flat_terrain_keeps_a_flat_surface_across_chunk_edges() {
        let coord = ChunkCoord::new(0, 0);
        let mut chunks = bevy::platform::collections::HashMap::default();
        for z in -1..=1 {
            for x in -1..=1 {
                let coord = ChunkCoord::new(x, z);
                let mut chunk = ChunkData::new(coord, Entity::PLACEHOLDER);
                for z in 0..CHUNK_WIDTH {
                    for x in 0..CHUNK_WIDTH {
                        for y in 0..3 {
                            chunk[VoxelCoord::new(x, y, z)] = VoxelType::from_terrain(TerrainType::Stone);
                        }
                    }
                }
                chunks.insert(coord, Arc::new(chunk));
            }
        }
        let mut images = Assets::<Image>::default();
        let (atlas, _) = build_voxel_atlas(&mut images).unwrap();
        let neighbors = ChunkNeighbors::from(coord, &chunks);
        let mesh = chunks[&coord].build_terrain_mesh(&neighbors, &atlas);
        let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("missing positions")
        };
        let area: f32 = positions.chunks_exact(3).map(|triangle| {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]].map(Vec3::from_array);
            (b - a).cross(c - a).length() / 2.0
        }).sum();
        assert_eq!(area, (CHUNK_WIDTH * CHUNK_WIDTH) as f32);
        assert!(positions.iter().all(|position| position[1] == 3.0));
    }

    #[test]
    fn terrain_mesh_ignores_density_and_uses_neighboring_chunks() {
        let coord = ChunkCoord::new(0, 0);
        let mut chunk = ChunkData::new(coord, Entity::PLACEHOLDER);
        let voxel = VoxelType::from_terrain(TerrainType::Stone);
        let edge = VoxelCoord::new(CHUNK_WIDTH - 1, 10, CHUNK_WIDTH - 1);
        chunk[edge] = voxel;
        let mut chunks = bevy::platform::collections::HashMap::default();
        for coord in [ChunkCoord::new(1, 0), ChunkCoord::new(0, 1), ChunkCoord::new(1, 1)] {
            chunks.insert(coord, Arc::new(ChunkData::new(coord, Entity::PLACEHOLDER)));
        }
        let diagonal = ChunkCoord::new(1, 1);
        let diagonal_pos = VoxelCoord::new(0, 10, 0);
        Arc::make_mut(chunks.get_mut(&diagonal).unwrap())[diagonal_pos] = voxel;
        let neighbors = ChunkNeighbors::from(coord, &chunks);
        let mut images = Assets::<Image>::default();
        let (atlas, _) = build_voxel_atlas(&mut images).unwrap();
        let first = chunk.build_terrain_mesh(&neighbors, &atlas);
        let second = chunk.build_terrain_mesh(&neighbors, &atlas);
        let Some(VertexAttributeValues::Float32x3(positions)) = first.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("missing positions")
        };
        assert!(!positions.is_empty());
        assert_eq!(first.attribute(Mesh::ATTRIBUTE_POSITION), second.attribute(Mesh::ATTRIBUTE_POSITION));

        Arc::make_mut(chunks.get_mut(&diagonal).unwrap())[diagonal_pos] = VoxelType::AIR;
        let without_diagonal = chunk.build_terrain_mesh(&ChunkNeighbors::from(coord, &chunks), &atlas);
        assert_ne!(first.attribute(Mesh::ATTRIBUTE_POSITION), without_diagonal.attribute(Mesh::ATTRIBUTE_POSITION));

        Arc::make_mut(chunks.get_mut(&diagonal).unwrap())[diagonal_pos] = VoxelType::from_block(BlockType::Log);
        let diagonal_block = chunk.build_terrain_mesh(&ChunkNeighbors::from(coord, &chunks), &atlas);
        assert_eq!(diagonal_block.attribute(Mesh::ATTRIBUTE_POSITION), without_diagonal.attribute(Mesh::ATTRIBUTE_POSITION));
    }

    #[test]
    fn blocks_do_not_affect_terrain_meshes() {
        let coord = ChunkCoord::new(0, 0);
        let mut chunk = ChunkData::new(coord, Entity::PLACEHOLDER);
        let mut chunks = bevy::platform::collections::HashMap::default();
        for coord in [ChunkCoord::new(1, 0), ChunkCoord::new(0, 1), ChunkCoord::new(1, 1)] {
            chunks.insert(coord, Arc::new(ChunkData::new(coord, Entity::PLACEHOLDER)));
        }
        let neighbors = ChunkNeighbors::from(coord, &chunks);
        let mut images = Assets::<Image>::default();
        let (atlas, _) = build_voxel_atlas(&mut images).unwrap();
        let terrain_pos = VoxelCoord::new(10, 10, 10);
        let block_pos = VoxelCoord::new(11, 10, 10);
        chunk[block_pos] = VoxelType::from_block(BlockType::Log);
        let blocks_only = chunk.build_terrain_mesh(&neighbors, &atlas);
        let Some(VertexAttributeValues::Float32x3(positions)) = blocks_only.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("missing positions")
        };
        assert!(positions.is_empty());

        chunk[terrain_pos] = VoxelType::from_terrain(TerrainType::Stone);
        let next_to_block = chunk.build_terrain_mesh(&neighbors, &atlas);
        let Some(VertexAttributeValues::Float32x3(positions)) = next_to_block.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("missing positions")
        };
        assert!(!positions.is_empty());
        chunk[block_pos] = VoxelType::AIR;
        let next_to_air = chunk.build_terrain_mesh(&neighbors, &atlas);
        for attribute in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_NORMAL, Mesh::ATTRIBUTE_UV_0] {
            assert_eq!(next_to_block.attribute(attribute), next_to_air.attribute(attribute));
        }
    }
}
