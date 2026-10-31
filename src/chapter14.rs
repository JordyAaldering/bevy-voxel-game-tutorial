mod coords;
mod editor;
mod meshdata;
mod opensimplex2s;
mod physics;
mod raycast;
mod telemetry;

use crate::{
    coords::*,
    editor::{EditorPlugin, EditorState},
    meshdata::MeshData,
    physics::PhysicsPlugin,
    raycast::VoxelHit,
    telemetry::{Telemetry, TelemetryPublisher},
};

use std::{fs, io, ops::{Index, IndexMut}, path::{Path, PathBuf}, time::Duration};

use better_default::Default;
use bevy::{
    prelude::*,
    app::OnAppExitSystems,
    diagnostic::{EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin, SystemInformationDiagnosticsPlugin},
    input::mouse::AccumulatedMouseMotion,
    platform::collections::HashMap,
    time::common_conditions::on_timer,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_inspector_egui::{
    prelude::*,
    bevy_egui::prelude::*,
};
use serde::{Deserialize, Serialize};
use wincode::{SchemaRead, SchemaWrite};

const CHUNK_WIDTH: usize = 32;
const CHUNK_HEIGHT: usize = 64;

const CHUNK_LOAD_RADIUS: i32 = 2;

const ATLAS_ROWS: u32 = 4;
const ATLAS_COLUMNS: u32 = 4;
const ATLAS_SIZE: u32 = ATLAS_COLUMNS * ATLAS_ROWS;

const PLAYER_EYE_HEIGHT: f32 = 1.6;
const MAX_PITCH: f32 = 1.4;

const INTERACTION_DISTANCE: f32 = 5.0;

const SAVE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/savedata/world");
const SETTINGS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/savedata/settings");

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

#[derive(Default, Resource, Reflect, InspectorOptions, Serialize, Deserialize)]
#[reflect(Resource, InspectorOptions)]
#[serde(default)]
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

impl CameraSettings {
    fn path() -> PathBuf {
        Path::new(SETTINGS_DIR).join("camera.json")
    }

    fn save(&self) {
        let result = serde_json::to_string_pretty(self)
            .map_err(|e| e.to_string())
            .and_then(|json| write_atomic(&Self::path(), json.as_bytes()).map_err(|e| e.to_string()));
        if let Err(error) = result {
            error!("Failed to save camera settings: {error}");
        }
    }

    /// Falls back to the defaults if the file is missing or invalid.
    fn load() -> Self {
        let json = match fs::read_to_string(Self::path()) {
            Ok(json) => json,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                error!("Failed to read camera.json: {error}");
                return Self::default();
            }
        };

        serde_json::from_str(&json).unwrap_or_else(|error| {
            error!("Failed to parse camera.json: {error}");
            Self::default()
        })
    }
}

#[derive(Default, Resource, Reflect, InspectorOptions)]
#[reflect(Resource, InspectorOptions)]
struct DebugGizmos {
    #[inspector()]
    chunk_borders: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SchemaWrite, SchemaRead)]
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

/// Event triggered when the voxel data of a chunk has changed, requiring a remesh.
#[derive(EntityEvent)]
struct ChunkDataChanged {
    entity: Entity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, SchemaWrite, SchemaRead)]
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
}

#[derive(SchemaWrite, SchemaRead)]
struct ChunkData {
    coord: ChunkCoord,
    voxels: [VoxelType; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH],
    #[wincode(skip)]
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

    fn path(coord: ChunkCoord) -> PathBuf {
        Path::new(SAVE_DIR).join(format!("chunk_{}_{}.bin", coord.x, coord.z))
    }

    fn save(&self) {
        let result = wincode::serialize(self)
            .map_err(|e| e.to_string())
            .and_then(|bytes| write_atomic(&Self::path(self.coord), &bytes).map_err(|e| e.to_string()));
        if let Err(error) = result {
            error!("Failed to save chunk {}: {error}", self.coord);
        }
    }

