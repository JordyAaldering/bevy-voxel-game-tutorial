mod debug_gizmos;
mod player;
mod screenshot;

use crate::{
    debug_gizmos::DebugGizmosPlugin,
    player::PlayerPlugin,
    screenshot::ScreenshotPlugin,
};

use bevy::{
    prelude::*,
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_inspector_egui::{
    bevy_egui::{
        prelude::*,
        input::EguiWantsInput,
    },
    quick::WorldInspectorPlugin,
};

#[derive(Clone, Copy)]
enum Direction {
    Right,
    Left,
    Top,
    Bottom,
    Front,
    Back,
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            EguiPlugin::default(),
            WorldInspectorPlugin::default(),
            ScreenshotPlugin::default(),
            DebugGizmosPlugin { wireframe: true, ..default() },
            PlayerPlugin::new(is_focused),
        ))
        .add_systems(Startup, (
            global_illumination.spawn(),
            voxel.spawn(),
        ))
        .add_systems(Update, (
            toggle_cursor,
        ))
        .run()
}

fn global_illumination() -> impl Scene {
    bsn! {
        DirectionalLight {
            shadow_maps_enabled: true,
            shadow_normal_bias: 1.0,
            shadow_depth_bias: 0.05,
        }
        Transform::from_rotation(
            Quat::from_euler(EulerRot::XYZ, 6.0, -7.5, 0.0)
        )
    }
}

fn voxel() -> impl Scene {
    bsn! {
        #Voxel
        Transform::from_xyz(0.0, 0.0, 0.0)
        Mesh3d(asset_value(build_mesh()))
        MeshMaterial3d<StandardMaterial>(asset_value(Color::linear_rgb(0.2, 0.7, 0.3)))
    }
}

fn build_mesh() -> Mesh {
    let mut vertices: Vec<Vec3> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut normals: Vec<Vec3> = Vec::new();
    let mut uvs: Vec<Vec2> = Vec::new();

    for direction in [
        Direction::Right,
        Direction::Left,
        Direction::Top,
        Direction::Bottom,
        Direction::Front,
        Direction::Back,
    ] {
        let start = vertices.len() as u32;
        vertices.extend(&quad_vertices(direction));
        indices.extend(&quad_indices(start));
        normals.extend(&quad_normals(direction));
        uvs.extend(&quad_uvs());
    }

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

/// The vertices for a quad in the given direction.
///
/// ```text
///     (0,1,0)——————————————(1,1,0)
///         / |             / |
///        /  |            /  |
/// (0,1,1)—————————(1,1,1)   |
///       |   |           |   |
///       |   |           |   |
///       |  (0,0,0)——————|——(1,0,0)
///       | /             | /
///       |/              |/
/// (0,0,1)———————————————(1,0,1)
/// ```
fn quad_vertices(direction: Direction) -> [Vec3; 4] {
    match direction {
        Direction::Right => [
            Vec3::X + Vec3::Z,
            Vec3::X,
            Vec3::X + Vec3::Y + Vec3::Z,
            Vec3::X + Vec3::Y,
        ],
        Direction::Left => [
            Vec3::ZERO,
            Vec3::Z,
            Vec3::Y,
            Vec3::Y + Vec3::Z,
        ],
        Direction::Top => [
            Vec3::Y + Vec3::Z,
            Vec3::X + Vec3::Y + Vec3::Z,
            Vec3::Y,
            Vec3::X + Vec3::Y,
        ],
        Direction::Bottom => [
            Vec3::ZERO,
            Vec3::X,
            Vec3::Z,
            Vec3::X + Vec3::Z,
        ],
        Direction::Front => [
            Vec3::Z,
            Vec3::X + Vec3::Z,
            Vec3::Y + Vec3::Z,
            Vec3::X + Vec3::Y + Vec3::Z,
        ],
        Direction::Back => [
            Vec3::X,
            Vec3::ZERO,
            Vec3::X + Vec3::Y,
            Vec3::Y,
        ],
    }
}

/// The vertex indices for a quad (two triangles).
///
/// ```text
/// 2————3
/// | \  |
/// |  \ |
/// 0————1
/// ```
fn quad_indices(start: u32) -> [u32; 6] {
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

/// The normals of the 4 corners of a quad.
fn quad_normals(direction: Direction) -> [Vec3; 4] {
    let normal = match direction {
        Direction::Right  => Vec3::X,
        Direction::Left   => Vec3::NEG_X,
        Direction::Top    => Vec3::Y,
        Direction::Bottom => Vec3::NEG_Y,
        Direction::Front  => Vec3::Z,
        Direction::Back   => Vec3::NEG_Z,
    };
    [normal; 4]
}

/// The UV coordinates of the corners of a quad.
///
/// Note that the y-axis is flipped in UV coordinates;
/// the bottom left voxel has $y = 1$ in UV coordinates.
fn quad_uvs() -> [Vec2; 4] {
    [
        vec2(0.0, 1.0), // Bottom left
        vec2(1.0, 1.0), // Bottom right
        vec2(0.0, 0.0), // Top left
        vec2(1.0, 0.0), // Top right
    ]
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
