use crate::{CHUNK_HEIGHT, CHUNK_WIDTH, direction::Direction};

use bevy::math::{CompassQuadrant, Vec3, vec3};
use wincode::{SchemaRead, SchemaWrite};

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

/// Coordinate of a chunk in the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SchemaWrite, SchemaRead)]
pub struct ChunkCoord {
    pub x: i32,
    pub z: i32,
}

#[allow(dead_code)]
impl ChunkCoord {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub const fn from_position(x: f32, z: f32) -> Self {
        Self {
            x: (x.floor() as i32).div_euclid(CHUNK_WIDTH as i32),
            z: (z.floor() as i32).div_euclid(CHUNK_WIDTH as i32),
        }
    }

    /// Returns the position of this chunk in world space.
    pub const fn position(self) -> Vec3 {
        vec3(self.x as f32 * CHUNK_WIDTH as f32, 0.0, self.z as f32 * CHUNK_WIDTH as f32)
    }
}

impl std::ops::Add<CompassQuadrant> for ChunkCoord {
    type Output = Self;

    fn add(self, rhs: CompassQuadrant) -> Self::Output {
        match rhs {
            CompassQuadrant::East  => Self { x: self.x + 1, ..self },
            CompassQuadrant::West  => Self { x: self.x - 1, ..self },
            CompassQuadrant::North => Self { z: self.z + 1, ..self },
            CompassQuadrant::South => Self { z: self.z - 1, ..self },
        }
    }
}

impl std::fmt::Display for ChunkCoord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.x, self.z)
    }
}
