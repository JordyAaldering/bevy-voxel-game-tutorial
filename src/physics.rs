use crate::{
    coords::{Axis, WorldCoord},
    Player,
    VoxelWorld,
};

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
                if !world.is_solid(WorldCoord::new(x, y, z)) {
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
