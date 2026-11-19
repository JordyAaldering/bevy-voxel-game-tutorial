use crate::*;

use bevy::{prelude::*, tasks::Task};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(wincode::SchemaWrite, wincode::SchemaRead)]
pub enum ChunkGenerationPhase {
    HeightGeneration,
    #[cfg(since17)]
    OreGeneration,
    #[cfg(since17)]
    CaveGeneration,
    #[cfg(since16)]
    GenerateStructures,
    MeshGeneration,
    Done,
}

impl ChunkGenerationPhase {
    #[cfg(until15)]
    pub const fn next(self) -> Self {
        match self {
            Self::HeightGeneration => Self::MeshGeneration,
            Self::MeshGeneration  => Self::Done,
            Self::Done => Self::Done,
        }
    }

    #[cfg(only16)]
    pub const fn next(self) -> Self {
        match self {
            Self::HeightGeneration => Self::GenerateStructures,
            Self::GenerateStructures => Self::MeshGeneration,
            Self::MeshGeneration  => Self::Done,
            Self::Done => Self::Done,
        }
    }

    #[cfg(since17)]
    pub const fn next(self) -> Self {
        match self {
            Self::HeightGeneration => Self::OreGeneration,
            Self::OreGeneration => Self::CaveGeneration,
            Self::CaveGeneration => Self::GenerateStructures,
            Self::GenerateStructures => Self::MeshGeneration,
            Self::MeshGeneration  => Self::Done,
            Self::Done => Self::Done,
        }
    }

    /// Which neighbors must have completed the previous phase before this phase can run.
    pub const fn neighbor_requirement(self) -> NeighborRequirement {
        match self {
            Self::HeightGeneration => NeighborRequirement::NONE,
            #[cfg(since17)] Self::OreGeneration => NeighborRequirement::NONE,
            #[cfg(since17)] Self::CaveGeneration => NeighborRequirement::NONE,
            #[cfg(since16)] Self::GenerateStructures => NeighborRequirement::SQUARE_1,
            Self::MeshGeneration => NeighborRequirement::SQUARE_1,
            Self::Done => NeighborRequirement::NONE,
        }
    }

    pub const fn color(self) -> Color {
        match self {
            Self::HeightGeneration => Color::hsl(213.0, 0.94, 0.68),
            #[cfg(since17)] Self::OreGeneration => Color::hsl(43.0, 0.96, 0.56),
            #[cfg(since17)] Self::CaveGeneration => Color::hsl(255.0, 0.92, 0.76),
            #[cfg(since16)] Self::GenerateStructures => Color::hsl(329.0, 0.86, 0.70),
            Self::MeshGeneration => Color::hsl(158.0, 0.64, 0.52),
            Self::Done => Color::hsl(0.0, 0.0, 1.0),
        }
    }

    /// The combined neighbor padding needed for every phase to complete.
    #[cfg(since13)]
    pub fn total_padding() -> i32 {
        let mut padding = 0;
        let mut phase = Self::HeightGeneration;
        while phase != Self::Done {
            padding += phase.neighbor_requirement().radius;
            phase = phase.next();
        }
        padding
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

    const SQUARE_1: Self = Self { radius: 1, diagonal: true };

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

#[cfg(until15)]
pub enum ChunkGenerationResult {
    Data(Box<ChunkData>),
    Mesh(Mesh),
}

#[cfg(since16)]
pub enum ChunkGenerationResult {
    Data(ChunkGenerationPhase, Box<ChunkData>),
    Structures(Vec<WorldCoord>),
    #[cfg(until18)]
    Mesh(Mesh),
    #[cfg(since19)]
    Mesh {
        voxels: Mesh,
        terrain: Mesh,
    },
    #[cfg(since18)]
    Error(String),
}

/// Event triggered when the voxel data of a chunk has changed, requiring a remesh.
#[derive(EntityEvent)]
pub struct ChunkDataChanged {
    pub entity: Entity,
}
