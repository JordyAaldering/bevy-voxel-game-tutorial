mod coords;
mod player;
mod screenshot;

use crate::{
    chunk::{CHUNK_WIDTH, CHUNK_HEIGHT},
    coords::Direction,
    player::PlayerPlugin,
    screenshot::ScreenshotPlugin,
};

use bevy::{
    prelude::*,
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    pbr::wireframe::{WireframeConfig, WireframePlugin},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_inspector_egui::{
    bevy_egui::{
        prelude::*,
        input::EguiWantsInput,
    },
    quick::WorldInspectorPlugin,
};

mod chunk {
    use super::*;

    pub const CHUNK_WIDTH: usize = 3;
    pub const CHUNK_HEIGHT: usize = 4;

    struct Face {
        direction: Direction,
    }

    impl Face {
        /// Returns the four vertices of the quad.
        ///
        /// The quad is defined by the origin and direction of the face:
        /// ```text
        ///     (0,1,0)-------------------(1,1,0)
        ///          /|                  /|
        ///         / |                 / |
        ///        /  |                /  |
        /// (0,1,1)-------------(1,1,1)   |
        ///       |   |               |   |
        ///       |   |               |   |
        ///       |   |               |   |
        ///       |  (0,0,0)----------|--(1,0,0)
        ///       | /                 | /
        ///       |/                  |/
        /// (0,0,1)-------------------(1,0,1)
        /// ```
        ///
        /// Note that the z axis points forward, so the going back decreases the z coordinate.
        fn vertices(&self) -> [Vec3; 4] {
            let o = Vec3::ZERO;

            match self.direction {
                Direction::Right => [
                    o + Vec3::X + Vec3::Z,
                    o + Vec3::X,
                    o + Vec3::X + Vec3::Y + Vec3::Z,
                    o + Vec3::X + Vec3::Y,
                ],
                Direction::Left => [
                    o,
                    o + Vec3::Z,
                    o + Vec3::Y,
                    o + Vec3::Y + Vec3::Z,
                ],
                Direction::Top => [
                    o + Vec3::Y + Vec3::Z,
                    o + Vec3::X + Vec3::Y + Vec3::Z,
                    o + Vec3::Y,
                    o + Vec3::X + Vec3::Y,
                ],
                Direction::Bottom => [
                    o,
                    o + Vec3::X,
                    o + Vec3::Z,
                    o + Vec3::X + Vec3::Z,
                ],
                Direction::Front => [
                    o + Vec3::Z,
                    o + Vec3::X + Vec3::Z,
                    o + Vec3::Y + Vec3::Z,
                    o + Vec3::X + Vec3::Y + Vec3::Z,
                ],
                Direction::Back => [
                    o + Vec3::X,
                    o,
                    o + Vec3::X + Vec3::Y,
                    o + Vec3::Y,
                ],
            }
        }

        /// Returns the vertex indices for the quad (two triangles).
        ///
        /// Vertices are ordered as viewed from outside the voxel:
        /// ```text
        /// 2----3
        /// | \  |
        /// |  \ |
        /// 0----1
        /// ```
        fn indices(&self, start: u32) -> [u32; 6] {
            [
                // First triangle
                start + 0, // Bottom left
                start + 1, // Bottom right
                start + 2, // Top left
                // Second triangle
                start + 2, // Top left
                start + 1, // Bottom right
                start + 3, // Top right
            ]
        }

        /// Returns the normals of the 4 corners of the quad.
        fn normals(&self) -> [Vec3; 4] {
            [self.direction.vec3(); 4]
        }

        /// Returns the UV coordinates of the 4 corners of the quad.
        ///
        /// Note that the y-axis is flipped in UV coordinates, so the
        /// bottom left voxel actually has `y = 1` in UV coordinates.
        fn uvs(&self) -> [Vec2; 4] {
            [
                vec2(0.0, 1.0), // Bottom left
                vec2(1.0, 1.0), // Bottom right
                vec2(0.0, 0.0), // Top left
                vec2(1.0, 0.0), // Top right
            ]
        }
    }

    pub fn build_mesh() -> Mesh {
        let mut vertices: Vec<Vec3> = Vec::with_capacity(6 * 4);
        let mut indices: Vec<u32> = Vec::with_capacity(6 * 2 * 3);
        let mut normals: Vec<Vec3> = Vec::with_capacity(6 * 4);
        let mut uvs: Vec<Vec2> = Vec::with_capacity(6 * 4);

        for direction in Direction::iter() {
            let face = Face { direction };

            let start = vertices.len() as u32;
            vertices.extend(&face.vertices());
            indices.extend(&face.indices(start));
            normals.extend(&face.normals());
            uvs.extend(&face.uvs());
        }

        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
            .with_inserted_indices(Indices::U32(indices))
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            EguiPlugin::default(),
            WorldInspectorPlugin::default(),
            WireframePlugin::default(),
            ScreenshotPlugin::default(),
            PlayerPlugin::new(is_focused),
        ))
        .insert_resource(WireframeConfig {
            global: true,
            ..default()
        })
        .add_systems(Startup, (
            spawn_global_illumination,
            spawn_voxel,
        ))
        .add_systems(Update, (
            toggle_cursor,
        ))
        .run()
}

fn spawn_global_illumination(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            shadow_normal_bias: 1.0,
            shadow_depth_bias: 0.05,
            ..default()
        },
        Transform::from_rotation(
            Quat::from_euler(EulerRot::XYZ, 6.0, -7.5, 0.0)
        ),
    ));
}

fn spawn_voxel(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0),
        Mesh3d(meshes.add(chunk::build_mesh())),
        MeshMaterial3d(materials.add(Color::linear_rgb(0.2, 0.7, 0.3))),
    ));
}

fn is_focused(cursor: Single<&CursorOptions, With<PrimaryWindow>>) -> bool {
    cursor.grab_mode == CursorGrabMode::Locked
}

fn toggle_cursor(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    egui_wants_input: Res<EguiWantsInput>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    // Release the cursor when escape is pressed
    if keyboard.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
    // Recapture when a click occurs outside egui
    if mouse.just_pressed(MouseButton::Left)
        && !egui_wants_input.wants_pointer_input()
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}
