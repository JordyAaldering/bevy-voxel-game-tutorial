mod axis;

use axis::Axis;

use crate::{Player, VoxelWorld, WorldCoord};

use better_default::Default;
use bevy::prelude::*;
use bevy_inspector_egui::prelude::*;


const PLAYER_WIDTH: f32 = 0.6;
const PLAYER_WIDTH_HALF: f32 = PLAYER_WIDTH * 0.5;
const PLAYER_HEIGHT: f32 = 1.8;

/// Prevents exact boundary contact from being treated as an overlap with the adjacent voxel.
const COLLISION_EPSILON: f32 = 1e-3;

const GRAVITY: f32 = 20.0;

#[derive(Default)]
pub struct PhysicsPlugin {
    #[default(15.0)]
    pub move_speed: f32,
    #[cfg(since07)]
    #[default(7.0)]
    pub jump_strenght: f32,
}

#[derive(Default, Resource)]
pub struct PhysicsInput {
    pub movement: Vec2,
    pub jump: bool,
    pub crouch: bool,
}

#[derive(Default, Resource)]
struct PhysicsState {
    velocity: Vec3,
    grounded: bool,
}

#[derive(Resource, Reflect, InspectorOptions)]
#[reflect(Resource, InspectorOptions)]
struct PhysicsConfig {
    #[inspector(min = 0.1, max = 20.0)]
    move_speed: f32,
    #[cfg(since07)]
    #[inspector(min = 0.1, max = 20.0)]
    jump_strenght: f32,
}

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<PhysicsInput>()
            .init_resource::<PhysicsState>()
            .insert_resource(PhysicsConfig {
                move_speed: self.move_speed,
                jump_strenght: self.jump_strenght,
            })
            .add_systems(FixedUpdate, (
                move_spectator.run_if(super::is_gamemode_spectator),
                move_survival.run_if(super::is_gamemode_survival),
            ));
    }
}

fn move_spectator(
    time: Res<Time>,
    input: Res<PhysicsInput>,
    settings: Res<PhysicsConfig>,
    mut player: Single<&mut Transform, With<Player>>,
) {
    let x = input.movement.x;
    let y = f32::from(input.jump) - f32::from(input.crouch);
    let z = input.movement.y;

    let direction = x * player.right() + y * Vec3::Y + z * player.forward();
    player.translation += settings.move_speed * time.delta_secs() * direction.normalize_or_zero();
}

fn move_survival(
    time: Res<Time>,
    input: Res<PhysicsInput>,
    settings: Res<PhysicsConfig>,
    mut state: ResMut<PhysicsState>,
    mut player: Single<&mut Transform, With<Player>>,
    world: Res<VoxelWorld>,
) {
    let dt = time.delta_secs();

    let x = input.movement.x;
    let z = input.movement.y;

    if x != 0.0 || z != 0.0 {
        let movement = x * player.right() + z * player.forward();
        let movement = movement.normalize_or_zero();
        state.velocity.x = settings.move_speed * movement.x;
        state.velocity.z = settings.move_speed * movement.z;
    } else {
        state.velocity.x = 0.0;
        state.velocity.z = 0.0;
    }

    state.velocity.y -= GRAVITY * dt;

    if state.grounded && input.jump {
        state.velocity.y = settings.jump_strenght;
    }

    state.grounded = false;

    for axis in [Axis::X, Axis::Y, Axis::Z] {
        #[cfg(since19)]
        let (previous, velocity) = (player.translation, axis.get(&state.velocity));
        move_axis(
            &mut state,
            &mut player.translation,
            &world,
            dt,
            axis,
        );

        #[cfg(since19)]
        collide_with_terrain(
            &mut state,
            &mut player.translation,
            &world,
            previous,
            velocity,
            axis,
        );
    }
}

/// Moves the player along one axis and resolves collisions with blocking voxels.
fn move_axis(
    state: &mut PhysicsState,
    position: &mut Vec3,
    world: &VoxelWorld,
    dt: f32,
    axis: Axis,
) {
    let velocity = axis.get(&state.velocity);

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
                        state.grounded = true;
                        y as f32 + 1.0
                    }
                    (Axis::Z, true)  => z as f32 - PLAYER_WIDTH_HALF,
                    (Axis::Z, false) => z as f32 + 1.0 + PLAYER_WIDTH_HALF,
                };

                Axis::set(axis, position, resolved);
                Axis::set(axis, &mut state.velocity, 0.0);
                return;
            }
        }
    }
}

/// Treats solid voxels as movement obstacles.
#[cfg(until18)]
fn blocks_movement(world: &VoxelWorld, pos: WorldCoord) -> bool {
    world.is_solid(pos)
}

/// Treats blocks and unavailable voxels as obstacles; terrain is resolved separately.
#[cfg(since19)]
fn blocks_movement(world: &VoxelWorld, pos: WorldCoord) -> bool {
    world.get_voxel(pos).is_none_or(|v| v.is_block())
}

