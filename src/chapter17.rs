mod aabb;
mod biome;
mod coords;
mod editor;
mod meshdata;
mod noise;
mod ore;
mod physics;
mod raycast;
mod screenshot;
mod structure;
mod telemetry;
mod voxel_atlas;

use crate::{
    biome::WorldConfig,
    coords::*,
    editor::{EditorPlugin, EditorState},
    meshdata::MeshData,
    physics::PhysicsPlugin,
    raycast::VoxelHit,
    screenshot::ScreenshotPlugin,
    structure::Structure,
    telemetry::{Telemetry, TelemetryPublisher},
    voxel_atlas::{VoxelAtlas, build_voxel_atlas},
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

// Meshing a chunk needs structures generated in all 8 neighbours, which in turn need terrain in their
// neighbours, so the outermost two rings of loaded chunks are not meshed.
const CHUNK_LOAD_RADIUS: i32 = 5;

const STRUCTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/structures");

const CHUNK_NEIGHBORS: [(i32, i32); 8] = [
    (-1, -1), (0, -1), (1, -1),
    (-1,  0),          (1,  0),
    (-1,  1), (0,  1), (1,  1),
];

const PLAYER_EYE_HEIGHT: f32 = 1.6;
const MAX_PITCH: f32 = 1.4;

const INTERACTION_DISTANCE: f32 = 5.0;

const SAVE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/savedata/world");
const SETTINGS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/savedata/settings");

/// Structures loaded from `assets/structures`.
#[derive(Resource)]
struct Structures {
    tree: Structure,
}

impl Structures {
    fn load() -> Result<Self, String> {
        Ok(Self { tree: Self::load_structure("tree")? })
    }

    fn load_structure(name: &str) -> Result<Structure, String> {
        let path = Path::new(STRUCTURES_DIR).join(format!("{name}.json"));
        let json = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&json).map_err(|e| format!("{}: {e}", path.display()))
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

#[derive(Default, Resource, Reflect, InspectorOptions, Serialize, Deserialize)]
#[reflect(Resource, InspectorOptions)]
#[serde(default)]
struct CameraSettings {
    #[default(10.0)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SchemaWrite, SchemaRead)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum VoxelType {
    Air,
    Grass,
    Dirt,
    Stone,
    StoneCoal,
    StoneIron,
    StoneGold,
    StoneDiamond,
    Log,
    Leaves,
    BrickGrey,
    BrickRed,
    Cactus,
    CottonBlue,
    CottonGreen,
    CottonRed,
    CottonWhite,
    Sand,
    Snow,
    Glass,
    Ice,
}

impl VoxelType {
    const ALL: [Self; 21] = [
        Self::Air,
        Self::Grass,
        Self::Dirt,
        Self::Stone,
        Self::StoneCoal,
        Self::StoneIron,
        Self::StoneGold,
        Self::StoneDiamond,
        Self::Log,
        Self::Leaves,
        Self::BrickGrey,
        Self::BrickRed,
        Self::Cactus,
        Self::CottonBlue,
        Self::CottonGreen,
        Self::CottonRed,
        Self::CottonWhite,
        Self::Sand,
        Self::Snow,
        Self::Glass,
        Self::Ice,
    ];

    fn is_transparent(self) -> bool {
        matches!(self, VoxelType::Air)
    }

    fn is_solid(&self) -> bool {
        !matches!(self, VoxelType::Air)
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
    OreGeneration,
    CaveGeneration,
    GenerateStructures,
    MeshGeneration,
    Done,
}

impl ChunkGenerationPhase {
    fn next(self) -> Self {
        match self {
            Self::HeightGeneration => Self::OreGeneration,
            Self::OreGeneration => Self::CaveGeneration,
            Self::CaveGeneration => Self::GenerateStructures,
            Self::GenerateStructures => Self::MeshGeneration,
            Self::MeshGeneration  => Self::Done,
            Self::Done => Self::Done,
        }
    }

    /// Which neighbors must have completed the previous phase before this phase can run.
    fn neighbor_requirement(self) -> NeighborRequirement {
        match self {
            Self::HeightGeneration => NeighborRequirement::NONE,
            Self::OreGeneration => NeighborRequirement::NONE,
            Self::CaveGeneration => NeighborRequirement::NONE,
            // Structures write into neighbors, including diagonal ones
            Self::GenerateStructures => NeighborRequirement { radius: 1, diagonal: true },
            // Only the chunks touching this one are needed to cull faces
            Self::MeshGeneration => NeighborRequirement { radius: 1, diagonal: false },
            Self::Done => NeighborRequirement { radius: 1, diagonal: false },
        }
    }
}

/// The set of neighboring chunks that must be ready before a generation phase can run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

    /// Whether all neighbors required by `phase` have completed the phase before it.
    fn neighbors_ready_for(&self, coord: ChunkCoord, phase: ChunkGenerationPhase) -> bool {
        phase.neighbor_requirement()
            .offsets()
            .all(|(dx, dz)| {
                self.get_chunk_neighbor(coord, dx, dz)
                    .is_some_and(|neighbor| neighbor.generation_phase >= phase)
            })
    }

    /// Writes the structure into the world, without triggering remeshes.
    /// Structures cannot extend further than one chunk away from their origin.
    fn place_structure(&mut self, structure: &Structure, origin: WorldCoord) {
        for voxel in &structure.voxels {
            let coord = origin + voxel.pos;
            if coord.y < 0 || coord.y >= CHUNK_HEIGHT as i32 {
                continue;
            }

            let chunk = self.chunks.get_mut(&coord.chunk_coord())
                .expect("Structure generation requires all neighboring chunk data");
            chunk[coord.local_coord()] = voxel.voxel;
        }
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
        for (dx, dz) in CHUNK_NEIGHBORS {
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
            ScreenshotPlugin::default(),
            PhysicsPlugin,
            EditorPlugin,
        ))
        .init_resource::<GameMode>()
        .init_resource::<VoxelWorld>()
        .insert_resource(WorldConfig::load_or_create().unwrap_or_else(|e| panic!("Failed to load world config: {e}")))
        .insert_resource(Structures::load().unwrap_or_else(|e| panic!("Failed to load structures: {e}")))
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
    config: Res<WorldConfig>,
) {
    let save = WorldSave::load();
    let position = save
        .as_ref()
        .map(|save| Vec3::from(save.player_position))
        .unwrap_or_else(|| {
            let world_height = config.column(0, 0).height;
            Vec3::new(0.5, world_height as f32 + 1.0, 0.5)
        });
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
                Projection::Perspective(PerspectiveProjection {
                    fov: 0.9,
                    ..default()
                }),
                Msaa::Off,
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
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (atlas, image) = build_voxel_atlas(&mut images)
        .unwrap_or_else(|e| panic!("Failed to build voxel atlas: {e}"));
    let texture = images.add(image);
    commands.insert_resource(atlas);

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

const SUBSURFACE_DEPTH: usize = 2;

fn generate_chunk_height(chunk: &mut ChunkData, config: &WorldConfig) {
    for x in 0..CHUNK_WIDTH {
        let world_x = chunk.coord.x * CHUNK_WIDTH as i32 + x as i32;

        for z in 0..CHUNK_WIDTH {
            let world_z = chunk.coord.z * CHUNK_WIDTH as i32 + z as i32;

            let column = config.column(world_x, world_z);
            let height = column.height;

            chunk[VoxelCoord::new(x, height, z)] = column.surface;
            for y in height.saturating_sub(SUBSURFACE_DEPTH)..height {
                chunk[VoxelCoord::new(x, y, z)] = column.subsurface;
            }
            // Below that is stone
            for y in 0..height.saturating_sub(SUBSURFACE_DEPTH) {
                chunk[VoxelCoord::new(x, y, z)] = VoxelType::Stone;
            }
        }
    }
}

/// Carves caves out of the terrain, which has to be done before structures are placed on top of it.
/// Only this chunk is read and written, the noise is a pure function of the world position.
fn generate_caves(chunk: &mut ChunkData, config: &WorldConfig) {
    for x in 0..CHUNK_WIDTH {
        let world_x = chunk.coord.x * CHUNK_WIDTH as i32 + x as i32;

        for z in 0..CHUNK_WIDTH {
            let world_z = chunk.coord.z * CHUNK_WIDTH as i32 + z as i32;

            let height = config.column(world_x, world_z).height;
            // Caves may open up at the surface, only the bottom layer is kept
            for y in 1..=height {
                if config.is_cave(world_x, y as i32, world_z) {
                    chunk[VoxelCoord::new(x, y, z)] = VoxelType::Air;
                }
            }
        }
    }
}

/// Replaces stone with ore. Only stone is replaced, so ores do not overwrite the surface.
fn generate_ores(chunk: &mut ChunkData, config: &WorldConfig) {
    for x in 0..CHUNK_WIDTH {
        let world_x = chunk.coord.x * CHUNK_WIDTH as i32 + x as i32;

        for z in 0..CHUNK_WIDTH {
            let world_z = chunk.coord.z * CHUNK_WIDTH as i32 + z as i32;

            for y in 0..CHUNK_HEIGHT {
                let coord = VoxelCoord::new(x, y, z);
                if chunk[coord] != VoxelType::Stone {
                    continue;
                }
                if let Some(ore) = config.ore_at(world_x, y as i32, world_z) {
                    chunk[coord] = ore;
                }
            }
        }
    }
}

fn generate_structures(world: &mut VoxelWorld, coord: ChunkCoord, config: &WorldConfig, structures: &Structures) {
    for z in 0..CHUNK_WIDTH {
        let world_z = coord.z * CHUNK_WIDTH as i32 + z as i32;

        for x in 0..CHUNK_WIDTH {
            let world_x = coord.x * CHUNK_WIDTH as i32 + x as i32;

            let column = config.column(world_x, world_z);
            if config.has_tree(&column, world_x, world_z) {
                // A cave may have been carved out of the surface, in which case the tree would float
                if world.chunks[&coord][VoxelCoord::new(x, column.height, z)] == VoxelType::Air {
                    continue;
                }

                // Trees stand on top of the surface
                let world_y = column.height as i32 + 1;
                world.place_structure(&structures.tree, WorldCoord::new(world_x, world_y, world_z));
            }
        }
    }
}

fn build_mesh(chunk: &ChunkData, chunk_pos: ChunkCoord, world: &VoxelWorld, atlas: &VoxelAtlas) -> Mesh {
    let mut mesh_data = MeshData::new(atlas.layout());

    for (voxel_pos, voxel) in chunk.iter()
        .filter(|(_, v)| *v != VoxelType::Air)
    {
        for direction in Direction::iter() {
            if is_face_visible(world, chunk, chunk_pos, voxel_pos, direction) {
                let origin = voxel_pos.local_position();
                let texture = atlas.texture(voxel, direction, variant_seed(chunk_pos, voxel_pos));
                mesh_data.add_quad(direction, origin, texture);
            }
        }
    }

    mesh_data.build()
}

/// Deterministic per-position hash, so a voxel keeps its variant when remeshed or reloaded.
fn variant_seed(chunk_pos: ChunkCoord, voxel_pos: VoxelCoord) -> u32 {
    let x = chunk_pos.x.wrapping_mul(CHUNK_WIDTH as i32).wrapping_add(voxel_pos.x as i32);
    let z = chunk_pos.z.wrapping_mul(CHUNK_WIDTH as i32).wrapping_add(voxel_pos.z as i32);
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1)
        ^ (voxel_pos.y as u32).wrapping_mul(0x85EB_CA77)
        ^ (z as u32).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 13)
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
    config: Res<WorldConfig>,
    structures: Res<Structures>,
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

    let advance = match phase {
        ChunkGenerationPhase::HeightGeneration => {
            info!("Generating height data for chunk {}", coord);
            generate_chunk_height(&mut world[&chunk], &config);
            true
        }
        ChunkGenerationPhase::CaveGeneration => {
            if world.neighbors_ready_for(coord, phase) {
                info!("Carving caves for chunk {}", coord);
                generate_caves(&mut world[&chunk], &config);
                true
            } else {
                false
            }
        }
        ChunkGenerationPhase::OreGeneration => {
            if world.neighbors_ready_for(coord, phase) {
                info!("Placing ores for chunk {}", coord);
                generate_ores(&mut world[&chunk], &config);
                true
            } else {
                false
            }
        }
        ChunkGenerationPhase::GenerateStructures => {
            if world.neighbors_ready_for(coord, phase) {
                info!("Generating structures for chunk {}", coord);
                generate_structures(&mut world, coord, &config, &structures);
                true
            } else {
                false
            }
        }
        // A chunk that is already done may need to be meshed again, e.g. after being reloaded
        ChunkGenerationPhase::MeshGeneration | ChunkGenerationPhase::Done => {
            if world.neighbors_ready_for(coord, ChunkGenerationPhase::MeshGeneration) {
                info!("Generating mesh for chunk {}", coord);
                commands.entity(event.entity).insert(NeedsRemesh);
                // Don't advance to the next phase yet, this will happen after the mesh is built
            }
            false
        }
    };

    if advance {
        let data = &mut world[&chunk];
        data.generation_phase = data.generation_phase.next();

        // Neighbors may have been waiting on this phase, and this chunk may be ready for the next one
        world.notify_neighbors(&mut commands, coord);
        commands.trigger(ChunkDataChanged { entity: event.entity });
    }
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
            world.neighbors_ready_for(chunk.coord, ChunkGenerationPhase::MeshGeneration),
            "Chunk {} was marked for remeshing before its neighbors were ready",
            chunk.coord,
        );

        let mesh = build_mesh(data, chunk.coord, &world, &atlas);

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
