mod chunk_neighbors;
mod coords;
mod meshdata;
mod noise;
mod raycast;
mod screenshot;

use crate::{
    axis::Axis,
    chunk_neighbors::ChunkNeighbors,
    coords::*,
    meshdata::MeshData,
    raycast::VoxelHit,
    screenshot::ScreenshotPlugin,
};

use std::{ops::{Index, IndexMut}, sync::Arc};

use better_default::Default;
use bevy::{
    prelude::*,
    input::mouse::AccumulatedMouseMotion,
    platform::collections::HashMap,
    tasks::{AsyncComputeTaskPool, futures_lite::future, Task, block_on},
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

const CHUNK_WIDTH: usize = 32;
const CHUNK_HEIGHT: usize = 64;

const CHUNK_LOAD_RADIUS: i32 = 1;

const ATLAS_ROWS: u32 = 4;
const ATLAS_COLUMNS: u32 = 4;
const ATLAS_SIZE: u32 = ATLAS_COLUMNS * ATLAS_ROWS;

/// Prevents exact boundary contact from being treated as an overlap with the adjacent voxel.
const COLLISION_EPSILON: f32 = 1e-3;

const PLAYER_WIDTH: f32 = 0.6;
const PLAYER_WIDTH_HALF: f32 = PLAYER_WIDTH * 0.5;
const PLAYER_HEIGHT: f32 = 1.8;
const PLAYER_EYE_HEIGHT: f32 = 1.6;
const MAX_PITCH: f32 = 1.4;
const GRAVITY: f32 = 20.0;

const INTERACTION_DISTANCE: f32 = 5.0;

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
    Stone,
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
            Self::Stone => atlas_index(0, 0),
            _ => unreachable!(),
        }
    }
}

/// ECS component of a chunk.
#[derive(Component)]
struct Chunk {
    coord: ChunkCoord,
}

#[derive(Resource)]
struct ChunkMaterial(Handle<StandardMaterial>);

#[derive(Component)]
struct NeedsRemesh;

#[derive(Component)]
struct NeedsDespawn;

/// A mesh that is being built on another thread. Dropping it cancels the task.
#[derive(Component)]
struct MeshTask(Task<Mesh>);

#[derive(Clone)]
pub struct ChunkData {
    coord: ChunkCoord,
    voxels: [VoxelType; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH],
    entity: Option<Entity>,
}

impl ChunkData {
    fn new(coord: ChunkCoord, entity: Entity) -> Self {
        Self {
            coord,
            voxels: [VoxelType::Air; _],
            entity: Some(entity),
        }
    }

    fn set_entity(&mut self, entity: Entity) {
        debug_assert!(self.entity.is_none());
        self.entity = Some(entity);
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
    /// Chunk data is shared with meshing tasks. Writes go through [`Arc::make_mut`],
    /// which only copies a chunk if a task is still reading it.
    chunks: HashMap<ChunkCoord, Arc<ChunkData>>,
}

#[derive(Resource)]
struct VoxelAtlas(TextureAtlasLayout);

impl VoxelWorld {
    fn is_solid(&self, pos: WorldCoord) -> bool {
        self.get_voxel(pos)
            .is_some_and(|voxel| voxel.is_solid())
    }

    /// Spawns an entity for the chunk at `coord`. The voxel data is generated only
    /// if it does not exist yet, so earlier modifications survive unloading.
    fn spawn_chunk(&mut self, commands: &mut Commands, coord: ChunkCoord) -> Entity {
        let entity = commands.spawn((
            Name::new(format!("Chunk ({}, {})", coord.x, coord.z)),
            Transform::from_translation(coord.position()),
            Chunk { coord },
            NeedsRemesh,
        )).id();

        if let Some(chunk) = self.chunks.get_mut(&coord) {
            Arc::make_mut(chunk).set_entity(entity);
        } else {
            let mut chunk = ChunkData::new(coord, entity);
            generate_chunk(&mut chunk);
            self.chunks.insert(coord, Arc::new(chunk));
        }

        // Existing neighbors previously treated this side as open, so their border faces change
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            if let Some(neighbor) = self.get_chunk_neighbor(coord, dx, dz) {
                if let Some(neighbor_entity) = neighbor.entity {
                    commands.entity(neighbor_entity).insert(NeedsRemesh);
                }
            }
        }

