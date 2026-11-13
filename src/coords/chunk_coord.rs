use crate::CHUNK_WIDTH;

use bevy::math::{CompassQuadrant, Vec3, vec3};

/// Coordinate of a chunk in the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(wincode::SchemaWrite, wincode::SchemaRead)]
pub struct ChunkCoord {
    pub x: i32,
    pub z: i32,
}

impl ChunkCoord {
    // pub const ZERO: Self = Self { x: 0, z: 0 };

    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    // pub const fn from_position(x: f32, z: f32) -> Self {
    //     Self {
    //         x: (x.floor() as i32).div_euclid(CHUNK_WIDTH as i32),
    //         z: (z.floor() as i32).div_euclid(CHUNK_WIDTH as i32),
    //     }
    // }

    /// Returns the position of this chunk in world space.
    pub const fn position(self) -> Vec3 {
        vec3(self.x as f32 * CHUNK_WIDTH as f32, 0.0, self.z as f32 * CHUNK_WIDTH as f32)
    }
}

impl std::ops::Add<CompassQuadrant> for ChunkCoord {
    type Output = Self;

    fn add(self, rhs: CompassQuadrant) -> Self::Output {
        match rhs {
            CompassQuadrant::East => Self { x: self.x + 1, ..self },
            CompassQuadrant::West => Self { x: self.x - 1, ..self },
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
