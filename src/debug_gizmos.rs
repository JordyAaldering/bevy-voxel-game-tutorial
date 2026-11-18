#[allow(unused_imports)]
use crate::*;

use better_default::Default;
use bevy::{
    prelude::*,
    pbr::wireframe::{WireframeConfig, WireframePlugin},
};
#[allow(unused_imports)]
use bevy_inspector_egui::prelude::*;

#[derive(Default)]
pub struct DebugGizmosPlugin {
    /// Whether mesh wireframes are shown.
    pub wireframe: bool,
    // /// Whether chunk borders are shown.
    // pub chunk_borders: bool,
    /// Whether the chunk spawn radius is drawn around the player.
    #[default(true)]
    pub chunk_spawn_radius: bool,
}

#[cfg(since12)]
#[derive(Default, Resource, Reflect, InspectorOptions)]
#[reflect(Resource, InspectorOptions)]
struct DebugGizmos {
    #[inspector()]
    chunk_borders: bool,
    #[inspector()]
    chunk_spawn_radius: bool,
}

impl Plugin for DebugGizmosPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
                WireframePlugin::default(),
            ))
            .insert_resource(WireframeConfig {
                global: self.wireframe,
                ..default()
            });

        #[cfg(since12)]
        app.register_type::<DebugGizmos>()
            .insert_resource(DebugGizmos {
                chunk_borders: false,
                chunk_spawn_radius: self.chunk_spawn_radius,
            })
            .add_systems(Update, draw_chunk_spawn_radius);
    }
}

#[cfg(since12)]
fn draw_chunk_spawn_radius(
    settings: Res<DebugGizmos>,
    player: Single<&Transform, With<Player>>,
    mut gizmos: Gizmos,
) {
    if !settings.chunk_spawn_radius {
        return;
    }

    const SEGMENTS: usize = 128;

    let center = player.translation;
    let center_2d = center.xz();
    let boundary = |predicate: fn(Vec2, Vec2) -> bool| {
        (0..SEGMENTS).map(move |index| {
            let angle = index as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            let direction = Vec2::from_angle(angle);
            let mut inside = 0.0;
            let mut outside = CHUNK_WIDTH as f32;
            while predicate(center_2d + direction * outside, center_2d) {
                outside *= 2.0;
            }
            for _ in 0..24 {
                let distance = (inside + outside) * 0.5;
                if predicate(center_2d + direction * distance, center_2d) {
                    inside = distance;
                } else {
                    outside = distance;
                }
            }
            let point = center_2d + direction * inside;
            vec3(point.x, center.y + 0.1, point.y)
        })
    };

    gizmos.lineloop(
        boundary(position_is_in_despawn_radius),
        Color::srgb(0.2, 0.7, 1.0),
    );
    gizmos.lineloop(
        boundary(position_is_in_spawn_radius),
        Color::srgb(1.0, 0.3, 0.2),
    );
}
