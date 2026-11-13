mod axis;

use axis::Axis;

use crate::{Player, VoxelWorld, WorldCoord};

use std::marker::PhantomData;

use bevy::prelude::*;

const PLAYER_WIDTH: f32 = 0.6;
const PLAYER_WIDTH_HALF: f32 = PLAYER_WIDTH * 0.5;
const PLAYER_HEIGHT: f32 = 1.8;

/// Prevents exact boundary contact from being treated as an overlap with the adjacent voxel.
const COLLISION_EPSILON: f32 = 1e-3;

const GRAVITY: f32 = 20.0;

pub struct PhysicsPlugin<F, M> {
    run_condition: F,
    _marker: PhantomData<fn() -> M>,
}

impl<F, M> PhysicsPlugin<F, M>
where
    F: SystemCondition<M>,
{
    pub fn new(run_condition: F) -> Self {
        Self {
            run_condition,
            _marker: PhantomData,
        }
    }
}

impl<F, M> Plugin for PhysicsPlugin<F, M>
where
    F: Clone + SystemCondition<M> + Send + Sync + 'static,
    M: 'static
{
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, (
            player_physics
                .run_if(self.run_condition.clone()),
        ));
    }
}

fn player_physics(
    time: Res<Time>,
    world: Res<VoxelWorld>,
    player: Single<(&mut Player, &mut Transform)>,
) {
    let (mut player, mut transform) = player.into_inner();
    let dt = time.delta_secs();

    player.velocity.y -= GRAVITY * dt;
    player.grounded = false;

    for axis in [Axis::X, Axis::Y, Axis::Z] {
        move_axis(
            &mut player,
            &mut transform.translation,
            &world,
            dt,
            axis,
        );

        #[cfg(since10)]
        collide_with_terrain(
            &mut player,
            &mut transform.translation,
            &world,
            dt,
            axis,
        );
    }
}

fn move_axis(
    player: &mut Player,
    position: &mut Vec3,
    world: &VoxelWorld,
    dt: f32,
    axis: Axis,
) {
    let velocity = axis.get(&player.velocity);

    if velocity == 0.0 {
        return;
    }

    axis.upd(position, |pos| pos + velocity * dt);

    let min = Vec3::new(
        position.x - PLAYER_WIDTH_HALF,
        position.y,
        position.z - PLAYER_WIDTH_HALF,
    );
    let max = Vec3::new(
        position.x + PLAYER_WIDTH_HALF - COLLISION_EPSILON,
        position.y + PLAYER_HEIGHT - COLLISION_EPSILON,
        position.z + PLAYER_WIDTH_HALF - COLLISION_EPSILON,
    );

    let min_x = min.x.floor() as i32;
    let max_x = max.x.floor() as i32;
    let min_y = min.y.floor() as i32;
    let max_y = max.y.floor() as i32;
    let min_z = min.z.floor() as i32;
    let max_z = max.z.floor() as i32;

    // Find the voxel coordinate of the leading face
    //
    // The positive-direction edge is an exclusive maximum, so the epsilon
    // keeps exact boundary contact from selecting the voxel beyond it.
    // Negative-direction edges are intentionally unadjusted; subtracting the
    // epsilon there would move the lookup into the voxel behind the face.
    let edge = match (axis, velocity > 0.0) {
        (Axis::X, true)  => position.x + PLAYER_WIDTH_HALF - COLLISION_EPSILON,
        (Axis::X, false) => position.x - PLAYER_WIDTH_HALF,
        (Axis::Y, true)  => position.y + PLAYER_HEIGHT - COLLISION_EPSILON,
        (Axis::Y, false) => position.y,
        (Axis::Z, true)  => position.z + PLAYER_WIDTH_HALF - COLLISION_EPSILON,
        (Axis::Z, false) => position.z - PLAYER_WIDTH_HALF,
    };

    let edge = edge.floor() as i32;

    let (x_range, y_range, z_range) = match axis {
        Axis::X => (edge..=edge,   min_y..=max_y, min_z..=max_z),
        Axis::Y => (min_x..=max_x, edge..=edge,   min_z..=max_z),
        Axis::Z => (min_x..=max_x, min_y..=max_y, edge..=edge),
    };

    for z in z_range {
        for y in y_range.clone() {
            for x in x_range.clone() {
                if !blocks_movement(world, WorldCoord::new(x, y, z)) {
                    continue;
                }

                let resolved = match (axis, velocity > 0.0) {
                    (Axis::X, true)  => x as f32 - PLAYER_WIDTH_HALF,
                    (Axis::X, false) => x as f32 + 1.0 + PLAYER_WIDTH_HALF,
                    (Axis::Y, true)  => y as f32 - PLAYER_HEIGHT,
                    (Axis::Y, false) => {
                        player.grounded = true;
                        y as f32 + 1.0
                    }
                    (Axis::Z, true)  => z as f32 - PLAYER_WIDTH_HALF,
                    (Axis::Z, false) => z as f32 + 1.0 + PLAYER_WIDTH_HALF,
                };

                Axis::set(axis, position, resolved);
                Axis::set(axis, &mut player.velocity, 0.0);
                return;
            }
        }
    }
}

