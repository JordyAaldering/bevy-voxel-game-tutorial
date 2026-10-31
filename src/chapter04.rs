mod coords;
mod screenshot;

use crate::{
    coords::*,
    screenshot::ScreenshotPlugin,
};

use std::{mem, ops::{Index, IndexMut}};

use better_default::Default;
use bevy::{
    prelude::*,
    asset::RenderAssetUsages,
    input::mouse::AccumulatedMouseMotion,
    mesh::{Indices, PrimitiveTopology},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_inspector_egui::{
    prelude::*,
    bevy_egui::{
        prelude::*,
        input::EguiWantsInput,
    },
    quick::WorldInspectorPlugin,
};

const CHUNK_WIDTH: usize = 3;
const CHUNK_HEIGHT: usize = 4;

#[derive(Default, Resource, Reflect, InspectorOptions)]
#[reflect(Resource, InspectorOptions)]
struct CameraSettings {
    #[default(5.0)]
    #[inspector(min = 0.1, max = 20.0)]
    move_speed: f32,
    #[default(0.0025)]
    #[inspector(min = 0.0001, max = 0.01)]
    mouse_sensitivity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelType {
    Air,
    Grass,
}

impl VoxelType {
    fn is_transparent(self) -> bool {
        matches!(self, VoxelType::Air)
    }
}

pub struct ChunkData {
    coord: ChunkCoord,
    voxels: [VoxelType; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH],
}

impl ChunkData {
    fn new(coord: ChunkCoord) -> Self {
        Self {
            coord,
            voxels: [VoxelType::Air; _],
        }
    }

    fn index(x: usize, y: usize, z: usize) -> usize {
        x + CHUNK_WIDTH * (y + CHUNK_HEIGHT * z)
    }

    fn get_neighbor_voxel(&self, pos: VoxelCoord, direction: Direction) -> Option<VoxelType> {
        let neighbor_pos = pos + direction;
        if neighbor_pos.in_bounds() {
            Some(self[neighbor_pos])
        } else {
            None
        }
    }

    fn iter(&self) -> impl Iterator<Item = (VoxelCoord, VoxelType)> {
        (0..CHUNK_WIDTH).flat_map(move |z| {
            (0..CHUNK_HEIGHT).flat_map(move |y| {
                (0..CHUNK_WIDTH).map(move |x| {
                    let pos = VoxelCoord::new(x, y, z);
                    (pos, self[pos])
                })
            })
        })
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

#[derive(Default)]
struct MeshData {
    indices: Vec<u32>,
    vertices: Vec<Vec3>,
    normals: Vec<Vec3>,
    uvs: Vec<Vec2>,
}

impl MeshData {
    fn add_quad(&mut self, direction: Direction, origin: Vec3) {
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

    fn build(&mut self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, mem::take(&mut self.vertices))
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, mem::take(&mut self.normals))
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, mem::take(&mut self.uvs))
            .with_inserted_indices(Indices::U32(mem::take(&mut self.indices)))
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            EguiPlugin::default(),
            WorldInspectorPlugin::default(),
            ScreenshotPlugin::default(),
        ))
        .init_resource::<CameraSettings>()
        .register_type::<CameraSettings>()
        .add_systems(Startup, (
            spawn_global_illumination,
            spawn_player,
            spawn_chunk,
        ))
        .add_systems(Update, (
            toggle_cursor,
            (player_look, player_move)
                .run_if(is_focused),
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

fn spawn_player(mut commands: Commands) {
    commands.spawn((
        Name::new("Player"),
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 0.9,
            ..default()
        }),
        Msaa::Off,
        Transform::from_xyz(5.0, 6.0, 11.0)
            .looking_at(vec3(0.5, 0.5, 0.5), Vec3::Y),
    ));
}

fn spawn_chunk(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let coord = ChunkCoord::new(0, 0);
    let mut chunk = ChunkData::new(coord);

    for z in 0..CHUNK_WIDTH {
        for y in 0..(CHUNK_HEIGHT / 2) {
            for x in 0..CHUNK_WIDTH {
                let idx = ChunkData::index(x, y, z);
                chunk.voxels[idx] = VoxelType::Grass;
            }
        }
    }

    commands.spawn((
        Name::new(format!("Chunk {coord}")),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Mesh3d(meshes.add(build_mesh(&chunk))),
        MeshMaterial3d(materials.add(Color::linear_rgb(0.2, 0.7, 0.3))),
    ));
}

fn build_mesh(chunk: &ChunkData) -> Mesh {
    let mut mesh_data = MeshData::default();

    for (voxel_pos, _) in chunk.iter()
        .filter(|(_, v)| *v != VoxelType::Air)
    {
        for direction in Direction::iter() {
            if is_face_visible(chunk, voxel_pos, direction) {
                let origin = voxel_pos.local_position();
                mesh_data.add_quad(direction, origin);
            }
        }
    }

    mesh_data.build()
}

fn is_face_visible(chunk: &ChunkData, voxel_pos: VoxelCoord, direction: Direction) -> bool {
    chunk.get_neighbor_voxel(voxel_pos, direction)
        .is_none_or(VoxelType::is_transparent)
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
    else if mouse.just_pressed(MouseButton::Left)
        && !egui_wants_input.wants_pointer_input()
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

fn player_look(
    settings: Res<CameraSettings>,
    mouse: Res<AccumulatedMouseMotion>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    let delta = mouse.delta;

    if delta != Vec2::ZERO {
        camera.rotate_y(-delta.x * settings.mouse_sensitivity);
        camera.rotate_local_x(-delta.y * settings.mouse_sensitivity);
    }
}

fn player_move(
    time: Res<Time>,
    settings: Res<CameraSettings>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    let delta = vec3(
        f32::from(keyboard.pressed(KeyCode::KeyD)) - f32::from(keyboard.pressed(KeyCode::KeyA)),
        f32::from(keyboard.pressed(KeyCode::Space)) - f32::from(keyboard.pressed(KeyCode::ShiftLeft)),
        f32::from(keyboard.pressed(KeyCode::KeyW)) - f32::from(keyboard.pressed(KeyCode::KeyS)),
    );

    if delta != Vec3::ZERO {
        let direction = delta.x * camera.right() + delta.y * Vec3::Y + delta.z * camera.forward();
        camera.translation += settings.move_speed * time.delta_secs() * direction.normalize();
    }
}
