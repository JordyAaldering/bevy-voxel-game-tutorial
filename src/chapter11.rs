mod coords;
mod meshdata;
mod opensimplex2s;
mod telemetry;

use crate::{
    axis::Axis,
    coords::*,
    meshdata::MeshData,
    telemetry::{Telemetry, TelemetryPublisher},
};

use std::{ops::{Index, IndexMut}, time::Duration};

use better_default::Default;
use bevy::{
    prelude::*,
    diagnostic::{EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin, SystemInformationDiagnosticsPlugin},
    input::mouse::AccumulatedMouseMotion,
    platform::collections::HashMap,
    time::common_conditions::on_timer,
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

#[derive(Default, Resource, Reflect, InspectorOptions)]
#[reflect(Resource, InspectorOptions)]
struct DebugGizmos {
    #[inspector()]
    chunk_borders: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VoxelType {
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

#[derive(Debug)]
struct VoxelHit {
    coord: WorldCoord,
    direction: Direction,
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

struct ChunkData {
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

    fn build_mesh(&self, world: &VoxelWorld, atlas: &TextureAtlasLayout) -> Mesh {
        let mut mesh_data = MeshData::new(atlas);

        for (voxel_pos, voxel) in self.iter()
            .filter(|(_, voxel)| *voxel != VoxelType::Air)
        {
            for direction in Direction::iter() {
                if let Some(neighbor) = self.get_neighbor_voxel(voxel_pos, direction) {
                    if !neighbor.is_transparent() {
                        continue;
                    }
                } else {
                    // If get_neighbor_voxel returns None, the voxel exists in a neighboring chunk
                    let offset = match direction {
                        Direction::Right => Some((1, 0)),
                        Direction::Left => Some((-1, 0)),
                        Direction::Front => Some((0, 1)),
                        Direction::Back => Some((0, -1)),
                        _ => None,
                    };

                    if let Some((dx, dz)) = offset &&
                        let Some(neighbor_chunk) = world.get_chunk_neighbor(self.coord, dx, dz)
                    {
                        let VoxelCoord { x, y, z } = voxel_pos;

                        let (nx, ny, nz) = match direction {
                            Direction::Right => (0, y, z),
                            Direction::Left => (CHUNK_WIDTH - 1, y, z),
                            Direction::Front => (x, y, 0),
                            Direction::Back => (x, y, CHUNK_WIDTH - 1),
                            _ => unreachable!(),
                        };

                        let voxel_pos = VoxelCoord::new(nx, ny, nz);
                        let neighbor = neighbor_chunk[voxel_pos];
                        if !neighbor.is_transparent() {
                            continue;
                        }
                    }
                }

                let origin = vec3(voxel_pos.x as f32, voxel_pos.y as f32, voxel_pos.z as f32);
                let texture = voxel.texture(direction);
                mesh_data.add_quad(direction, origin, texture);
            }
        }

        mesh_data.build()
    }

    fn set_entity(&mut self, entity: Entity) {
        debug_assert!(self.entity.is_none());
        self.entity = Some(entity);
    }

    fn get_neighbor_voxel(&self, pos: VoxelCoord, direction: Direction) -> Option<VoxelType> {
        let VoxelCoord { x, y, z } = pos;

        // Using `wrapping_sub` ensures that the value becomes `usize::MAX`,
        // which is definitely greather than the width and height.
        let (x, y, z) = match direction {
            Direction::Right => (x + 1, y, z),
            Direction::Left => (x.wrapping_sub(1), y, z),
            Direction::Top => (x, y + 1, z),
            Direction::Bottom => (x, y.wrapping_sub(1), z),
            Direction::Front => (x, y, z + 1),
            Direction::Back => (x, y, z.wrapping_sub(1)),
        };

        if x < CHUNK_WIDTH && y < CHUNK_HEIGHT && z < CHUNK_WIDTH {
            Some(self[VoxelCoord::new(x, y, z)])
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
            chunk.set_entity(entity);
        } else {
            let mut chunk = ChunkData::new(coord, entity);
            generate_chunk(&mut chunk);
            self.chunks.insert(coord, chunk);
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
        chunk[voxel_pos] = voxel;

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
        self.chunks.get(&neighbor_pos)
    }

    fn is_loaded(&self, coord: ChunkCoord) -> bool {
        self.chunks.get(&coord)
            .is_some_and(|chunk| chunk.entity.is_some())
    }
}

fn main() -> AppExit {
    let (sender, receiver) = tokio::sync::watch::channel(Telemetry::default());

    // Run Tokio on its own thread, independently of the game
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(telemetry::run_server(receiver));
    });

    App::new()
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest()),
            EguiPlugin::default(),
            WorldInspectorPlugin::default(),
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
            SystemInformationDiagnosticsPlugin::default(),
        ))
        .init_resource::<GameMode>()
        .init_resource::<VoxelWorld>()
        .init_resource::<CameraSettings>()
        .register_type::<CameraSettings>()
        .init_resource::<DebugGizmos>()
        .register_type::<DebugGizmos>()
        .insert_resource(TelemetryPublisher(sender))
        .add_systems(Startup, (
            setup_chunk_material,
            spawn_global_illumination,
            spawn_player,
        ))
        .add_systems(Update, (
            telemetry::collect_telemetry
                .run_if(on_timer(Duration::from_millis(250))),
            toggle_cursor,
            (toggle_gamemode, player_look, break_place_block)
                .run_if(is_focused),
            move_survival
                .run_if(is_focused.and_then(is_gamemode_survival)),
            move_spectator
                .run_if(is_focused.and_then(is_gamemode_spectator)),
            draw_selected_voxel,
            spawn_chunks_around_player,
            retire_distant_chunks,
        ))
        .add_systems(PostUpdate, (
            // TODO: check if we need a constraint between these
            despawn_retired_chunks,
            remesh_chunks,
        ))
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
    let world_height = opensimplex2s::noise2d(0, 0.0, 0.0) * 10.0 + 20.0;

    commands.spawn((
            Name::new("Player"),
            Player::default(),
            Transform::from_xyz(1.5, world_height as f32 + 1.0, 1.5),
            InheritedVisibility::default(),
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
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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
    for x in 0..CHUNK_WIDTH {
        let world_x = chunk.coord.x * CHUNK_WIDTH as i32 + x as i32;

        for z in 0..CHUNK_WIDTH {
            let world_z = chunk.coord.z * CHUNK_WIDTH as i32 + z as i32;

            let height = (opensimplex2s::noise2d(0, world_x as f64 / 100.0, world_z as f64 / 100.0) * 10.0) as i32 + 20;
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

fn remesh_chunks(
    mut commands: Commands,
    world: Res<VoxelWorld>,
    atlas: Res<VoxelAtlas>,
    mut meshes: ResMut<Assets<Mesh>>,
    chunks: Query<(Entity, &Chunk, Option<&Mesh3d>), With<NeedsRemesh>>,
) {
    for (entity, chunk, mesh3d) in &chunks {
        let Some(data) = world.chunks.get(&chunk.coord) else {
            continue;
        };

        let mesh = data.build_mesh(&world, &atlas.0);

        match mesh3d {
            Some(handle) => {
                let _ = meshes.insert(&handle.0, mesh);
            }
            None => {
                commands.entity(entity).insert(Mesh3d(meshes.add(mesh)));
            }
        }
        commands.entity(entity).remove::<NeedsRemesh>();
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
            data.entity = None;
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
    let Some(hit) = find_voxel(
        &world,
        camera.translation(),
        *camera.forward(),
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
    let direction = camera.forward();

    if let Some(VoxelHit { coord, direction }) = find_voxel(&world, origin, *direction) {
        info!("Hit {:?} at {:?}", coord, direction);
        if do_break {
            world.set_voxel(&mut commands, coord, VoxelType::Air);
        } else /* if do_place */ {
            let neighbor = coord.shift(direction);
            world.set_voxel(&mut commands, neighbor, VoxelType::Dirt);
        }
    }
}

fn find_voxel(world: &VoxelWorld, origin: Vec3, direction: Vec3) -> Option<VoxelHit> {
    let direction = direction.normalize();

    let mut x = origin.x.floor() as i32;
    let mut y = origin.y.floor() as i32;
    let mut z = origin.z.floor() as i32;

    let step_x = direction.x.signum() as i32;
    let step_y = direction.y.signum() as i32;
    let step_z = direction.z.signum() as i32;

    let delta_x = if direction.x != 0.0 {
        (1.0 / direction.x).abs()
    } else {
        f32::INFINITY
    };

    let delta_y = if direction.y != 0.0 {
        (1.0 / direction.y).abs()
    } else {
        f32::INFINITY
    };

    let delta_z = if direction.z != 0.0 {
        (1.0 / direction.z).abs()
    } else {
        f32::INFINITY
    };

    let mut max_x = initial_axis_distance(origin.x, direction.x);
    let mut max_y = initial_axis_distance(origin.y, direction.y);
    let mut max_z = initial_axis_distance(origin.z, direction.z);

    loop {
        let (distance, face) = if max_x <= max_y && max_x <= max_z {
            x += step_x;

            let distance = max_x;
            max_x += delta_x;

            let direction = if step_x > 0 {
                Direction::Left
            } else {
                Direction::Right
            };

            (distance, direction)
        } else if max_y <= max_z {
            y += step_y;

            let distance = max_y;
            max_y += delta_y;

            let direction = if step_y > 0 {
                Direction::Bottom
            } else {
                Direction::Top
            };

            (distance, direction)
        } else {
            z += step_z;

            let distance = max_z;
            max_z += delta_z;

            let direction = if step_z > 0 {
                Direction::Back
            } else {
                Direction::Front
            };

            (distance, direction)
        };

        if distance > INTERACTION_DISTANCE {
            return None;
        }

        let coord = WorldCoord::new(x, y, z);
        if world.is_solid(coord) {
            return Some(VoxelHit {
                coord,
                direction: face,
            });
        }
    }
}

/// Returns the distance until the ray crosses the next voxel boundary on this axis.
/// This is needed to determine which neighboring voxel to visit next.
fn initial_axis_distance(origin: f32, direction: f32) -> f32 {
    if direction > 0.0 {
        (origin.ceil() - origin) / direction
    } else if direction < 0.0 {
        (origin.floor() - origin) / direction
    } else {
        f32::INFINITY
    }
}
