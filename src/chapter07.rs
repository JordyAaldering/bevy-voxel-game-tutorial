mod axis;
mod coordinates;
mod direction;
mod meshdata;
mod physics;
mod screenshot;

use crate::{
    coordinates::{ChunkCoord, VoxelCoord, WorldCoord},
    direction::Direction,
    meshdata::MeshData,
    physics::PhysicsPlugin,
    screenshot::ScreenshotPlugin,
};

use std::ops::{Index, IndexMut};

use better_default::Default;
use bevy::{
    prelude::*,
    input::mouse::AccumulatedMouseMotion,
    math::CompassQuadrant,
    platform::collections::HashMap,
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

const ATLAS_ROWS: u32 = 4;
const ATLAS_COLUMNS: u32 = 4;

const PLAYER_EYE_HEIGHT: f32 = 1.6;
const MAX_PITCH: f32 = 1.4;

#[derive(Clone, Copy, Default, PartialEq, Eq, Resource)]
enum GameMode {
    #[default]
    Survival,
    Spectator,
}

#[derive(Default, Component)]
struct Player {
    velocity: Vec3,
    grounded: bool,
}

#[derive(Component)]
struct PlayerCamera;

#[derive(Default, Resource, Reflect, InspectorOptions)]
#[reflect(Resource, InspectorOptions)]
struct CameraSettings {
    #[default(5.0)]
    #[inspector(min = 0.1, max = 20.0)]
    move_speed: f32,
    #[default(7.0)]
    #[inspector(min = 0.1, max = 20.0)]
    jump_strenght: f32,
    #[default(0.0025)]
    #[inspector(min = 0.0001, max = 0.01)]
    mouse_sensitivity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelType {
    Air,
    Grass,
    Dirt,
}

impl VoxelType {
    fn is_transparent(self) -> bool {
        matches!(self, VoxelType::Air)
    }

    fn is_solid(&self) -> bool {
        !matches!(self, VoxelType::Air)
    }

    fn texture(self, direction: Direction) -> usize {
        fn atlas_index(x: usize, y: usize) -> usize {
            x + (y * ATLAS_COLUMNS as usize)
        }

        match self {
            Self::Grass => {
                match direction {
                    Direction::Top => atlas_index(3, 1),
                    Direction::Bottom => atlas_index(1, 0),
                    _ => atlas_index(2, 0),
                }
            },
            Self::Dirt => atlas_index(1, 0),
            _ => unreachable!(),
        }
    }
}

#[derive(Clone)]
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

#[derive(Default, Resource)]
struct VoxelWorld {
    chunks: HashMap<ChunkCoord, ChunkData>,
}

impl VoxelWorld {
    fn add_chunk(&mut self, pos: ChunkCoord, chunk: ChunkData) {
        debug_assert!(!self.chunks.contains_key(&pos));
        self.chunks.insert(pos, chunk);
    }

    fn get_chunk(&self, pos: ChunkCoord) -> Option<&ChunkData> {
        self.chunks.get(&pos)
    }

    fn is_solid(&self, pos: WorldCoord) -> bool {
        self.get_voxel(pos)
            .is_some_and(|voxel| voxel.is_solid())
    }

    fn get_voxel(&self, pos: WorldCoord) -> Option<VoxelType> {
        if pos.y < 0 || pos.y >= CHUNK_HEIGHT as i32 {
            return None;
        }

        let chunk_pos = pos.chunk_coord();
        self.get_chunk(chunk_pos).map(|chunk| {
            let voxel_pos = pos.local_coord();
            chunk[voxel_pos]
        })
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest()),
            EguiPlugin::default(),
            WorldInspectorPlugin::default(),
            ScreenshotPlugin::default(),
            PhysicsPlugin,
        ))
        .init_resource::<GameMode>()
        .init_resource::<VoxelWorld>()
        .init_resource::<CameraSettings>()
        .register_type::<CameraSettings>()
        .add_systems(Startup, (
            spawn_global_illumination,
            spawn_player,
            spawn_world,
        ))
        .add_systems(Update, (
            toggle_cursor,
            (
                toggle_gamemode,
                player_look,
                move_survival.run_if(is_gamemode_survival),
                move_spectator.run_if(is_gamemode_spectator),
            )
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
            Player::default(),
            Projection::Perspective(PerspectiveProjection {
                fov: 0.9,
                ..default()
            }),
            Msaa::Off,
            Transform::from_xyz(0.5, 5.0, 0.5),
        ))
        .with_children(|commands| {
            commands.spawn((
                PlayerCamera,
                Camera3d::default(),
                Transform::from_xyz(0.0, PLAYER_EYE_HEIGHT, 0.0),
            ));
        });
}

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut world: ResMut<VoxelWorld>,
    asset_server: Res<AssetServer>,
) {
    let layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::splat(ATLAS_COLUMNS * ATLAS_ROWS),
        ATLAS_COLUMNS,
        ATLAS_ROWS,
        None,
        None,
    ));
    let layout = layouts.get(&layout).unwrap();

    let texture = asset_server.load("blocks.png");

    let material = materials.add(StandardMaterial {
        base_color_texture: Some(texture),
        perceptual_roughness: 0.9,
        reflectance: 0.1,
        ..default()
    });

    for z in -1..=1 {
        for x in -1..=1 {
            let coord = ChunkCoord::new(x, z);
            let chunk = generate_chunk(coord);
            world.add_chunk(coord, chunk);
        }
    }

    for (&chunk_pos, chunk) in &world.chunks {
        commands.spawn((
            Name::new(format!("Chunk {chunk_pos}")),
            Transform::from_translation(chunk_pos.position()),
            Mesh3d(meshes.add(build_mesh(chunk, chunk_pos, &world, layout))),
            MeshMaterial3d(material.clone()),
        ));
    }
}

