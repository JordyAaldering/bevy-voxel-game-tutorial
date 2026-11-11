mod chunk_neighbors;
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
    chunk_neighbors::ChunkNeighbors,
    coords::*,
    editor::{EditorPlugin, EditorState},
    meshdata::MeshData,
    physics::PhysicsPlugin,
    raycast::VoxelHit,
    screenshot::ScreenshotPlugin,
    structure::Structure,
    telemetry::TelemetryPlugin,
    voxel_atlas::{VoxelAtlas, build_voxel_atlas},
};

use std::{fs, io, ops::{Index, IndexMut}, path::{Path, PathBuf}, sync::Arc};

use better_default::Default;
use bevy::{
    prelude::*,
    app::OnAppExitSystems,
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

// Meshing a chunk needs structures generated in all 8 neighbours, which in turn need terrain in their
// neighbours, so the outermost two rings of loaded chunks are not meshed.
const CHUNK_LOAD_RADIUS: i32 = 5;

const PLAYER_EYE_HEIGHT: f32 = 1.6;
const MAX_PITCH: f32 = 1.4;

const INTERACTION_DISTANCE: f32 = 5.0;

const SAVE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/savedata/world");

/// Structures loaded from `assets/structures`.
#[derive(Resource)]
struct Structures {
    tree: Structure,
}
impl Structures {
    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/structures");

    fn load() -> Result<Self, String> {
        Ok(Self { tree: Self::load_structure("tree")? })
    }

    fn load_structure(name: &str) -> Result<Structure, String> {
        let path = Path::new(Self::DIR).join(format!("{name}.json"));
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

#[derive(Default, Resource, Reflect, InspectorOptions)]
#[derive(serde::Serialize, serde::Deserialize)]
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
    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/savedata/settings");

    fn path() -> PathBuf {
        Path::new(Self::DIR).join("camera.json")
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(wincode::SchemaWrite, wincode::SchemaRead)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoxelType {
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

    fn is_solid(self) -> bool {
        !matches!(self, VoxelType::Air)
    }
}

/// ECS component of a chunk.
#[derive(Component)]
struct Chunk {
    coord: ChunkCoord,
}

#[derive(Component)]
struct ChunkMeshes {
    voxels: Entity,
    terrain: Entity,
}

#[derive(Component)]
struct VoxelMesh;

#[derive(Component)]
struct TerrainMesh;

#[derive(Resource)]
struct ChunkMaterial(Handle<StandardMaterial>);

/// Marks a chunk for a generation check when it or its neighbors change.
#[derive(Component)]
#[component(storage = "SparseSet")]
struct NeedsGenerationCheck;

/// A chunk generation phase running asynchronously.
#[derive(Component)]
struct ChunkGenerationTask(Task<ChunkGenerationResult>);

enum ChunkGenerationResult {
    Data(ChunkGenerationPhase, Box<ChunkData>),
    Structures(Vec<WorldCoord>),
    Mesh {
        voxels: Mesh,
        terrain: Mesh,
    },
    Error(String),
}

#[derive(Component)]
#[component(storage = "SparseSet")]
struct NeedsDespawn;

/// Event triggered when the voxel data of a chunk has changed, requiring a remesh.
#[derive(EntityEvent)]
struct ChunkDataChanged {
    entity: Entity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(wincode::SchemaWrite, wincode::SchemaRead)]
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
            // Diagonal structures can also write into this chunk.
            Self::MeshGeneration => NeighborRequirement { radius: 1, diagonal: true },
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
#[derive(wincode::SchemaWrite, wincode::SchemaRead)]
pub struct ChunkData {
    coord: ChunkCoord,
    voxels: [VoxelType; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH],
    /// Density of each voxel, used for marching cubes. 0 is empty, 1 is solid.
    density: [f32; CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH],
    #[wincode(skip)]
    entity: Option<Entity>,
    generation_phase: ChunkGenerationPhase,
}

impl ChunkData {
    fn new(coord: ChunkCoord, entity: Entity) -> Self {
        Self {
            coord,
            voxels: [VoxelType::Air; _],
            density: [0.0; _],
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
    /// Chunk data is shared with generation tasks. Writes use copy-on-write.
    chunks: HashMap<ChunkCoord, Arc<ChunkData>>,
}

#[derive(Resource)]
struct SharedVoxelAtlas(Arc<VoxelAtlas>);

/// Global world state, stored in `world.bin`.
#[derive(wincode::SchemaWrite, wincode::SchemaRead)]
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
        let entity = commands
            .spawn((
                Chunk { coord },
                Name::new(format!("Chunk {coord}")),
                Transform::from_translation(coord.position()),
                InheritedVisibility::default(),
            ))
            .id();

        if let Some(chunk) = self.chunks.get_mut(&coord) {
            Arc::make_mut(chunk).set_entity(entity);
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
        if !pos.in_bounds() {
            return None;
        }

        let chunk_pos = pos.chunk_coord();
        self.get_chunk(chunk_pos)
            .filter(|chunk| chunk.generation_phase >= ChunkGenerationPhase::MeshGeneration
                && self.neighbors_ready_for(chunk_pos, ChunkGenerationPhase::MeshGeneration))
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

    /// Writes the structure into the world, without triggering remeshes.
    /// Structures cannot extend further than one chunk away from their origin.
    fn place_structure(&mut self, structure: &Structure, origin: WorldCoord) {
        for voxel in &structure.voxels {
            let coord = origin + voxel.pos;
            if !coord.in_bounds() {
                continue;
            }

            let chunk = self.chunks.get_mut(&coord.chunk_coord())
                .expect("Structure generation requires all neighboring chunk data");
            Arc::make_mut(chunk)[coord.local_coord()] = voxel.voxel;
        }
    }

    /// Notifies the chunk at `coord` that its, or its neighbors' voxel data has changed.
    /// This triggers the chunk to advance to the next generation phase, if applicable.
    fn notify_chunk(&self, commands: &mut Commands, coord: ChunkCoord) {
        if let Some(entity) = self.get_chunk(coord).and_then(|chunk| chunk.entity) {
            trace!("Notifying chunk {} of voxel data change", coord);
            commands.trigger(ChunkDataChanged { entity });
        }
    }

    fn notify_neighbors(&self, commands: &mut Commands, coord: ChunkCoord) {
        for dz in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }

                self.notify_chunk(commands, ChunkCoord::new(coord.x + dx, coord.z + dz));
            }
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
            noise::gpu::GpuPlugin::default(),
        ))
        .init_resource::<GameMode>()
        .init_resource::<VoxelWorld>()
        .insert_resource(WorldConfig::load_or_create().unwrap_or_else(|e| panic!("Failed to load world config: {e}")))
        .insert_resource(Structures::load().unwrap_or_else(|e| panic!("Failed to load structures: {e}")))
        .insert_resource(CameraSettings::load())
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
            Projection::Perspective(PerspectiveProjection {
                fov: 0.9,
                ..default()
            }),
            Msaa::Off,
            InheritedVisibility::default(),
            Transform { translation: position, rotation, ..default() },
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
    commands.insert_resource(SharedVoxelAtlas(Arc::new(atlas)));

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
    player: Single<&Transform, With<Player>>,
) {
    let center = ChunkCoord::from_position(player.translation.x, player.translation.z);

    for dz in -CHUNK_LOAD_RADIUS..=CHUNK_LOAD_RADIUS {
        for dx in -CHUNK_LOAD_RADIUS..=CHUNK_LOAD_RADIUS {
            let coord = ChunkCoord::new(center.x + dx, center.z + dz);

            if world.is_loaded(coord) {
                continue;
            }

            world.spawn_chunk(&mut commands, coord);
        }
    }
}

const SUBSURFACE_DEPTH: usize = 2;

fn generate_chunk_height(chunk: &mut ChunkData, config: &WorldConfig) {
    let columns = config.columns(chunk.coord);
    for x in 0..CHUNK_WIDTH {
        for z in 0..CHUNK_WIDTH {
            let column = &columns[x + z * CHUNK_WIDTH];
            let height = column.height;

            let pos = VoxelCoord::new(x, height, z);
            chunk[pos] = column.surface;
            chunk.density[pos.index()] = 1.0;

            for y in height.saturating_sub(SUBSURFACE_DEPTH)..height {
                let pos = VoxelCoord::new(x, y, z);
                chunk[pos] = column.subsurface;
                chunk.density[pos.index()] = 1.0;
            }

            for y in 0..height.saturating_sub(SUBSURFACE_DEPTH) {
                let pos = VoxelCoord::new(x, y, z);
                chunk[pos] = VoxelType::Stone;
                chunk.density[pos.index()] = 1.0;
            }
        }
    }
}

/// Carves caves out of the terrain, which has to be done before structures are placed on top of it.
/// Only this chunk is read and written, the noise is a pure function of the world position.
fn generate_caves(chunk: &mut ChunkData, config: &WorldConfig, noise: &[f32]) {
    let columns = config.columns(chunk.coord);
    for x in 0..CHUNK_WIDTH {
        for z in 0..CHUNK_WIDTH {
            let height = columns[x + z * CHUNK_WIDTH].height;
            // Caves may open up at the surface, only the bottom layer is kept
            for y in 1..=height {
                let pos = VoxelCoord::new(x, y, z);
                if noise[pos.index()] > config.caves.threshold {
                    chunk[pos] = VoxelType::Air;
                    chunk.density[pos.index()] = 0.0;
                }
            }
        }
    }
}

/// Replaces stone with ore. Only stone is replaced, so ores do not overwrite the surface.
fn generate_ores(chunk: &mut ChunkData, config: &WorldConfig, maps: &[Vec<f32>]) {
    for x in 0..CHUNK_WIDTH {
        for z in 0..CHUNK_WIDTH {
            for y in 0..CHUNK_HEIGHT {
                let coord = VoxelCoord::new(x, y, z);
                if chunk[coord] != VoxelType::Stone {
                    continue;
                }

                if let Some(ore) = config.ores
                    .iter()
                    .zip(maps)
                    .find_map(|(ore, map)|
                {
                    ore.contains_noise(y as i32, map[coord.index()]).then_some(ore.voxel)
                }) {
                    chunk[coord] = ore;
                    chunk.density[coord.index()] = 1.0;
                }
            }
        }
    }
}

fn generate_structures(coord: ChunkCoord, config: &WorldConfig) -> Vec<WorldCoord> {
    let columns = config.columns(coord);

    let mut origins = Vec::new();
    for z in 0..CHUNK_WIDTH {
        let world_z = coord.z * CHUNK_WIDTH as i32 + z as i32;

        for x in 0..CHUNK_WIDTH {
            let world_x = coord.x * CHUNK_WIDTH as i32 + x as i32;

            let column = &columns[x + z * CHUNK_WIDTH];
            if config.has_tree(column, world_x, world_z) {
                // Trees stand on top of the surface
                let world_y = column.height as i32 + 1;
                origins.push(WorldCoord::new(world_x, world_y, world_z));
            }
        }
    }

    origins
}

fn build_mesh(
    chunk: &ChunkData,
    chunk_pos: ChunkCoord,
    neighbors: &ChunkNeighbors,
    atlas: &VoxelAtlas,
) -> Mesh {
    let mut mesh_data = MeshData::new(atlas.layout());

    for (voxel_pos, voxel) in chunk.iter()
        .filter(|(_, v)| *v != VoxelType::Air)
    {
        for direction in Direction::iter() {
            if is_face_visible(neighbors, chunk, voxel_pos, direction) {
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
    structures: Res<Structures>,
    mut tasks: Query<(Entity, &Chunk, &mut ChunkGenerationTask, Option<&ChunkMeshes>)>,
    voxel_meshes: Query<&Mesh3d, With<VoxelMesh>>,
    terrain_meshes: Query<&Mesh3d, With<TerrainMesh>>,
    material: Res<ChunkMaterial>,
) -> Result<(), BevyError> {
    for (entity, chunk, mut task, chunk_meshes) in &mut tasks {
        let Some(result) = block_on(future::poll_once(&mut task.0)) else {
            continue;
        };

        match result {
            ChunkGenerationResult::Data(phase, generated) => {
                let data = &mut world[chunk];
                assert_eq!(data.generation_phase, phase);
                data.voxels = generated.voxels;
                data.generation_phase = data.generation_phase.next();
                world.notify_chunk(&mut commands, chunk.coord);
                world.notify_neighbors(&mut commands, chunk.coord);
            }
            ChunkGenerationResult::Structures(origins) => {
                assert_eq!(world[chunk].generation_phase, ChunkGenerationPhase::GenerateStructures);
                for origin in origins {
                    // A cave may have removed the surface below the tree.
                    let ground = origin + Direction::Bottom;
                    if world.chunks[&chunk.coord][ground.local_coord()] == VoxelType::Air {
                        continue;
                    }
                    world.place_structure(&structures.tree, origin);
                }
                let data = &mut world[chunk];
                data.generation_phase = data.generation_phase.next();
                world.notify_chunk(&mut commands, chunk.coord);
                world.notify_neighbors(&mut commands, chunk.coord);
            }
            ChunkGenerationResult::Mesh { voxels, terrain } => {
                match chunk_meshes {
                    Some(handle) => {
                        let voxel_handle = &voxel_meshes.get(handle.voxels)?.0;
                        meshes.insert(voxel_handle, voxels)?;

                        let terrain_handle = &terrain_meshes.get(handle.terrain)?.0;
                        meshes.insert(terrain_handle, terrain)?;
                    }
                    None => {
                        let voxels = commands.spawn((
                            VoxelMesh,
                            Name::new("Voxels mesh"),
                            Mesh3d(meshes.add(voxels)),
                            MeshMaterial3d(material.0.clone()),
                            ChildOf(entity),
                        )).id();
                        let terrain = commands.spawn((
                            TerrainMesh,
                            Name::new("Terrain mesh"),
                            Mesh3d(meshes.add(terrain)),
                            MeshMaterial3d(material.0.clone()),
                            ChildOf(entity),
                        )).id();
                        commands.entity(entity).insert(ChunkMeshes { voxels, terrain });
                    }
                }

                let data = &mut world[chunk];
                assert!(data.generation_phase >= ChunkGenerationPhase::MeshGeneration);
                data.generation_phase = data.generation_phase.next();
            }
            ChunkGenerationResult::Error(error) => panic!("Chunk {} generation failed: {error}", chunk.coord),
        }

        commands.entity(entity).remove::<ChunkGenerationTask>();
    }

    Ok(())
}

fn start_chunk_generation_tasks(
    mut commands: Commands,
    world: Res<VoxelWorld>,
    config: Res<WorldConfig>,
    atlas: Res<SharedVoxelAtlas>,
    mut noise_backend: ResMut<noise::gpu::GpuBackend>,
    chunks: Query<(Entity, &Chunk), (With<NeedsGenerationCheck>, Without<ChunkGenerationTask>)>,
) {
    let pool = AsyncComputeTaskPool::get();

    for (entity, chunk) in &chunks {
        let data = &world.chunks[&chunk.coord];

        let phase = data.generation_phase;

        if !world.neighbors_ready_for(chunk.coord, match phase {
            ChunkGenerationPhase::Done => ChunkGenerationPhase::MeshGeneration,
            _ => phase,
        }) {
            commands.entity(entity).remove::<NeedsGenerationCheck>();
            continue;
        }

        commands.entity(entity).remove::<NeedsGenerationCheck>();

        let task = match phase {
            ChunkGenerationPhase::HeightGeneration | ChunkGenerationPhase::OreGeneration | ChunkGenerationPhase::CaveGeneration => {
                let data = Arc::clone(data);
                let config = config.clone();
                let noise_tasks: Vec<_> = match phase {
                    ChunkGenerationPhase::OreGeneration => config.ores.iter().enumerate()
                        .map(|(index, ore)| noise_backend.area3(ore.noise_settings(), config.ore_seed(index), chunk.coord.x, chunk.coord.z))
                        .collect(),
                    ChunkGenerationPhase::CaveGeneration => vec![noise_backend.area3(config.caves.noise, config.cave_seed(), chunk.coord.x, chunk.coord.z)],
                    _ => Vec::new(),
                };
                trace!("Generating {:?} for chunk {}", phase, chunk.coord);
                pool.spawn(async move {
                    let mut generated = Box::new((*data).clone());
                    let mut maps = Vec::with_capacity(noise_tasks.len());
                    for task in noise_tasks {
                        match task.result().await {
                            Ok(values) => maps.push(values),
                            Err(error) => return ChunkGenerationResult::Error(error),
                        }
                    }
                    match phase {
                        ChunkGenerationPhase::HeightGeneration => generate_chunk_height(&mut generated, &config),
                        ChunkGenerationPhase::OreGeneration => generate_ores(&mut generated, &config, &maps),
                        ChunkGenerationPhase::CaveGeneration => generate_caves(&mut generated, &config, &maps[0]),
                        _ => unreachable!(),
                    }
                    ChunkGenerationResult::Data(phase, generated)
                })
            }
            ChunkGenerationPhase::GenerateStructures => {
                let coord = chunk.coord;
                let config = config.clone();
                trace!("Generating structures for chunk {}", coord);
                pool.spawn(async move {
                    ChunkGenerationResult::Structures(generate_structures(coord, &config))
                })
            }
            ChunkGenerationPhase::MeshGeneration | ChunkGenerationPhase::Done => {
                let data = Arc::clone(data);
                let neighbors = ChunkNeighbors::from(chunk.coord, &world.chunks);
                let atlas = Arc::clone(&atlas.0);
                let coord = chunk.coord;
                trace!("Generating mesh for chunk {}", coord);
                pool.spawn(async move {
                    ChunkGenerationResult::Mesh {
                        voxels: build_mesh(&data, coord, &neighbors, &atlas),
                        terrain: Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default())
                            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<Vec3>::new())
                            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<Vec3>::new())
                            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<Vec2>::new())
                            .with_inserted_indices(bevy::mesh::Indices::U32(vec![])),
                    }
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
            if world.get_voxel(neighbor).is_some() {
                world.set_voxel(&mut commands, neighbor, VoxelType::Dirt);
            }
        }
    }
}
