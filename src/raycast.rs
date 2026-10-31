use crate::{Direction, VoxelWorld, WorldCoord};

use bevy::math::{Dir3, Vec3};

/// A raycast through the voxel world, returning the first solid voxel hit.
#[derive(Debug)]
pub struct VoxelHit {
    /// The world coordinate of the voxel that was hit.
    pub coord: WorldCoord,
    /// The face of the voxel that was hit.
    pub direction: Direction,
}

pub fn find_voxel(
    world: &VoxelWorld,
    origin: Vec3,
    look: Dir3,
    max_distance: f32,
) -> Option<VoxelHit> {
    let look = look.normalize();

    let mut x = origin.x.floor() as i32;
    let mut y = origin.y.floor() as i32;
    let mut z = origin.z.floor() as i32;

    let step_x = look.x.signum() as i32;
    let step_y = look.y.signum() as i32;
    let step_z = look.z.signum() as i32;

    let delta_x = if look.x != 0.0 {
        (1.0 / look.x).abs()
    } else {
        f32::INFINITY
    };

    let delta_y = if look.y != 0.0 {
        (1.0 / look.y).abs()
    } else {
        f32::INFINITY
    };

    let delta_z = if look.z != 0.0 {
        (1.0 / look.z).abs()
    } else {
        f32::INFINITY
    };

    let mut max_x = initial_axis_distance(origin.x, look.x);
    let mut max_y = initial_axis_distance(origin.y, look.y);
    let mut max_z = initial_axis_distance(origin.z, look.z);

    loop {
        let (distance, face) = if max_x <= max_y && max_x <= max_z {
            x += step_x;

            let distance = max_x;
            max_x += delta_x;

            let direction = if step_x > 0 {
                Direction::Left
            } else {
                Direction::Right
            };

            (distance, direction)
        } else if max_y <= max_z {
            y += step_y;

            let distance = max_y;
            max_y += delta_y;

            let direction = if step_y > 0 {
                Direction::Bottom
            } else {
                Direction::Top
            };

            (distance, direction)
        } else {
            z += step_z;

            let distance = max_z;
            max_z += delta_z;

            let direction = if step_z > 0 {
                Direction::Back
            } else {
                Direction::Front
            };

            (distance, direction)
        };

        if distance > max_distance {
            return None;
        }

        let coord = WorldCoord::new(x, y, z);
        if world.is_solid(coord) {
            return Some(VoxelHit {
                coord,
                direction: face,
            });
        }
    }
}

/// Returns the distance until the ray crosses the next voxel boundary on this axis.
/// This is needed to determine which neighboring voxel to visit next.
fn initial_axis_distance(origin: f32, direction: f32) -> f32 {
    if direction > 0.0 {
        (origin.ceil() - origin) / direction
    } else if direction < 0.0 {
        (origin.floor() - origin) / direction
    } else {
        f32::INFINITY
    }
}