    /// Returns `None` if there is no saved chunk, or it could not be read.
    fn load(coord: ChunkCoord, entity: Entity) -> Option<Self> {
        let bytes = match fs::read(Self::path(coord)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(error) => {
                error!("Failed to read chunk {coord}: {error}");
                return None;
            }
        };

        match wincode::deserialize::<Self>(&bytes) {
            Ok(mut chunk) if chunk.coord == coord => {
                chunk.entity = Some(entity);
                // Meshes are not saved, so a loaded chunk always has to be meshed again
                chunk.generation_phase = chunk.generation_phase.min(ChunkGenerationPhase::MeshGeneration);
                Some(chunk)
            }
            Ok(_) => {
                error!("Saved chunk file for {coord} contains a different chunk");
                None
            }
            Err(error) => {
                error!("Failed to decode chunk {coord}: {error}");
                None
            }
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
    chunks: HashMap<ChunkCoord, ChunkData>,
}

/// Global world state, stored in `world.bin`.
#[derive(SchemaWrite, SchemaRead)]
struct WorldSave {
    player_position: [f32; 3],
    player_rotation: [f32; 4],
    camera_rotation: [f32; 4],
    player_velocity: [f32; 3],
}

impl WorldSave {
    fn path() -> PathBuf {
        Path::new(SAVE_DIR).join("world.bin")
    }

    fn save(&self) {
        let result = wincode::serialize(self)
            .map_err(|e| e.to_string())
            .and_then(|bytes| write_atomic(&Self::path(), &bytes).map_err(|e| e.to_string()));
        if let Err(error) = result {
            error!("Failed to save world: {error}");
        }
    }

    fn load() -> Option<Self> {
        let bytes = fs::read(Self::path()).ok()?;
        wincode::deserialize(&bytes)
            .inspect_err(|e| error!("Failed to decode world.bin: {e}"))
            .ok()
    }
}

/// Writes to a temporary file first, so a crash cannot leave a half-written save.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

#[derive(Resource)]
struct VoxelAtlas(TextureAtlasLayout);

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

    fn is_loaded(&self, coord: ChunkCoord) -> bool {
        self.chunks.get(&coord)
            .is_some_and(|chunk| chunk.entity.is_some())
    }

    /// Spawns an entity for the chunk at `coord`, creating data if it does not exist.
    fn spawn_chunk(&mut self, commands: &mut Commands, coord: ChunkCoord) -> Entity {
        let entity = commands.spawn((
            Name::new(format!("Chunk ({}, {})", coord.x, coord.z)),
            Transform::from_translation(coord.position()),
            Chunk { coord },
        )).id();

        if let Some(chunk) = self.chunks.get_mut(&coord) {
            chunk.set_entity(entity);
        } else {
            let chunk = ChunkData::load(coord, entity)
                .unwrap_or_else(|| ChunkData::new(coord, entity));
            self.add_chunk(coord, chunk);
        }

        commands.trigger(ChunkDataChanged { entity });
        // Loaded chunks skip height generation, which is what normally notifies neighbors.
        // Neighbors waiting for this chunk to become available need to know it now exists.
        self.notify_neighbors(commands, coord);

        entity
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

    fn set_voxel(&mut self, commands: &mut Commands, pos: WorldCoord, voxel: VoxelType) {
        let voxel_pos = pos.local_coord();
        let chunk_pos = pos.chunk_coord();
        let chunk = self.chunks.get_mut(&chunk_pos).unwrap();
        chunk[voxel_pos] = voxel;

        self.notify_chunk(commands, chunk_pos);

        // Voxels on a chunk border influence the faces of the adjacent chunk
        if voxel_pos.x == 0 {
            self.notify_chunk(commands, ChunkCoord::new(chunk_pos.x - 1, chunk_pos.z));
        } else if voxel_pos.x == CHUNK_WIDTH - 1 {
            self.notify_chunk(commands, ChunkCoord::new(chunk_pos.x + 1, chunk_pos.z));
        }
        if voxel_pos.z == 0 {
            self.notify_chunk(commands, ChunkCoord::new(chunk_pos.x, chunk_pos.z - 1));
        } else if voxel_pos.z == CHUNK_WIDTH - 1 {
            self.notify_chunk(commands, ChunkCoord::new(chunk_pos.x, chunk_pos.z + 1));
        }
    }

    fn get_chunk_neighbor(&self, pos: ChunkCoord, dx: i32, dz: i32) -> Option<&ChunkData> {
        let neighbor_pos = ChunkCoord::new(pos.x + dx, pos.z + dz);
        self.chunks.get(&neighbor_pos)
    }

    fn neighbors_ready_for_mesh(&self, coord: ChunkCoord) -> bool {
        [(-1, 0), (1, 0), (0, -1), (0, 1)]
            .into_iter()
            .all(|(dx, dz)| {
                self.get_chunk_neighbor(coord, dx, dz)
                    .is_some_and(|neighbor| {
                        neighbor.generation_phase >= ChunkGenerationPhase::MeshGeneration
                    })
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
        self.chunks.get_mut(&chunk.coord)
            .expect("A chunk entity cannot exist without its data")
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
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
            SystemInformationDiagnosticsPlugin::default(),
            PhysicsPlugin,
            EditorPlugin,
        ))
        .init_resource::<GameMode>()
        .init_resource::<VoxelWorld>()
        .insert_resource(CameraSettings::load())
        .register_type::<CameraSettings>()
        .init_resource::<DebugGizmos>()
        .register_type::<DebugGizmos>()
        .insert_resource(TelemetryPublisher(sender))
        .add_observer(on_chunk_data_changed)
        .add_systems(Startup, (
            setup_chunk_material,
            spawn_global_illumination,
            spawn_player,
        ))
        .add_systems(Update, (
            telemetry::collect_telemetry
                .run_if(on_timer(Duration::from_millis(250))),
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
                remesh_chunks,
            ).chain(),
        ))
        .add_systems(Last, save_on_exit.in_set(OnAppExitSystems))
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
) {
    let world_height = opensimplex2s::noise2d(0, 0.0, 0.0) * 10.0 + 20.0;
    let save = WorldSave::load();
    let position = save
        .as_ref()
        .map(|save| Vec3::from(save.player_position))
        .unwrap_or(Vec3::new(1.5, world_height as f32 + 1.0, 1.5));
    let rotation = save
        .as_ref()
        .map(|save| Quat::from_array(save.player_rotation))
        .unwrap_or(Quat::IDENTITY);
    let camera_rotation = save
        .as_ref()
        .map(|save| Quat::from_array(save.camera_rotation))
        .unwrap_or(Quat::IDENTITY);
    let velocity = save
        .map(|save| Vec3::from(save.player_velocity))
        .unwrap_or(Vec3::ZERO);

    commands.spawn((
            Name::new("Player"),
            Player { velocity, ..default() },
            Transform { translation: position, rotation, ..default() },
            InheritedVisibility::default(),
        ))
        .with_children(|commands| {
            commands.spawn((
                PlayerCamera,
                Camera3d::default(),
                editor_state.game_camera_target(),
                Transform {
                    translation: Vec3::new(0.0, PLAYER_EYE_HEIGHT, 0.0),
                    rotation: camera_rotation,
                    ..default()
                },
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

fn generate_chunk_height(chunk: &mut ChunkData) {
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
    let neighbor_chunk = world.get_chunk(chunk_pos + compass)
        .expect("Mesh generation requires all neighboring chunk data");

    let neighbor_pos = match compass {
        Compass::Front => VoxelCoord { z: 0, ..voxel_pos },
        Compass::Right  => VoxelCoord { x: 0, ..voxel_pos },
        Compass::Back => VoxelCoord { z: CHUNK_WIDTH - 1, ..voxel_pos },
        Compass::Left  => VoxelCoord { x: CHUNK_WIDTH - 1, ..voxel_pos },
    };

    neighbor_chunk[neighbor_pos]
        .is_transparent()
}

fn on_chunk_data_changed(
    event: On<ChunkDataChanged>,
    chunks: Query<&Chunk>,
    mut world: ResMut<VoxelWorld>,
    mut commands: Commands,
) {
    // May be None if the chunk was despawned before the event was processed
    let Ok(chunk) = chunks.get(event.entity) else {
        return;
    };

    let coord = chunk.coord;

    let phase = {
        let data = &mut world[&chunk];

        // Ignore events for entities that have since been replaced by a reloaded chunk
        if data.entity != Some(event.entity) {
            return;
        }

        data.generation_phase
    };

    match phase {
        ChunkGenerationPhase::HeightGeneration => {
            info!("Generating height data for chunk {}", coord);
            generate_chunk_height(&mut world[&chunk]);
            world.notify_neighbors(&mut commands, coord);

            let data = &mut world[&chunk];
            data.generation_phase = data.generation_phase.next();
        }
        // A chunk that is already done may need to be meshed again, e.g. after being reloaded
        ChunkGenerationPhase::MeshGeneration | ChunkGenerationPhase::Done => {
            if world.neighbors_ready_for_mesh(coord) {
                info!("Generating mesh for chunk {}", coord);
                commands.entity(event.entity).insert(NeedsRemesh);
                // Don't advance to the next phase yet, this will happen after the mesh is built
            }
        }
    };
}

fn remesh_chunks(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    atlas: Res<VoxelAtlas>,
    mut meshes: ResMut<Assets<Mesh>>,
    chunks: Query<(Entity, &Chunk, Option<&Mesh3d>), With<NeedsRemesh>>,
) {
    for (entity, chunk, mesh3d) in &chunks {
        let data = &world.chunks[&chunk.coord];

        assert!(
            data.generation_phase >= ChunkGenerationPhase::MeshGeneration,
            "Chunk {} was marked for remeshing before reaching mesh generation",
            chunk.coord,
        );
        assert!(
            world.neighbors_ready_for_mesh(chunk.coord),
            "Chunk {} was marked for remeshing before its neighbors were ready",
            chunk.coord,
        );

        let mesh = build_mesh(data, chunk.coord, &world, &atlas.0);

        match mesh3d {
            Some(handle) => {
                if let Err(error) = meshes.insert(&handle.0, mesh) {
                    error!("Failed to update mesh for chunk {}: {error}", chunk.coord);
                }
            }
            None => {
                commands.entity(entity).insert(Mesh3d(meshes.add(mesh)));
            }
        }

        let data = &mut world[&chunk];
        data.generation_phase = data.generation_phase.next();

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
            data.save();
        }

        commands.entity(entity).despawn();
    }
}

fn save_on_exit(
    mut exits: MessageReader<AppExit>,
    world: Res<VoxelWorld>,
    settings: Res<CameraSettings>,
    player: Single<(&Transform, &Player)>,
    camera: Single<&Transform, (With<PlayerCamera>, Without<Player>)>,
) {
    if exits.read().next().is_none() {
        return;
    }

    info!("Saving world");
    for chunk in world.chunks.values() {
        chunk.save();
    }

    settings.save();

    let (transform, player) = *player;
    WorldSave {
        player_position: transform.translation.to_array(),
        player_rotation: transform.rotation.to_array(),
        camera_rotation: camera.rotation.to_array(),
        player_velocity: player.velocity.to_array(),
    }
    .save();
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
            world.set_voxel(&mut commands, neighbor, VoxelType::Dirt);
        }
    }
}
