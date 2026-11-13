mod coords;
mod debug_gizmos;
mod player;
mod screenshot;

use crate::{
    chunk::{ChunkData, CHUNK_WIDTH, CHUNK_HEIGHT},
    coords::{Direction, VoxelCoord},
    debug_gizmos::DebugGizmosPlugin,
    meshdata::MeshData,
    player::PlayerPlugin,
    screenshot::ScreenshotPlugin,
    voxel::VoxelType,
};

use std::{mem, ops::{Index, IndexMut}};

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

mod voxel {
    #[derive(Clone, Copy)]
    pub enum VoxelType {
        Air,
        Grass,
    }

    impl VoxelType {
        pub fn is_transparent(self) -> bool {
            matches!(self, VoxelType::Air)
        }
    }
}

mod chunk {
    use super::*;

    pub const CHUNK_WIDTH: usize = 3;
    pub const CHUNK_HEIGHT: usize = 4;

    #[derive(Clone)]
    pub struct ChunkData {
        voxels: [VoxelType; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH],
    }

    impl ChunkData {
        pub fn new() -> Self {
            Self {
                voxels: [VoxelType::Air; _],
            }
        }

        pub fn get_neighbor_voxel(&self, pos: VoxelCoord, direction: Direction) -> Option<VoxelType> {
            let neighbor_pos = pos + direction;
            if neighbor_pos.in_bounds() {
                Some(self[neighbor_pos])
            } else {
                None
            }
        }

        pub fn iter(&self) -> impl Iterator<Item = (VoxelCoord, VoxelType)> {
            (0..CHUNK_WIDTH).flat_map(move |z| {
                (0..CHUNK_HEIGHT).flat_map(move |y| {
                    (0..CHUNK_WIDTH).map(move |x| {
                        let pos = VoxelCoord::new(x, y, z);
                        (pos, self[pos])
                    })
                })
            })
        }

        pub fn build_mesh(&self) -> Mesh {
            let mut mesh_data = MeshData::default();

            for (voxel_pos, _) in self.iter()
                .filter(|(_, v)| matches!(v, VoxelType::Air))
            {
                for direction in Direction::iter() {
                    if self.is_face_visible(voxel_pos, direction) {
                        let origin = voxel_pos.local_position();
                        mesh_data.add_quad(direction, origin);
                    }
                }
            }

            mesh_data.build()
        }

        fn is_face_visible(&self, voxel_pos: VoxelCoord, direction: Direction) -> bool {
            self.get_neighbor_voxel(voxel_pos, direction)
                .is_none_or(VoxelType::is_transparent)
        }
    }

    impl Index<VoxelCoord> for ChunkData {
        type Output = VoxelType;

        fn index(&self, pos: VoxelCoord) -> &Self::Output {
            &self.voxels[pos.index()]
        }
    }

    impl IndexMut<VoxelCoord> for ChunkData {
        fn index_mut(&mut self, pos: VoxelCoord) -> &mut Self::Output {
            &mut self.voxels[pos.index()]
        }
    }
}

mod meshdata {
    use super::*;

    #[derive(Default)]
    pub struct MeshData {
        indices: Vec<u32>,
        vertices: Vec<Vec3>,
        normals: Vec<Vec3>,
        uvs: Vec<Vec2>,
    }

    impl MeshData {
        pub fn add_quad(&mut self, direction: Direction, origin: Vec3) {
            let start = self.vertices.len() as u32;
            self.add_quad_indices(start);
            self.add_quad_vertices(direction, origin);
            self.add_quad_normals(direction);
            self.add_quad_uvs();
        }

        fn add_quad_indices(&mut self, start: u32) {
            self.indices.extend([
                // First triangle
                start + 0, // Bottom left
                start + 1, // Bottom right
                start + 2, // Top left
                // Second triangle
                start + 2, // Top left
                start + 1, // Bottom right
                start + 3, // Top right
            ]);
        }

        fn add_quad_vertices(&mut self, direction: Direction, o: Vec3) {
            self.vertices.extend(match direction {
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
            });
        }

        fn add_quad_normals(&mut self, direction: Direction) {
            self.normals.extend([direction.vec3(); 4]);
        }

        fn add_quad_uvs(&mut self) {
            self.uvs.extend([
                vec2(0.0, 1.0), // Bottom left
                vec2(1.0, 1.0), // Bottom right
                vec2(0.0, 0.0), // Top left
                vec2(1.0, 0.0), // Top right
            ]);
        }

        pub fn build(&mut self) -> Mesh {
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, mem::take(&mut self.vertices))
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, mem::take(&mut self.normals))
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, mem::take(&mut self.uvs))
                .with_inserted_indices(Indices::U32(mem::take(&mut self.indices)))
        }
    }
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
            chunk.spawn(),
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

fn chunk() -> impl Scene {
    let mut chunk = ChunkData::new();
    for z in 0..CHUNK_WIDTH {
        for y in 0..(CHUNK_HEIGHT / 2) {
            for x in 0..CHUNK_WIDTH {
                let pos = VoxelCoord::new(x, y, z);
                chunk[pos] = VoxelType::Grass;
            }
        }
    }

    bsn! {
        #Chunk
        Transform
        Mesh3d(asset_value(chunk.build_mesh()))
        MeshMaterial3d<StandardMaterial>(asset_value(Color::linear_rgb(0.2, 0.7, 0.3)))
    }
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