/// Resolves an axis movement against sampled terrain triangles, including slope overlaps.
/// Stops the corresponding velocity and marks the player grounded on a downward collision.
#[cfg(since19)]
fn collide_with_terrain(
    state: &mut PhysicsState,
    position: &mut Vec3,
    world: &VoxelWorld,
    previous: Vec3,
    velocity: f32,
    axis: Axis,
) {
    if velocity == 0.0 {
        return;
    }

    let min = vec3(position.x - PLAYER_WIDTH_HALF, position.y, position.z - PLAYER_WIDTH_HALF);
    let max = vec3(
        position.x + PLAYER_WIDTH_HALF - COLLISION_EPSILON,
        position.y + PLAYER_HEIGHT - COLLISION_EPSILON,
        position.z + PLAYER_WIDTH_HALF - COLLISION_EPSILON,
    );
    let edge_offset = match (axis, velocity > 0.0) {
        (Axis::X | Axis::Z, true) => PLAYER_WIDTH_HALF,
        (Axis::X | Axis::Z, false) => -PLAYER_WIDTH_HALF,
        (Axis::Y, true) => PLAYER_HEIGHT,
        (Axis::Y, false) => 0.0,
    };
    let start = axis.get(&previous) + edge_offset;
    let end = axis.get(position) + edge_offset;
    let center = *position + Vec3::Y * (PLAYER_HEIGHT * 0.5);
    let samples = match axis {
        Axis::X => [vec2(center.y, min.z), vec2(center.y, max.z), vec2(max.y, min.z), vec2(max.y, max.z)],
        Axis::Y => [vec2(center.x, center.z); 4],
        Axis::Z => [vec2(min.x, center.y), vec2(max.x, center.y), vec2(min.x, max.y), vec2(max.x, max.y)],
    };

    let sample_count = if axis == Axis::Y { 1 } else { samples.len() };
    let surface = samples.into_iter().take(sample_count).filter_map(|sample| {
        let forward = terrain_ray(world, axis, sample, start, end);
        if let Some((point, facing)) = forward {
            if facing < 0.0 {
                return Some(point);
            }
        }

        // Horizontal movement can move the feet into a slope. Resolve that overlap
        // toward the surface, just as when the player starts inside terrain.
        let distance = match axis {
            Axis::X | Axis::Z => (PLAYER_WIDTH + 1.0).ceil(),
            Axis::Y => (PLAYER_HEIGHT + 1.0).ceil(),
        };
        let reverse_end = end - velocity.signum() * distance;
        terrain_ray(world, axis, sample, end, reverse_end)
            .filter(|(_, facing)| *facing > 0.0)
            .map(|(point, _)| point)
    }).min_by(|a, b| {
        if velocity > 0.0 { a.total_cmp(b) } else { b.total_cmp(a) }
    });

    if let Some(surface) = surface {
        axis.set(position, surface - edge_offset);
        axis.set(&mut state.velocity, 0.0);
        if axis == Axis::Y && velocity < 0.0 {
            state.grounded = true;
        }
    }
}

/// Returns the first terrain crossing along an axis-aligned segment.
/// The result contains its axis coordinate and the dot product of its normal with the ray direction.
/// Cells with unavailable voxel samples are skipped.
#[cfg(since19)]
fn terrain_ray(world: &VoxelWorld, axis: Axis, sample: Vec2, start: f32, end: f32) -> Option<(f32, f32)> {
    use crate::{marching, CHUNK_HEIGHT, CHUNK_WIDTH};

    if start == end {
        return None;
    }
    let ray = match axis {
        Axis::X => Vec3::X,
        Axis::Y => Vec3::Y,
        Axis::Z => Vec3::Z,
    } * (end - start).signum();
    let origin = match axis {
        Axis::X => vec3(start, sample.x, sample.y),
        Axis::Y => vec3(sample.x, start, sample.y),
        Axis::Z => vec3(sample.x, sample.y, start),
    };
    let first = (start.min(end) - 0.5).floor() as i32;
    let last = (start.max(end) - 0.5).floor() as i32;
    let fixed_a = (sample.x - 0.5).floor() as i32;
    let fixed_b = (sample.y - 0.5).floor() as i32;
    let mut best: Option<(f32, f32)> = None;
    for cell in first..=last {
        let (x, y, z) = match axis {
            Axis::X => (cell, fixed_a, fixed_b),
            Axis::Y => (fixed_a, cell, fixed_b),
            Axis::Z => (fixed_a, fixed_b, cell),
        };
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            continue;
        }
        let chunk_origin = vec3(
            x.div_euclid(CHUNK_WIDTH as i32) as f32 * CHUNK_WIDTH as f32,
            0.0,
            z.div_euclid(CHUNK_WIDTH as i32) as f32 * CHUNK_WIDTH as f32,
        );
        let mut valid = true;
        let corners = marching::OFFSETS.map(|(dx, dy, dz)| {
            let coord = WorldCoord::new(x + dx as i32, y + dy as i32, z + dz as i32);
            let solid = if y + dy as i32 == CHUNK_HEIGHT as i32 {
                false
            } else if let Some(voxel) = world.get_voxel(coord) {
                voxel.is_terrain()
            } else {
                valid = false;
                false
            };
            (coord.position() - chunk_origin + Vec3::splat(0.5), solid)
        });
        if !valid {
            continue;
        }
        marching::triangles(corners, |triangle| {
            let vertices = triangle.vertices.map(|v| v + chunk_origin);
            let facing = triangle.normal.dot(ray);
            if facing.abs() < 1e-6 {
                return;
            }
            let distance = (vertices[0] - origin).dot(triangle.normal) / facing;
            if distance < -1e-5 || distance > (end - start).abs() + 1e-5 {
                return;
            }
            let point = origin + ray * distance;
            if (0..3).any(|i| {
                (vertices[(i + 1) % 3] - vertices[i])
                    .cross(point - vertices[i])
                    .dot(triangle.normal) < -1e-5
            }) {
                return;
            }
            let coordinate = axis.get(&point);
            if best.is_none_or(|(best_point, _)| (coordinate - start).abs() < (best_point - start).abs()) {
                best = Some((coordinate, facing));
            }
        });
    }
    best
}