        entity
    }

    fn get_voxel(&self, pos: WorldCoord) -> Option<VoxelType> {
        if pos.y < 0 || pos.y >= CHUNK_HEIGHT as i32 {
            return None;
        }

        let chunk_pos = pos.chunk_coord();
        self.chunks.get(&chunk_pos).map(|chunk| {
            let voxel_pos = pos.local_coord();
            chunk[voxel_pos]
        })
    }

    fn set_voxel(&mut self, commands: &mut Commands, pos: WorldCoord, voxel: VoxelType) {
        let voxel_pos = pos.local_coord();
        let chunk_pos = pos.chunk_coord();
        let chunk = self.chunks.get_mut(&chunk_pos).unwrap();
        Arc::make_mut(chunk)[voxel_pos] = voxel;

        if let Some(entity) = chunk.entity {
            commands.entity(entity).insert(NeedsRemesh);
        }

        // Voxels on a chunk border influence the faces of the adjacent chunk
        if voxel_pos.x == 0 {
            if let Some(neighbor) = self.get_chunk_neighbor(chunk_pos, -1, 0) {
                if let Some(entity) = neighbor.entity {
                    commands.entity(entity).insert(NeedsRemesh);
                }
            }
        } else if voxel_pos.x == CHUNK_WIDTH - 1 {
            if let Some(neighbor) = self.get_chunk_neighbor(chunk_pos, 1, 0) {
                if let Some(entity) = neighbor.entity {
                    commands.entity(entity).insert(NeedsRemesh);
                }
            }
        }
        if voxel_pos.z == 0 {
            if let Some(neighbor) = self.get_chunk_neighbor(chunk_pos, 0, -1) {
                if let Some(entity) = neighbor.entity {
                    commands.entity(entity).insert(NeedsRemesh);
                }
            }
        } else if voxel_pos.z == CHUNK_WIDTH - 1 {
            if let Some(neighbor) = self.get_chunk_neighbor(chunk_pos, 0, 1) {
                if let Some(entity) = neighbor.entity {
                    commands.entity(entity).insert(NeedsRemesh);
                }
            }
        }
    }

    fn get_chunk_neighbor(&self, pos: ChunkCoord, dx: i32, dz: i32) -> Option<&ChunkData> {
        let neighbor_pos = ChunkCoord::new(pos.x + dx, pos.z + dz);
        self.chunks.get(&neighbor_pos).map(|chunk| &**chunk)
    }

    fn is_loaded(&self, coord: ChunkCoord) -> bool {
        self.chunks.get(&coord)
            .is_some_and(|chunk| chunk.entity.is_some())
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
        ))
        .init_resource::<GameMode>()
        .init_resource::<VoxelWorld>()
        .init_resource::<CameraSettings>()
        .register_type::<CameraSettings>()
        .add_systems(Startup, (
            setup_chunk_material,
            spawn_global_illumination,
            spawn_player,
        ))
        .add_systems(Update, (
            toggle_cursor,
            (
                toggle_gamemode,
                player_look,
                break_place_block,
                move_survival.run_if(is_gamemode_survival),
                move_spectator.run_if(is_gamemode_spectator),
            )
            .run_if(is_focused),
            draw_selected_voxel,
            spawn_chunks_around_player,
            retire_distant_chunks,
        ))
        .add_systems(PostUpdate, (
            despawn_retired_chunks,
            poll_mesh_tasks,
            remesh_chunks,
        ).chain())
        .add_systems(FixedUpdate, (
            player_physics
                .run_if(is_gamemode_survival),
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
    let world_height = noise::opensimplex2s::noise2(0, 0.0, 0.0) * 10.0 + 20.0;

    commands.spawn((
            Name::new("Player"),
            Player::default(),
            Transform::from_xyz(0.5, world_height as f32 + 1.0, 0.5),
        ))
        .with_children(|commands| {
            commands.spawn((
                PlayerCamera,
                Camera3d::default(),
                Transform::from_xyz(0.0, PLAYER_EYE_HEIGHT, 0.0),
            ));
        });
}

fn setup_chunk_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    commands.insert_resource(VoxelAtlas(TextureAtlasLayout::from_grid(
        UVec2::splat(ATLAS_SIZE),
        ATLAS_COLUMNS,
        ATLAS_ROWS,
        None,
        None,
    )));

    let texture = asset_server.load("blocks.png");

    let material = materials.add(StandardMaterial {
        base_color_texture: Some(texture),
        perceptual_roughness: 0.9,
        reflectance: 0.1,
        ..default()
    });

    commands.insert_resource(ChunkMaterial(material));
}