fn generate_chunk(coord: ChunkCoord) -> ChunkData {
    let mut chunk = ChunkData::new(coord);

    for z in 0..CHUNK_WIDTH {
        let max_y = (coord.x + coord.z) as usize % CHUNK_HEIGHT;
        for y in 0..=max_y {
            for x in 0..CHUNK_WIDTH {
                let pos = VoxelCoord::new(x, y, z);
                chunk[pos] = if y == max_y {
                    VoxelType::Grass
                } else {
                    VoxelType::Dirt
                };
            }
        }
    }

    chunk
}

fn build_mesh(chunk: &ChunkData, chunk_pos: ChunkCoord, world: &VoxelWorld, atlas: &TextureAtlasLayout) -> Mesh {
    let mut mesh_data = MeshData::new(atlas);

    for (voxel_pos, voxel) in chunk.iter()
        .filter(|(_, v)| *v != VoxelType::Air)
    {
        for direction in Direction::iter() {
            if is_face_visible(world, chunk, chunk_pos, voxel_pos, direction) {
                let origin = voxel_pos.local_position();
                let texture = voxel.texture(direction);
                mesh_data.add_quad(direction, origin, texture);
            }
        }
    }

    mesh_data.build()
}

fn is_face_visible(world: &VoxelWorld, chunk: &ChunkData, chunk_pos: ChunkCoord, voxel_pos: VoxelCoord, direction: Direction) -> bool {
    if let Some(neighbor) = chunk.get_neighbor_voxel(voxel_pos, direction) {
        return neighbor.is_transparent();
    }

    // Chunks only exist horizontally, always show the top-most and bottom-most faces
    let Ok(compass) = direction.try_into() else {
        return true;
    };

    // If the chunk does not exist, we assume it is empty and show the face
    let Some(neighbor_chunk) = world.get_chunk(chunk_pos + compass) else {
        return true;
    };

    let neighbor_pos = match compass {
        CompassQuadrant::North => VoxelCoord { z: 0, ..voxel_pos },
        CompassQuadrant::East  => VoxelCoord { x: 0, ..voxel_pos },
        CompassQuadrant::South => VoxelCoord { z: CHUNK_WIDTH - 1, ..voxel_pos },
        CompassQuadrant::West  => VoxelCoord { x: CHUNK_WIDTH - 1, ..voxel_pos },
    };

    neighbor_chunk[neighbor_pos]
        .is_transparent()
}

