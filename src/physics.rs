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

/// Applies gravity and resolves player movement against blocks and terrain on each axis.
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
        #[cfg(since19)]
        let (previous, velocity) = (transform.translation, axis.get(&player.velocity));
        move_axis(
            &mut player,
            &mut transform.translation,
            &world,
            dt,
            axis,
        );

        #[cfg(since19)]
        collide_with_terrain(
            &mut player,
            &mut transform.translation,
            &world,
            previous,
            velocity,
            axis,
        );
    }
}

/// Moves the player along one axis and resolves collisions with blocking voxels.
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
    player: &mut Player,
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
        axis.set(&mut player.velocity, 0.0);
        if axis == Axis::Y && velocity < 0.0 {
            player.grounded = true;
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

#[cfg(all(test, since19))]
mod tests {
    use super::*;
    use crate::{ChunkCoord, ChunkData, ChunkGenerationPhase, Density, TerrainType, VoxelType, CHUNK_WIDTH};
    use std::sync::Arc;

    /// Creates a ready, empty three-by-three chunk world for collision tests.
    fn world() -> VoxelWorld {
        let mut world = VoxelWorld::default();
        for z in -1..=1 {
            for x in -1..=1 {
                let coord = ChunkCoord::new(x, z);
                let mut chunk = ChunkData::new(coord, Entity::PLACEHOLDER);
                chunk.generation_phase = ChunkGenerationPhase::Done;
                world.add_chunk(coord, Arc::new(chunk));
            }
        }
        world
    }

    /// Sets a voxel in the test world without updating its density.
    fn set(world: &mut VoxelWorld, pos: WorldCoord, voxel: VoxelType) {
        let chunk = Arc::make_mut(world.chunks.get_mut(&pos.chunk_coord()).unwrap());
        chunk[pos.local_coord()] = voxel;
    }

    /// Fills a square test area with three layers of flat terrain.
    fn flat(world: &mut VoxelWorld, min: i32, max: i32) {
        for x in min..=max {
            for z in min..=max {
                for y in 0..3 {
                    set(world, WorldCoord::new(x, y, z), VoxelType::from_terrain(TerrainType::Stone));
                }
            }
        }
    }

    /// Runs one axis of block and terrain collision using the player's current velocity.
    fn step(player: &mut Player, position: &mut Vec3, world: &VoxelWorld, dt: f32, axis: Axis) {
        let previous = *position;
        let velocity = axis.get(&player.velocity);
        move_axis(player, position, world, dt, axis);
        collide_with_terrain(player, position, world, previous, velocity, axis);
    }

    #[test]
    fn terrain_collision_uses_binary_triangles_not_density() {
        let mut world = world();
        flat(&mut world, 8, 13);
        set(&mut world, WorldCoord::new(10, 3, 10), VoxelType::from_terrain(TerrainType::Stone));
        let ray = terrain_ray(&world, Axis::Y, vec2(10.75, 10.75), 5.0, 2.0).unwrap();
        assert!((ray.0 - 3.5).abs() < 1e-5, "slope height: {}", ray.0);
        let mut player = Player { velocity: vec3(0.0, -10.0, 0.0), grounded: false };
        let mut position = vec3(10.75, 5.0, 10.75);
        step(&mut player, &mut position, &world, 0.3, Axis::Y);
        assert!((position.y - ray.0).abs() < 1e-5);
        assert!(player.grounded);
        assert_eq!(player.velocity.y, 0.0);
        for chunk in world.chunks.values_mut() {
            Arc::make_mut(chunk).density.fill(Density::FULL);
        }
        assert_eq!(terrain_ray(&world, Axis::Y, vec2(10.75, 10.75), 5.0, 2.0), Some(ray));
    }

    #[test]
    fn flat_ground_is_stable_and_blocks_do_not_change_terrain() {
        let mut world = world();
        flat(&mut world, 8, 13);
        let mut player = Player::default();
        let mut position = vec3(10.5, 3.0, 10.5);
        for _ in 0..100 {
            player.velocity.y = -0.3;
            player.grounded = false;
            step(&mut player, &mut position, &world, 0.02, Axis::Y);
            assert!((position.y - 3.0).abs() < 1e-5);
            assert!(player.grounded);
        }
        let before = terrain_ray(&world, Axis::Y, vec2(10.5, 10.5), 5.0, 2.0);
        set(&mut world, WorldCoord::new(10, 3, 10), VoxelType::from_block(crate::BlockType::Log));
        assert_eq!(before, terrain_ray(&world, Axis::Y, vec2(10.5, 10.5), 5.0, 2.0));
    }

    #[test]
    fn terrain_collision_crosses_chunk_boundaries_and_negative_coordinates() {
        let mut world = world();
        flat(&mut world, -2, 2);
        let width = CHUNK_WIDTH as i32;
        for x in width - 2..=width + 1 {
            for z in -1..=1 {
                for y in 0..3 {
                    set(&mut world, WorldCoord::new(x, y, z), VoxelType::from_terrain(TerrainType::Stone));
                }
            }
        }
        for x in [-0.25, 0.25, width as f32 - 0.25, width as f32 + 0.25] {
            let hit = terrain_ray(&world, Axis::Y, vec2(x, 0.0), 5.0, 2.0).unwrap();
            assert!((hit.0 - 3.0).abs() < 1e-5, "x={x}: {hit:?}");
        }
    }

    #[test]
    fn uphill_overlap_resolves_to_the_slope() {
        let mut world = world();
        flat(&mut world, 8, 13);
        set(&mut world, WorldCoord::new(10, 3, 10), VoxelType::from_terrain(TerrainType::Stone));
        let mut player = Player { velocity: vec3(0.0, -0.3, 0.0), grounded: false };
        let mut position = vec3(10.75, 3.0, 10.75);
        step(&mut player, &mut position, &world, 0.02, Axis::Y);
        let expected = terrain_ray(&world, Axis::Y, vec2(10.75, 10.75), 5.0, 2.0).unwrap().0;
        assert!((position.y - expected).abs() < 1e-5);
        assert!(player.grounded);
    }

    #[test]
    fn walking_up_and_down_slope_tracks_the_rendered_surface() {
        let mut world = world();
        flat(&mut world, 8, 13);
        set(&mut world, WorldCoord::new(10, 3, 10), VoxelType::from_terrain(TerrainType::Stone));
        let mut player = Player::default();
        let mut position = vec3(11.5, 3.0, 10.5);
        for velocity in [-1.0; 10].into_iter().chain([1.0; 10]).chain([0.0; 10]) {
            player.velocity.x = velocity;
            player.velocity.y -= GRAVITY * 0.1;
            player.grounded = false;
            step(&mut player, &mut position, &world, 0.1, Axis::X);
            step(&mut player, &mut position, &world, 0.1, Axis::Y);
            let expected = terrain_ray(&world, Axis::Y, vec2(position.x, position.z), 5.0, 2.0).unwrap().0;
            assert!(position.y >= expected - 1e-4, "{position:?}, expected {expected}");
            if velocity <= 0.0 {
                assert!((position.y - expected).abs() < 1e-4, "{position:?}, expected {expected}");
                assert!(player.grounded);
            }
        }
        assert!((position.x - 11.5).abs() < 1e-4);
    }

    #[test]
    fn terrain_wall_stops_motion_but_allows_moving_away() {
        let mut world = world();
        for x in 10..=12 {
            for y in 1..=6 {
                for z in 8..=12 {
                    set(&mut world, WorldCoord::new(x, y, z), VoxelType::from_terrain(TerrainType::Stone));
                }
            }
        }
        let mut player = Player { velocity: vec3(5.0, 0.0, 0.0), grounded: false };
        let mut position = vec3(8.5, 2.0, 10.0);
        step(&mut player, &mut position, &world, 0.4, Axis::X);
        assert!((position.x - 9.7).abs() < 1e-5);
        assert_eq!(player.velocity.x, 0.0);
        player.velocity.x = -1.0;
        step(&mut player, &mut position, &world, 0.1, Axis::X);
        assert!((position.x - 9.6).abs() < 1e-5);
    }

    #[test]
    fn terrain_ceiling_and_blocks_remain_solid() {
        let mut world = world();
        for x in 8..=12 {
            for z in 8..=12 {
                for y in 5..=6 {
                    set(&mut world, WorldCoord::new(x, y, z), VoxelType::from_terrain(TerrainType::Stone));
                }
            }
        }
        let mut player = Player { velocity: vec3(0.0, 5.0, 0.0), grounded: false };
        let mut position = vec3(10.0, 2.0, 10.0);
        step(&mut player, &mut position, &world, 0.4, Axis::Y);
        assert!((position.y - 3.2).abs() < 1e-5);
        assert_eq!(player.velocity.y, 0.0);
        assert!(!player.grounded);

        set(&mut world, WorldCoord::new(10, 2, 10), VoxelType::from_block(crate::BlockType::Log));
        player.velocity.y = -4.0;
        position.y = 4.0;
        step(&mut player, &mut position, &world, 0.4, Axis::Y);
        assert_eq!(position.y, 3.0);
        assert!(player.grounded);
    }

    #[test]
    fn terrain_before_a_block_wins_when_both_are_crossed() {
        let mut world = world();
        flat(&mut world, 8, 12);
        for x in 9..=11 {
            for z in 9..=11 {
                set(&mut world, WorldCoord::new(x, 1, z), VoxelType::from_block(crate::BlockType::Log));
            }
        }
        let mut player = Player { velocity: vec3(0.0, -6.0, 0.0), grounded: false };
        let mut position = vec3(10.0, 5.0, 10.0);
        step(&mut player, &mut position, &world, 0.55, Axis::Y);
        assert!((position.y - 3.0).abs() < 1e-5);
        assert!(player.grounded);
    }
}
