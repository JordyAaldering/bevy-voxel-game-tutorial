use bevy::math::Vec3;

use super::Direction;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Compass {
    /// Increasing x
    Right,
    /// Decreasing x
    Left,
    /// Increasing z
    Front,
    /// Decreasing z
    Back,
}

#[allow(dead_code)]
impl Compass {
    pub const fn iter() -> [Self; 4] {
        [
            Self::Right,
            Self::Left,
            Self::Front,
            Self::Back,
        ]
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::Right  => Self::Left,
            Self::Left   => Self::Right,
            Self::Front  => Self::Back,
            Self::Back   => Self::Front,
        }
    }
}

impl Into<Direction> for Compass {
    fn into(self) -> Direction {
        match self {
            Self::Right => Direction::Right,
            Self::Left  => Direction::Left,
            Self::Front => Direction::Front,
            Self::Back  => Direction::Back,
        }
    }
}
