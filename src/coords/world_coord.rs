use super::{Direction, ChunkCoord, VoxelCoord};

use crate::CHUNK_WIDTH;

use bevy::math::{Vec3, vec3};

/// Coordinate of a voxel in global world space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(from = "[i32; 3]", into = "[i32; 3]")]
pub struct WorldCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[allow(dead_code)]
impl WorldCoord {
    pub const ZERO: Self = Self { x: 0, y: 0, z: 0 };

    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Returns the local position of the voxel within its chunk.
    pub const fn local_coord(self) -> VoxelCoord {
        VoxelCoord {
            x: self.x.rem_euclid(CHUNK_WIDTH as i32) as usize,
            y: self.y as usize,
            z: self.z.rem_euclid(CHUNK_WIDTH as i32) as usize,
        }
    }

    /// Returns the coordinate of the chunk that contains this voxel.
    pub const fn chunk_coord(self) -> ChunkCoord {
        ChunkCoord {
            x: self.x.div_euclid(CHUNK_WIDTH as i32),
            z: self.z.div_euclid(CHUNK_WIDTH as i32),
        }
    }

    /// Returns the position of this voxel in world space.
    pub const fn position(self) -> Vec3 {
        vec3(self.x as f32, self.y as f32, self.z as f32)
    }

    pub const fn shift(self, direction: Direction) -> Self {
        let (dx, dy, dz) = direction.offset();
        Self {
            x: self.x + dx,
            y: self.y + dy,
            z: self.z + dz,
        }
    }

    pub fn min(self, other: Self) -> Self {
        Self {
            x: self.x.min(other.x),
            y: self.y.min(other.y),
            z: self.z.min(other.z),
        }
    }

    pub fn max(self, other: Self) -> Self {
        Self {
            x: self.x.max(other.x),
            y: self.y.max(other.y),
            z: self.z.max(other.z),
        }
    }
}

impl From<[i32; 3]> for WorldCoord {
    fn from([x, y, z]: [i32; 3]) -> Self {
        Self { x, y, z }
    }
}

impl From<WorldCoord> for [i32; 3] {
    fn from(coord: WorldCoord) -> Self {
        [coord.x, coord.y, coord.z]
    }
}

impl std::ops::Add for WorldCoord {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl std::fmt::Display for WorldCoord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}
