use super::ChunkData;

use bevy::{prelude::*, tasks::Task};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(wincode::SchemaWrite, wincode::SchemaRead)]
pub enum ChunkGenerationPhase {
    HeightGeneration,
    MeshGeneration,
    Done,
}

impl ChunkGenerationPhase {
    pub fn next(self) -> Self {
        match self {
            Self::HeightGeneration => Self::MeshGeneration,
            Self::MeshGeneration  => Self::Done,
            Self::Done => Self::Done,
        }
    }

    /// Which neighbors must have completed the previous phase before this phase can run.
    pub fn neighbor_requirement(self) -> NeighborRequirement {
        match self {
            Self::HeightGeneration => NeighborRequirement::NONE,
            // Only the chunks touching this one are needed to cull faces
            Self::MeshGeneration => NeighborRequirement { radius: 1, diagonal: false },
            Self::Done => NeighborRequirement::NONE,
        }
    }
}

/// The set of neighboring chunks that must be ready before a generation phase can run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NeighborRequirement {
    /// How many chunks away to check, 0 meaning only the chunk itself.
    radius: i32,
    /// Whether to also check chunks that are not in the same row or column.
    diagonal: bool,
}

impl NeighborRequirement {
    const NONE: Self = Self { radius: 0, diagonal: false };

    pub fn offsets(self) -> impl Iterator<Item = (i32, i32)> {
        let r = self.radius;
        (-r..=r)
            .flat_map(move |dz| (-r..=r).map(move |dx| (dx, dz)))
            .filter(move |&(dx, dz)| (dx, dz) != (0, 0) && (self.diagonal || dx == 0 || dz == 0))
    }
}

/// Marker component for chunks that need to be checked for generation progress.
/// This is added when the chunk's voxel data changes, and removed when a generation task is started.
/// It is also added when the neighboring chunks progress to the next phase, meaning that this chunk
/// may now be able to progress to the next phase as well.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct NeedsGenerationCheck;

/// A chunk generation phase running on another thread. Dropping it cancels the task.
#[derive(Component)]
pub struct ChunkGenerationTask(pub Task<ChunkGenerationResult>);

pub enum ChunkGenerationResult {
    Height(Box<ChunkData>),
    Mesh(Mesh),
}

/// Event triggered when the voxel data of a chunk has changed, requiring a remesh.
#[derive(EntityEvent)]
pub struct ChunkDataChanged {
    pub entity: Entity,
}