#[cfg(until09)]
fn blocks_movement(world: &VoxelWorld, pos: WorldCoord) -> bool {
    world.is_solid(pos)
}

#[cfg(since10)]
fn blocks_movement(world: &VoxelWorld, pos: WorldCoord) -> bool {
    world.get_voxel(pos).is_none_or(VoxelType::is_block)
}

#[cfg(since10)]
fn collide_with_terrain(
    player: &mut Player,
    position: &mut Vec3,
    world: &VoxelWorld,
    dt: f32,
    axis: Axis,
) {
    let velocity = axis.get(&player.velocity);
    if velocity == 0.0 {
        return;
    }

    let min = Vec3::new(
        position.x - PLAYER_WIDTH_HALF,
        position.y,
        position.z - PLAYER_WIDTH_HALF,
    );
    let max = Vec3::new(
        position.x + PLAYER_WIDTH_HALF - COLLISION_EPSILON,
        position.y + PLAYER_HEIGHT - COLLISION_EPSILON,
        position.z + PLAYER_WIDTH_HALF - COLLISION_EPSILON,
    );
    let current_edge = match (axis, velocity > 0.0) {
        (Axis::X, true) => position.x + PLAYER_WIDTH_HALF,
        (Axis::X, false) => position.x - PLAYER_WIDTH_HALF,
        (Axis::Y, true) => position.y + PLAYER_HEIGHT,
        (Axis::Y, false) => position.y,
        (Axis::Z, true) => position.z + PLAYER_WIDTH_HALF,
        (Axis::Z, false) => position.z - PLAYER_WIDTH_HALF,
    };
    let previous_edge = current_edge - velocity * dt;
    let center_x = (min.x + max.x) * 0.5;
    let center_z = (min.z + max.z) * 0.5;
    let center_y = (min.y + max.y) * 0.5;
    let cross_samples = match axis {
        Axis::X => [
            vec3(center_y, min.z, 0.0),
            vec3(center_y, max.z, 0.0),
            vec3(max.y, min.z, 0.0),
            vec3(max.y, max.z, 0.0),
        ],
        Axis::Y => [vec3(center_x, center_z, 0.0); 4],
        Axis::Z => [
            vec3(min.x, center_y, 0.0),
            vec3(max.x, center_y, 0.0),
            vec3(min.x, max.y, 0.0),
            vec3(max.x, max.y, 0.0),
        ],
    };

    let surface = cross_samples.iter().copied()
        .filter_map(|sample| terrain_crossing(world, axis, sample, previous_edge, current_edge, velocity))
        .min_by(|a, b| {
            if velocity > 0.0 {
                a.total_cmp(b)
            } else {
                b.total_cmp(a)
            }
        });

    let Some(surface) = surface else {
        return;
    };

    match axis {
        Axis::X => position.x = surface + if velocity > 0.0 { -PLAYER_WIDTH_HALF } else { PLAYER_WIDTH_HALF },
        Axis::Y => {
            position.y = surface + if velocity > 0.0 { -PLAYER_HEIGHT } else { 0.0 };
            if velocity < 0.0 {
                player.grounded = true;
            }
        }
        Axis::Z => position.z = surface + if velocity > 0.0 { -PLAYER_WIDTH_HALF } else { PLAYER_WIDTH_HALF },
    }
    Axis::set(axis, &mut player.velocity, 0.0);
}