fn spawn_chunks_around_player(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    material: Res<ChunkMaterial>,
    player: Single<&Transform, With<Player>>,
) {
    let center = ChunkCoord::from_position(player.translation.x, player.translation.z);

    for dz in -CHUNK_LOAD_RADIUS..=CHUNK_LOAD_RADIUS {
        for dx in -CHUNK_LOAD_RADIUS..=CHUNK_LOAD_RADIUS {
            let coord = ChunkCoord::new(center.x + dx, center.z + dz);

            if world.is_loaded(coord) {
                continue;
            }

            let entity = world.spawn_chunk(&mut commands, coord);
            commands.entity(entity).insert(MeshMaterial3d(material.0.clone()));
        }
    }
}

fn generate_chunk(chunk: &mut ChunkData) {
    for z in 0..CHUNK_WIDTH {
        let world_z = chunk.coord.z * CHUNK_WIDTH as i32 + z as i32;

        for x in 0..CHUNK_WIDTH {
            let world_x = chunk.coord.x * CHUNK_WIDTH as i32 + x as i32;

            let height = (noise::opensimplex2s::noise2(0, world_x as f64 / 100.0, world_z as f64 / 100.0) * 10.0) as i32 + 20;
            let height = (height as usize).clamp(0, CHUNK_HEIGHT - 1);

            // Top layer is grass
            chunk[VoxelCoord::new(x, height, z)] = VoxelType::Grass;
            // Two layers of dirt
            for y in (height - 2).max(0)..height {
                chunk[VoxelCoord::new(x, y, z)] = VoxelType::Dirt;
            }
            // Below that is stone
            for y in 0..(height - 2).max(0) {
                chunk[VoxelCoord::new(x, y, z)] = VoxelType::Stone;
            }
        }
    }
}

/// Builds the mesh from owned data only, so it can run on another thread.
fn build_mesh(chunk: &ChunkData, neighbors: &ChunkNeighbors, atlas: &TextureAtlasLayout) -> Mesh {
    let mut mesh_data = MeshData::new(atlas);

    for (voxel_pos, voxel) in chunk.iter()
        .filter(|(_, v)| *v != VoxelType::Air)
    {
        for direction in Direction::iter() {
            if is_face_visible(chunk, neighbors, voxel_pos, direction) {
                let origin = voxel_pos.local_position();
                let texture = voxel.texture(direction);
                mesh_data.add_quad(direction, origin, texture);
            }
        }
    }

    mesh_data.build()
}

fn is_face_visible(chunk: &ChunkData, neighbors: &ChunkNeighbors, voxel_pos: VoxelCoord, direction: Direction) -> bool {
    if let Some(neighbor) = chunk.get_neighbor_voxel(voxel_pos, direction) {
        return neighbor.is_transparent();
    }

    // Chunks only exist horizontally, always show the top-most and bottom-most faces
    let Ok(compass) = direction.try_into() else {
        return true;
    };

    // If the chunk does not exist, we assume it is empty and show the face
    let Some(neighbor_chunk) = &neighbors[compass] else {
        return true;
    };

    let neighbor_pos = match compass {
        Compass::Front => VoxelCoord { z: 0, ..voxel_pos },
        Compass::Right  => VoxelCoord { x: 0, ..voxel_pos },
        Compass::Back => VoxelCoord { z: CHUNK_WIDTH - 1, ..voxel_pos },
        Compass::Left  => VoxelCoord { x: CHUNK_WIDTH - 1, ..voxel_pos },
    };

    neighbor_chunk[neighbor_pos]
        .is_transparent()
}

/// Applies the meshes of finished [`MeshTask`]s.
fn poll_mesh_tasks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tasks: Query<(Entity, &mut MeshTask, Option<&Mesh3d>)>,
) -> Result<(), BevyError> {
    for (entity, mut task, mesh3d) in &mut tasks {
        let Some(mesh) = block_on(future::poll_once(&mut task.0)) else {
            continue;
        };

        match mesh3d {
            Some(handle) => {
                meshes.insert(&handle.0, mesh)?;
            }
            None => {
                commands.entity(entity).insert(Mesh3d(meshes.add(mesh)));
            }
        }

        commands.entity(entity).remove::<MeshTask>();
    }

    Ok(())
}

