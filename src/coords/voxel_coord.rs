use super::Direction;

use crate::{CHUNK_HEIGHT, CHUNK_WIDTH};

use bevy::math::{Vec3, vec3};

/// Chunk-local voxel coordinate.
///
/// x ∈ [0, CHUNK_WIDTH)
/// y ∈ [0, CHUNK_HEIGHT)
/// z ∈ [0, CHUNK_WIDTH)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoxelCoord {
    pub x: usize,
    pub y: usize,
    pub z: usize,
}

#[allow(dead_code)]
impl VoxelCoord {
    pub const fn new(x: usize, y: usize, z: usize) -> Self {
        debug_assert!(x < CHUNK_WIDTH);
        debug_assert!(y < CHUNK_HEIGHT);
        debug_assert!(z < CHUNK_WIDTH);
        Self { x, y, z }
    }

    pub const fn in_bounds(self) -> bool {
        self.x < CHUNK_WIDTH && self.y < CHUNK_HEIGHT && self.z < CHUNK_WIDTH
    }

    /// The flattened index of the voxel in the chunk's voxel 1D array.
    pub const fn index(self) -> usize {
        self.x + CHUNK_WIDTH * (self.y + CHUNK_HEIGHT * self.z)
    }

    /// Returns the local position of this voxel within its chunk.
    pub const fn local_position(self) -> Vec3 {
        vec3(self.x as f32, self.y as f32, self.z as f32)
    }
}

impl std::ops::Add<Direction> for VoxelCoord {
    type Output = Self;

    /// Offset the voxel coordinate by one in the given direction.
    /// This might result in voxel coordinates that are outside the bounds.
    fn add(self, rhs: Direction) -> Self::Output {
        // We deliberately use `wrapping_sub` to make the value wrap around to an out of bounds value.
        match rhs {
            Direction::Right  => Self { x: self.x + 1, ..self },
            Direction::Left   => Self { x: self.x.wrapping_sub(1), ..self },
            Direction::Top    => Self { y: self.y + 1, ..self },
            Direction::Bottom => Self { y: self.y.wrapping_sub(1), ..self },
            Direction::Front  => Self { z: self.z + 1, ..self },
            Direction::Back   => Self { z: self.z.wrapping_sub(1), ..self },
        }
    }
}

impl std::fmt::Display for VoxelCoord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}