#[cfg(since10)]
fn terrain_crossing(
    world: &VoxelWorld,
    axis: Axis,
    sample: Vec3,
    start: f32,
    end: f32,
    velocity: f32,
) -> Option<f32> {
    let previous = sample_terrain_at_axis(world, axis, start, sample)?;
    if previous <= 0.5 {
        return find_terrain_crossing(world, axis, sample, start, end, false);
    }

    let current = sample_terrain_at_axis(world, axis, end, sample)?;
    if current < 0.5 {
        return None;
    }

    let direction = if velocity < 0.0 { 1.0 } else { -1.0 };
    let max_distance = match axis {
        Axis::X | Axis::Z => (PLAYER_WIDTH + 1.0).ceil(),
        Axis::Y => (PLAYER_HEIGHT + 1.0).ceil(),
    };

    find_terrain_crossing(world, axis, sample, end, end + direction * max_distance, true)
}

#[cfg(since10)]
fn find_terrain_crossing(
    world: &VoxelWorld,
    axis: Axis,
    sample: Vec3,
    start: f32,
    end: f32,
    leaving_solid: bool,
) -> Option<f32> {
    let increasing = end > start;

    let mut boundary = if increasing {
        (start - 0.5).floor() as i32 + 1
    } else {
        (start - 0.5).ceil() as i32 - 1
    };

    let mut position = start;
    let mut density = sample_terrain_at_axis(world, axis, position, sample)?;

    loop {
        let next_boundary = boundary as f32 + 0.5;
        let next = if increasing {
            next_boundary.min(end)
        } else {
            next_boundary.max(end)
        };

        let next_density = sample_terrain_at_axis(world, axis, next, sample)?;
        let crosses = if leaving_solid {
            density >= 0.5 && next_density < 0.5
        } else {
            density <= 0.5 && next_density >= 0.5
        };

        if crosses && density != next_density {
            let fraction = (0.5 - density) / (next_density - density);
            return Some(position + (next - position) * fraction);
        }

        if next == end {
            return None;
        }

        position = next;
        density = next_density;
        boundary += if increasing { 1 } else { -1 };
    }
}

#[cfg(since10)]
fn sample_terrain_at_axis(world: &VoxelWorld, axis: Axis, axis_position: f32, sample: Vec3) -> Option<f32> {
    let position = match axis {
        Axis::X => vec3(axis_position, sample.x, sample.y),
        Axis::Y => vec3(sample.x, axis_position, sample.y),
        Axis::Z => vec3(sample.x, sample.y, axis_position),
    };
    sample_terrain(world, position)
}

#[cfg(since10)]
fn sample_terrain(world: &VoxelWorld, position: Vec3) -> Option<f32> {
    let sample_position = position - Vec3::splat(0.5);
    let base = sample_position.floor();
    let fraction = sample_position - base;
    let x = base.x as i32;
    let y = base.y as i32;
    let z = base.z as i32;

    let density = |x: i32, y: i32, z: i32| {
        let pos = WorldCoord::new(x, y, z);
        let voxel = world.get_voxel(pos)?;
        if !voxel.is_terrain() {
            return Some(0.0);
        }

        let density = world.get_density(pos)?;
        let density: f32 = density.into();
        Some(density)
    };

    let densities = [
        density(x, y, z)?,
        density(x + 1, y, z)?,
        density(x, y + 1, z)?,
        density(x + 1, y + 1, z)?,
        density(x, y, z + 1)?,
        density(x + 1, y, z + 1)?,
        density(x, y + 1, z + 1)?,
        density(x + 1, y + 1, z + 1)?,
    ];

    let x00 = lerp(densities[0], densities[1], fraction.x);
    let x10 = lerp(densities[2], densities[3], fraction.x);
    let x01 = lerp(densities[4], densities[5], fraction.x);
    let x11 = lerp(densities[6], densities[7], fraction.x);
    let y0 = lerp(x00, x10, fraction.y);
    let y1 = lerp(x01, x11, fraction.y);
    Some(lerp(y0, y1, fraction.z))
}

#[cfg(since10)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
