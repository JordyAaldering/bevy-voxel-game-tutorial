//! This module contains the various coordinate types used in the voxel world
//! and some accompanying utilities for operating on axes and directions.

mod direction;
#[cfg(since04)]
mod chunk_coord;
#[cfg(since04)]
mod voxel_coord;
#[cfg(since07)]
mod world_coord;

pub use direction::Direction;
#[cfg(since04)]
pub use chunk_coord::ChunkCoord;
#[cfg(since04)]
pub use voxel_coord::VoxelCoord;
#[cfg(since07)]
pub use world_coord::WorldCoord;
