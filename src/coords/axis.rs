use bevy::math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub fn get(self, v: Vec3) -> f32 {
        match self {
            Self::X => v.x,
            Self::Y => v.y,
            Self::Z => v.z,
        }
    }

    pub fn set(self, v: &mut Vec3, value: f32) {
        match self {
            Self::X => v.x = value,
            Self::Y => v.y = value,
            Self::Z => v.z = value,
        }
    }
}
