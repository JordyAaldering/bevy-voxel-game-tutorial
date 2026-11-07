mod chunk_neighbors;
mod coords;
mod editor;
mod meshdata;
mod noise;
mod physics;
mod raycast;
mod screenshot;
mod telemetry;

use crate::{
    chunk_neighbors::ChunkNeighbors,
    coords::*,
    editor::{EditorPlugin, EditorState},
    meshdata::MeshData,
    noise::NoiseSettings,
    physics::PhysicsPlugin,
    raycast::VoxelHit,
    screenshot::ScreenshotPlugin,
    telemetry::TelemetryPlugin,
};

use std::{ops::{Index, IndexMut}, sync::Arc};

use better_default::Default;
use bevy::{
    prelude::*,
    diagnostic::{EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin, SystemInformationDiagnosticsPlugin},
    input::mouse::AccumulatedMouseMotion,
    platform::collections::HashMap,
    tasks::{AsyncComputeTaskPool, futures_lite::future, Task, block_on},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_inspector_egui::{
    prelude::*,
    bevy_egui::prelude::*,
};

pub const CHUNK_WIDTH: usize = 32;
pub const CHUNK_HEIGHT: usize = 64;

const CHUNK_LOAD_RADIUS: i32 = 2;

const ATLAS_ROWS: u32 = 4;
const ATLAS_COLUMNS: u32 = 4;

const PLAYER_EYE_HEIGHT: f32 = 1.6;
const MAX_PITCH: f32 = 1.4;

const INTERACTION_DISTANCE: f32 = 5.0;

#[derive(Clone, Resource)]
struct TerrainConfig {
    seed: i64,
    height: NoiseSettings,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            height: NoiseSettings {
                amplitude: 10.0,
                frequency: 0.01,
                bias: 20.0,
                ..default()
            },
        }
    }
}

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