/// Starts building the mesh of every chunk marked with [`NeedsRemesh`] on a background thread.
/// A chunk that is still being meshed stays marked, and is meshed again once that task is done.
fn remesh_chunks(
    mut commands: Commands,
    world: Res<VoxelWorld>,
    atlas: Res<VoxelAtlas>,
    chunks: Query<(Entity, &Chunk), (With<NeedsRemesh>, Without<MeshTask>)>,
) {
    let pool = AsyncComputeTaskPool::get();

    for (entity, chunk) in &chunks {
        let Some(data) = world.chunks.get(&chunk.coord) else {
            continue;
        };

        // The task cannot borrow the world, so it gets shared handles to the data it needs
        let data = Arc::clone(data);
        let neighbors = ChunkNeighbors::from(chunk.coord, &world.chunks);
        let atlas = atlas.0.clone();
        let task = pool.spawn(async move { build_mesh(&data, &neighbors, &atlas) });

        commands.entity(entity)
            .remove::<NeedsRemesh>()
            .insert(MeshTask(task));
    }
}

fn retire_distant_chunks(
    mut commands: Commands,
    player: Single<&Transform, With<Player>>,
    chunks: Query<(Entity, &Chunk, Has<NeedsDespawn>)>,
) {
    let center = ChunkCoord::from_position(player.translation.x, player.translation.z);

    for (entity, chunk, retired) in &chunks {
        let in_range = (chunk.coord.x - center.x).abs() <= CHUNK_LOAD_RADIUS
            && (chunk.coord.z - center.z).abs() <= CHUNK_LOAD_RADIUS;

        if !in_range && !retired {
            commands.entity(entity).insert(NeedsDespawn);
        } else if in_range && retired {
            commands.entity(entity).remove::<NeedsDespawn>();
        }
    }
}

fn despawn_retired_chunks(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    retired_chunks: Query<(Entity, &Chunk), With<NeedsDespawn>>,
) {
    for (entity, chunk) in &retired_chunks {
        info!("Despawning chunk ({}, {})", chunk.coord.x, chunk.coord.z);
        // Keep the voxel data; only the entity goes away
        if let Some(data) = world.chunks.get_mut(&chunk.coord) {
            Arc::make_mut(data).entity = None;
        }

        commands.entity(entity).despawn();
    }
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
    let velocity = Axis::get(axis, player.velocity);

    if velocity == 0.0 {
        return;
    }

    let axis_position = Axis::get(axis, *position) + velocity * dt;
    Axis::set(axis, position, axis_position);

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

fn draw_selected_voxel(
    mut gizmos: Gizmos,
    world: Res<VoxelWorld>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
) {
    let Some(hit) = raycast::find_voxel(
        &world,
        camera.translation(),
        camera.forward(),
        INTERACTION_DISTANCE,
    ) else {
        return;
    };

    let origin = hit.coord.position();
    let vertices = meshdata::quad_vertices(hit.direction, origin);

    gizmos.line(vertices[0], vertices[1], Color::WHITE);
    gizmos.line(vertices[1], vertices[3], Color::WHITE);
    gizmos.line(vertices[3], vertices[2], Color::WHITE);
    gizmos.line(vertices[2], vertices[0], Color::WHITE);
}

fn break_place_block(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut world: ResMut<VoxelWorld>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
) {
    let do_break = mouse.just_pressed(MouseButton::Left);
    let do_place = mouse.just_pressed(MouseButton::Right);
    if !(do_break || do_place) {
        return;
    }

    let origin = camera.translation();
    let look = camera.forward();

    if let Some(VoxelHit { coord, direction }) = raycast::find_voxel(&world, origin, look, INTERACTION_DISTANCE) {
        info!("Hit {:?} at {:?}", coord, direction);
        if do_break {
            world.set_voxel(&mut commands, coord, VoxelType::Air);
        } else /* if do_place */ {
            let neighbor = coord.shift(direction);
            world.set_voxel(&mut commands, neighbor, VoxelType::Dirt);
        }
    }
}
