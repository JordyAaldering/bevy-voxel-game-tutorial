use super::Compass;

use bevy::math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum Direction {
    /// Increasing x
    Right,
    /// Decreasing x
    Left,
    /// Increasing y
    Top,
    /// Decreasing y
    Bottom,
    /// Increasing z
    Front,
    /// Decreasing z
    Back,
}

#[allow(dead_code)]
impl Direction {
    pub const fn iter() -> [Self; 6] {
        [
            Self::Right,
            Self::Left,
            Self::Top,
            Self::Bottom,
            Self::Front,
            Self::Back,
        ]
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::Right  => Self::Left,
            Self::Left   => Self::Right,
            Self::Top    => Self::Bottom,
            Self::Bottom => Self::Top,
            Self::Front  => Self::Back,
            Self::Back   => Self::Front,
        }
    }

    pub const fn offset(self) -> (i32, i32, i32) {
        match self {
            Self::Right  => ( 1,  0,  0),
            Self::Left   => (-1,  0,  0),
            Self::Top    => ( 0,  1,  0),
            Self::Bottom => ( 0, -1,  0),
            Self::Front  => ( 0,  0,  1),
            Self::Back   => ( 0,  0, -1),
        }
    }

    /// Returns the outward-facing normal vector for this direction.
    pub const fn vec3(self) -> Vec3 {
        match self {
            Self::Right  => Vec3::X,
            Self::Left   => Vec3::NEG_X,
            Self::Top    => Vec3::Y,
            Self::Bottom => Vec3::NEG_Y,
            Self::Front  => Vec3::Z,
            Self::Back   => Vec3::NEG_Z,
        }
    }
}

impl TryInto<Compass> for Direction {
    type Error = ();

    fn try_into(self) -> Result<Compass, Self::Error> {
        match self {
            Self::Right => Ok(Compass::Right),
            Self::Left  => Ok(Compass::Left),
            Self::Front => Ok(Compass::Front),
            Self::Back  => Ok(Compass::Back),
            _ => Err(())
        }
    }
}