#[derive(Component)]
struct EditorCamera;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

    fn is_solid(self) -> bool {
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

/// Marker component for chunks that need to be checked for generation progress.
/// This is added when the chunk's voxel data changes, and removed when a generation task is started.
/// It is also added when the neighboring chunks progress to the next phase, meaning that this chunk
/// may now be able to progress to the next phase as well.
#[derive(Component)]
struct NeedsGenerationCheck;

/// A chunk generation phase running on another thread. Dropping it cancels the task.
#[derive(Component)]
struct ChunkGenerationTask(Task<ChunkGenerationResult>);

enum ChunkGenerationResult {
    Height(Box<ChunkData>),
    Mesh(Mesh),
}

#[derive(Component)]
struct NeedsDespawn;

/// Event triggered when the voxel data of a chunk has changed, requiring a remesh.
#[derive(EntityEvent)]
struct ChunkDataChanged {
    entity: Entity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ChunkGenerationPhase {
    HeightGeneration,
    MeshGeneration,
    Done,
}

impl ChunkGenerationPhase {
    fn next(self) -> Self {
        match self {
            Self::HeightGeneration => Self::MeshGeneration,
            Self::MeshGeneration  => Self::Done,
            Self::Done => Self::Done,
        }
    }

    /// Which neighbors must have completed the previous phase before this phase can run.
    fn neighbor_requirement(self) -> NeighborRequirement {
        match self {
            Self::HeightGeneration => NeighborRequirement::NONE,
            // Only the chunks touching this one are needed to cull faces
            Self::MeshGeneration => NeighborRequirement { radius: 1, diagonal: false },
            Self::Done => NeighborRequirement::NONE,
        }
    }
}

/// The set of neighboring chunks that must be ready before a generation phase can run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct NeighborRequirement {
    /// How many chunks away to check, 0 meaning only the chunk itself.
    radius: i32,
    /// Whether to also check chunks that are not in the same row or column.
    diagonal: bool,
}

impl NeighborRequirement {
    const NONE: Self = Self { radius: 0, diagonal: false };

    fn offsets(self) -> impl Iterator<Item = (i32, i32)> {
        let radius = self.radius;
        (-radius..=radius)
            .flat_map(move |dz| (-radius..=radius).map(move |dx| (dx, dz)))
            .filter(move |&(dx, dz)| (dx, dz) != (0, 0) && (self.diagonal || dx == 0 || dz == 0))
    }
}

#[derive(Clone)]
pub struct ChunkData {
    coord: ChunkCoord,
    voxels: [VoxelType; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH],
    entity: Option<Entity>,
    generation_phase: ChunkGenerationPhase,
}

impl ChunkData {
    fn new(coord: ChunkCoord, entity: Entity) -> Self {
        Self {
            coord,
            voxels: [VoxelType::Air; _],
            entity: Some(entity),
            generation_phase: ChunkGenerationPhase::HeightGeneration,
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
    /// Chunk data is shared with generation tasks. Writes use copy-on-write.
    chunks: HashMap<ChunkCoord, Arc<ChunkData>>,
}

#[derive(Resource)]
struct VoxelAtlas(TextureAtlasLayout);

impl VoxelWorld {
    fn add_chunk(&mut self, pos: ChunkCoord, chunk: ChunkData) {
        debug_assert!(!self.chunks.contains_key(&pos));
        self.chunks.insert(pos, Arc::new(chunk));
    }

    fn get_chunk(&self, pos: ChunkCoord) -> Option<&ChunkData> {
        self.chunks.get(&pos).map(Arc::as_ref)
    }

    fn is_solid(&self, pos: WorldCoord) -> bool {
        self.get_voxel(pos)
            .is_some_and(VoxelType::is_solid)
    }

    fn is_loaded(&self, coord: ChunkCoord) -> bool {
        self.chunks.get(&coord)
            .is_some_and(|chunk| chunk.entity.is_some())
    }

    /// Spawns an entity for the chunk at `coord`, creating data if it does not exist.
    fn spawn_chunk(&mut self, commands: &mut Commands, coord: ChunkCoord) -> Entity {
        let entity = commands.spawn((
            Name::new(format!("Chunk {coord}")),
            Transform::from_translation(coord.position()),
            Chunk { coord },
        )).id();

        if let Some(chunk) = self.chunks.get_mut(&coord) {
            Arc::make_mut(chunk).set_entity(entity);
        } else {
            let chunk = ChunkData::new(coord, entity);
            self.add_chunk(coord, chunk);
        }

        commands.trigger(ChunkDataChanged { entity });

        entity
    }

    fn get_voxel(&self, pos: WorldCoord) -> Option<VoxelType> {
        if !pos.in_bounds() {
            return None;
        }

        let chunk_pos = pos.chunk_coord();
        self.get_chunk(chunk_pos)
            .filter(|chunk| chunk.generation_phase > ChunkGenerationPhase::HeightGeneration)
            .map(|chunk| {
                let voxel_pos = pos.local_coord();
                chunk[voxel_pos]
            })
    }

    fn set_voxel(&mut self, commands: &mut Commands, pos: WorldCoord, voxel: VoxelType) {
        let voxel_pos = pos.local_coord();
        let chunk_pos = pos.chunk_coord();
        let chunk = self.chunks.get_mut(&chunk_pos).unwrap();
        Arc::make_mut(chunk)[voxel_pos] = voxel;

        self.notify_chunk(commands, chunk_pos);

        // Voxels on a chunk border influence the faces of the adjacent chunk
        if voxel_pos.x == 0 {
            self.notify_chunk(commands, chunk_pos + Compass::Left);
        } else if voxel_pos.x == CHUNK_WIDTH - 1 {
            self.notify_chunk(commands, chunk_pos + Compass::Right);
        }
        if voxel_pos.z == 0 {
            self.notify_chunk(commands, chunk_pos + Compass::Back);
        } else if voxel_pos.z == CHUNK_WIDTH - 1 {
            self.notify_chunk(commands, chunk_pos + Compass::Front);
        }
    }

    fn get_chunk_neighbor(&self, pos: ChunkCoord, dx: i32, dz: i32) -> Option<&ChunkData> {
        let neighbor_pos = ChunkCoord::new(pos.x + dx, pos.z + dz);
        self.chunks.get(&neighbor_pos).map(Arc::as_ref)
    }

    /// Whether all neighbors required by `phase` have completed the phase before it.
    fn neighbors_ready_for(&self, coord: ChunkCoord, phase: ChunkGenerationPhase) -> bool {
        phase.neighbor_requirement()
            .offsets()
            .all(|(dx, dz)| {
                self.get_chunk_neighbor(coord, dx, dz)
                    .is_some_and(|neighbor| neighbor.generation_phase >= phase)
            })
    }

    /// Notifies the chunk at `coord` that its, or its neighbors' voxel data has changed.
    /// This triggers the chunk to advance to the next generation phase, if applicable.
    fn notify_chunk(&self, commands: &mut Commands, coord: ChunkCoord) {
        if let Some(entity) = self.get_chunk(coord).and_then(|chunk| chunk.entity) {
            info!("Notifying chunk {} of voxel data change", coord);
            commands.trigger(ChunkDataChanged { entity });
        }
    }

    fn notify_neighbors(&self, commands: &mut Commands, coord: ChunkCoord) {
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            self.notify_chunk(commands, ChunkCoord::new(coord.x + dx, coord.z + dz));
        }
    }
}

impl Index<&Chunk> for VoxelWorld {
    type Output = ChunkData;

    fn index(&self, chunk: &Chunk) -> &Self::Output {
        self.chunks.get(&chunk.coord)
            .expect("A chunk entity cannot exist without its data")
    }
}

impl IndexMut<&Chunk> for VoxelWorld {
    fn index_mut(&mut self, chunk: &Chunk) -> &mut Self::Output {
        Arc::make_mut(
            self.chunks.get_mut(&chunk.coord)
                .expect("A chunk entity cannot exist without its data"),
        )
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest()),
            EguiPlugin::default(),
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
            SystemInformationDiagnosticsPlugin::default(),
            ScreenshotPlugin::default(),
            PhysicsPlugin::new(is_gamemode_survival),
            TelemetryPlugin,
            EditorPlugin,
        ))
        .init_resource::<GameMode>()
        .init_resource::<VoxelWorld>()
        .init_resource::<TerrainConfig>()
        .init_resource::<CameraSettings>()
        .register_type::<CameraSettings>()
        .init_resource::<DebugGizmos>()
        .register_type::<DebugGizmos>()
        .add_observer(on_chunk_data_changed)
        .add_systems(Startup, (
            setup_chunk_material,
            spawn_global_illumination,
            spawn_player,
        ))
        .add_systems(Update, (
            toggle_cursor,
            draw_selected_voxel,
            (
                toggle_gamemode,
                player_look,
                break_place_block,
                move_survival.run_if(is_gamemode_survival),
                move_spectator.run_if(is_gamemode_spectator),
            )
            .run_if(game_view_captured),
            (
                spawn_chunks_around_player,
                retire_distant_chunks,
            ).chain(),
        ))
        .add_systems(PostUpdate, (
            (
                despawn_retired_chunks,
                poll_chunk_generation_tasks,
                start_chunk_generation_tasks,
            ).chain(),
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

fn spawn_player(
    mut commands: Commands,
    editor_state: Res<EditorState>,
    config: Res<TerrainConfig>,
) {
    let world_height = terrain_height(&config, 0, 0) as f32 + 1.0;

    commands.spawn((
            Name::new("Player"),
            Player::default(),
            Projection::Perspective(PerspectiveProjection {
                fov: 0.9,
                ..default()
            }),
            Msaa::Off,
            InheritedVisibility::default(),
            Transform::from_xyz(1.5, world_height as f32 + 1.0, 1.5),
        ))
        .with_children(|commands| {
            commands.spawn((
                PlayerCamera,
                Camera3d::default(),
                editor_state.game_camera_target(),
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
        UVec2::splat(ATLAS_COLUMNS * ATLAS_ROWS),
        ATLAS_COLUMNS,
        ATLAS_ROWS,
        None,
        None,
    )));

    let texture = asset_server.load("spritesheet.png");

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

fn terrain_height(config: &TerrainConfig, world_x: i32, world_z: i32) -> usize {
    let height = config.height.sample2(config.seed, world_x as f32, world_z as f32) as i32;
    (height as usize).clamp(0, CHUNK_HEIGHT - 1)
}

fn generate_chunk_height(chunk: &mut ChunkData, config: &TerrainConfig) {
    let heights = config.height.area2(config.seed, chunk.coord.x, chunk.coord.z);
    for x in 0..CHUNK_WIDTH {
        for z in 0..CHUNK_WIDTH {
            let height = (heights[x + z * CHUNK_WIDTH] as i32 as usize).clamp(0, CHUNK_HEIGHT - 1);

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

fn build_mesh(
    chunk: &ChunkData,
    neighbors: &ChunkNeighbors,
    atlas: &TextureAtlasLayout,
) -> Mesh {
    let mut mesh_data = MeshData::new(atlas);

    for (voxel_pos, voxel) in chunk.iter()
        .filter(|(_, v)| *v != VoxelType::Air)
    {
        for direction in Direction::iter() {
            if is_face_visible(neighbors, chunk, voxel_pos, direction) {
                let origin = voxel_pos.local_position();
                let texture = voxel.texture(direction);
                mesh_data.add_quad(direction, origin, texture);
            }
        }
    }

    mesh_data.build()
}

fn is_face_visible(
    neighbors: &ChunkNeighbors,
    chunk: &ChunkData,
    voxel_pos: VoxelCoord,
    direction: Direction,
) -> bool {
    if let Some(neighbor) = chunk.get_neighbor_voxel(voxel_pos, direction) {
        return neighbor.is_transparent();
    }

    // Chunks only exist horizontally, always show the top-most but never the bottom-most faces
    let Ok(compass) = direction.try_into() else {
        return direction == Direction::Top;
    };

    let neighbor_chunk = neighbors[compass].as_ref()
        .expect("Mesh generation requires all neighboring chunk data");

    let neighbor_pos = match compass {
        Compass::Front => VoxelCoord { z: 0, ..voxel_pos },
        Compass::Right => VoxelCoord { x: 0, ..voxel_pos },
        Compass::Back => VoxelCoord { z: CHUNK_WIDTH - 1, ..voxel_pos },
        Compass::Left => VoxelCoord { x: CHUNK_WIDTH - 1, ..voxel_pos },
    };

    neighbor_chunk[neighbor_pos]
        .is_transparent()
}

fn on_chunk_data_changed(
    event: On<ChunkDataChanged>,
    chunks: Query<&Chunk>,
    world: Res<VoxelWorld>,
    mut commands: Commands,
) {
    // May be None if the chunk was despawned before the event was processed
    let Ok(chunk) = chunks.get(event.entity) else {
        return;
    };

    // Ignore events for entities that have since been replaced by a reloaded chunk.
    if world[&chunk].entity == Some(event.entity) {
        commands.entity(event.entity).insert(NeedsGenerationCheck);
    }
}

fn poll_chunk_generation_tasks(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut tasks: Query<(Entity, &Chunk, &mut ChunkGenerationTask, Option<&Mesh3d>)>,
) -> Result<(), BevyError> {
    for (entity, chunk, mut task, mesh3d) in &mut tasks {
        let Some(result) = block_on(future::poll_once(&mut task.0)) else {
            continue;
        };

        match result {
            ChunkGenerationResult::Height(generated) => {
                let data = &mut world[chunk];
                assert_eq!(data.generation_phase, ChunkGenerationPhase::HeightGeneration);
                data.voxels = generated.voxels;
                data.generation_phase = data.generation_phase.next();
                world.notify_chunk(&mut commands, chunk.coord);
                world.notify_neighbors(&mut commands, chunk.coord);
            }
            ChunkGenerationResult::Mesh(mesh) => {
                match mesh3d {
                    Some(handle) => {
                        meshes.insert(&handle.0, mesh)?;
                    }
                    None => {
                        commands.entity(entity).insert(Mesh3d(meshes.add(mesh)));
                    }
                }

                let data = &mut world[chunk];
                assert!(data.generation_phase >= ChunkGenerationPhase::MeshGeneration);
                data.generation_phase = data.generation_phase.next();
            }
        }

        commands.entity(entity).remove::<ChunkGenerationTask>();
    }

    Ok(())
}

fn start_chunk_generation_tasks(
    mut commands: Commands,
    world: Res<VoxelWorld>,
    config: Res<TerrainConfig>,
    atlas: Res<VoxelAtlas>,
    chunks: Query<(Entity, &Chunk), (With<NeedsGenerationCheck>, Without<ChunkGenerationTask>)>,
) {
    let pool = AsyncComputeTaskPool::get();

    for (entity, chunk) in &chunks {
        let data = &world.chunks[&chunk.coord];

        let phase = data.generation_phase;
        commands.entity(entity).remove::<NeedsGenerationCheck>();

        let task = match phase {
            ChunkGenerationPhase::HeightGeneration => {
                let data = Arc::clone(data);
                let config = config.clone();
                info!("Generating height data for chunk {}", chunk.coord);
                pool.spawn(async move {
                    let mut generated = Box::new((*data).clone());
                    generate_chunk_height(&mut generated, &config);
                    ChunkGenerationResult::Height(generated)
                })
            }
            ChunkGenerationPhase::MeshGeneration | ChunkGenerationPhase::Done => {
                if !world.neighbors_ready_for(chunk.coord, ChunkGenerationPhase::MeshGeneration) {
                    continue;
                }

                let data = Arc::clone(data);
                let neighbors = ChunkNeighbors::from(chunk.coord, &world.chunks);
                let atlas = atlas.0.clone();
                info!("Generating mesh for chunk {}", chunk.coord);
                pool.spawn(async move {
                    ChunkGenerationResult::Mesh(build_mesh(&data, &neighbors, &atlas))
                })
            }
        };

        commands.entity(entity).insert(ChunkGenerationTask(task));
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

fn is_gamemode_survival(mode: Res<GameMode>) -> bool {
    *mode == GameMode::Survival
}

fn is_gamemode_spectator(mode: Res<GameMode>) -> bool {
    *mode == GameMode::Spectator
}

/// The game view only receives input while it has captured the cursor.
fn game_view_captured(
    editor_state: Res<EditorState>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
) -> bool {
    editor_state.is_game_view_active()
        && editor_state.is_game_view_visible()
        && cursor.grab_mode == CursorGrabMode::Locked
}

fn toggle_cursor(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    editor_state: Res<EditorState>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    // Release the cursor when escape is pressed, or when the game view is no longer the active tab
    if keyboard.just_pressed(KeyCode::Escape)
        || (
            cursor.grab_mode != CursorGrabMode::None
            && !(editor_state.is_game_view_active() && editor_state.is_game_view_visible())
        )
    {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }

    // Capture only when clicking inside the game view
    if mouse.just_pressed(MouseButton::Left)
        && editor_state.is_game_view_active()
        && window.cursor_position().is_some_and(|position| editor_state.game_view_contains(position))
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
            if world.get_voxel(neighbor).is_some() {
                world.set_voxel(&mut commands, neighbor, VoxelType::Dirt);
            }
        }
    }
}
