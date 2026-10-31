use super::Compass;

use crate::CHUNK_WIDTH;

use bevy::math::{Vec3, vec3};
use wincode::{SchemaRead, SchemaWrite};

/// Coordinate of a chunk in the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SchemaWrite, SchemaRead)]
pub struct ChunkCoord {
    pub x: i32,
    pub z: i32,
}

#[allow(dead_code)]
impl ChunkCoord {
    pub const ZERO: Self = Self { x: 0, z: 0 };

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

impl std::ops::Add<Compass> for ChunkCoord {
    type Output = Self;

    fn add(self, rhs: Compass) -> Self::Output {
        match rhs {
            Compass::Right => Self { x: self.x + 1, ..self },
            Compass::Left  => Self { x: self.x - 1, ..self },
            Compass::Front => Self { z: self.z + 1, ..self },
            Compass::Back  => Self { z: self.z - 1, ..self },
        }
    }
}

impl std::fmt::Display for ChunkCoord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.x, self.z)
    }
}