fn is_focused(cursor: Single<&CursorOptions, With<PrimaryWindow>>) -> bool {
    cursor.grab_mode == CursorGrabMode::Locked
}

fn is_gamemode_survival(mode: Res<GameMode>) -> bool {
    *mode == GameMode::Survival
}

fn is_gamemode_spectator(mode: Res<GameMode>) -> bool {
    *mode == GameMode::Spectator
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

fn toggle_gamemode(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut player: Single<&mut Player>,
    mut mode: ResMut<GameMode>,
) {
    if keyboard.just_pressed(KeyCode::KeyF) {
        *mode = match *mode {
            GameMode::Survival => GameMode::Spectator,
            GameMode::Spectator => GameMode::Survival,
        };
        // Don't retain old vertical momentum
        player.velocity = Vec3::ZERO;
    }
}

fn player_look(
    settings: Res<CameraSettings>,
    mouse: Res<AccumulatedMouseMotion>,
    mut camera: Single<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
    mut player: Single<&mut Transform, (With<Player>, Without<PlayerCamera>)>,
) {
    let delta = mouse.delta;

    if delta != Vec2::ZERO {
        // Horizontal movement rotates the entire player around the world Y axis
        player.rotate_y(-delta.x * settings.mouse_sensitivity);
        // Vertical movement rotates only the camera around its local X axis
        let (yaw, mut pitch, roll) = camera.rotation.to_euler(EulerRot::YXZ);

        pitch -= delta.y * settings.mouse_sensitivity;
        pitch = pitch.clamp(-MAX_PITCH, MAX_PITCH);

        camera.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, roll);
    }
}

fn move_spectator(
    time: Res<Time>,
    settings: Res<CameraSettings>,
    keyboard: Res<ButtonInput<KeyCode>>,
    player: Single<(&mut Transform, &mut Player)>,
) {
    let (mut transform, mut player) = player.into_inner();

    player.velocity = Vec3::ZERO;
    player.grounded = false;

    let delta = vec3(
        f32::from(keyboard.pressed(KeyCode::KeyD)) - f32::from(keyboard.pressed(KeyCode::KeyA)),
        f32::from(keyboard.pressed(KeyCode::Space)) - f32::from(keyboard.pressed(KeyCode::ShiftLeft)),
        f32::from(keyboard.pressed(KeyCode::KeyW)) - f32::from(keyboard.pressed(KeyCode::KeyS)),
    );

    if delta != Vec3::ZERO {
        let direction = delta.x * transform.right() + delta.y * Vec3::Y + delta.z * transform.forward();
        transform.translation += settings.move_speed * time.delta_secs() * direction.normalize();
    }
}

fn move_survival(
    settings: Res<CameraSettings>,
    keyboard: Res<ButtonInput<KeyCode>>,
    player: Single<(&mut Player, &Transform)>,
) {
    let (mut player, transform) = player.into_inner();

    let dx = f32::from(keyboard.pressed(KeyCode::KeyD)) - f32::from(keyboard.pressed(KeyCode::KeyA));
    let dz = f32::from(keyboard.pressed(KeyCode::KeyW)) - f32::from(keyboard.pressed(KeyCode::KeyS));

    if dx != 0.0 || dz != 0.0 {
        let movement = dx * transform.right() + dz * transform.forward();
        let movement = movement.normalize_or_zero();
        player.velocity.x = movement.x * settings.move_speed;
        player.velocity.z = movement.z * settings.move_speed;
    } else {
        player.velocity.x = 0.0;
        player.velocity.z = 0.0;
    }

    if player.grounded && keyboard.just_pressed(KeyCode::Space) {
        player.velocity.y = settings.jump_strenght;
        player.grounded = false;
    }
}
