use crate::*;

#[allow(unused_imports)]
use std::{ops::{Index, IndexMut}, sync::Arc};

use better_default::Default;
#[allow(unused_imports)]
use bevy::{
    prelude::*,
    math::CompassQuadrant,
    platform::collections::HashMap,
    tasks::{AsyncComputeTaskPool, futures_lite::future, Task, block_on},
};
#[allow(unused_imports)]
use bevy_inspector_egui::{egui, bevy_egui::{EguiContexts, EguiPrimaryContextPass}};

#[cfg(all(since07, until14))]
pub const ATLAS_COLUMNS: u32 = 9;
#[cfg(all(since07, until14))]
pub const ATLAS_ROWS: u32 = 10;

#[cfg(since10)]
const CHUNK_LOAD_RADIUS: i32 = 4;

#[derive(Default)]
pub struct VoxelWorldPlugin;

#[cfg(until07)]
type TChunkData = ChunkData;
/// Chunk data is shared with meshing tasks. Writes go through [`Arc::make_mut`],
/// which only copies a chunk if a task is still reading it.
#[cfg(since08)]
type TChunkData = Arc<ChunkData>;

#[derive(Default, Resource)]
pub struct VoxelWorld {
    chunks: HashMap<ChunkCoord, TChunkData>,
}

/// Marker component for a chunk that is currently being meshed on another thread.
#[cfg(since08)]
#[derive(Component)]
pub struct MeshTask(pub Task<Mesh>);

#[cfg(all(since07, until14))]
#[derive(Resource)]
pub struct VoxelAtlas(pub TextureAtlasLayout);

#[derive(Resource)]
pub struct ChunkMaterial(pub Handle<StandardMaterial>);

impl Plugin for VoxelWorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoxelWorld>();
        app.add_systems(PreStartup, setup_chunk_material);

        #[cfg(only05)]
        app.add_systems(EguiPrimaryContextPass, chunk_debug_ui);

        #[cfg(since08)]
        app.add_systems(PostUpdate, (
                poll_mesh_tasks,
                remesh_chunks,
            ).chain()
        );

        #[cfg(all(since10, until12))]
        app.add_systems(Update,
            (
                    spawn_chunks_around_player,
                    retire_distant_chunks,
                    despawn_retired_chunks,
                    poll_mesh_tasks,
                    remesh_chunks,
                ).chain()
            );

        #[cfg(since13)]
        app.add_systems(Update,
            (
                    spawn_chunks_around_player,
                    retire_distant_chunks,
                    despawn_retired_chunks,
                    poll_chunk_generation_tasks,
                    start_chunk_generation_tasks,
                ).chain()
            )
            .add_observer(on_chunk_data_changed);
    }
}

impl VoxelWorld {
    pub fn add_chunk(&mut self, pos: ChunkCoord, chunk: TChunkData) {
        debug_assert!(!self.chunks.contains_key(&pos));
        self.chunks.insert(pos, chunk);
    }

    pub fn get_chunk(&self, pos: ChunkCoord) -> Option<&TChunkData> {
        self.chunks.get(&pos)
    }

    #[cfg(until07)]
    pub fn iter(&self) -> impl Iterator<Item = (&ChunkCoord, &TChunkData)> {
        self.chunks.iter()
    }
}

#[cfg(since07)]
impl VoxelWorld {
    #[cfg(until12)]
    pub fn get_voxel(&self, pos: WorldCoord) -> Option<VoxelType> {
        if !pos.in_bounds() {
            return None;
        }

        let chunk_pos = pos.chunk_coord();
        self.get_chunk(chunk_pos).map(|chunk| {
            let voxel_pos = pos.local_coord();
            chunk[voxel_pos]
        })
    }

    #[cfg(since13)]
    pub fn get_voxel(&self, pos: WorldCoord) -> Option<VoxelType> {
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

    pub fn is_solid(&self, pos: WorldCoord) -> bool {
        self.get_voxel(pos)
            .is_some_and(VoxelType::is_solid)
    }
}

#[cfg(since08)]
impl VoxelWorld {
    #[cfg(until12)]
    pub fn get_chunk_mut(&mut self, pos: ChunkCoord) -> Option<&mut TChunkData> {
        self.chunks.get_mut(&pos)
    }


    /// Spawns an entity for the chunk at `coord`. The voxel data is generated only
    /// if it does not exist yet, so earlier modifications survive unloading.
    #[cfg(until12)]
    pub fn spawn_chunk(
        &mut self,
        commands: &mut Commands,
        coord: ChunkCoord,
        generate_chunk: impl FnOnce(&mut ChunkData),
    ) -> Entity {
        let entity = commands.spawn((
            Name::new(format!("Chunk {coord}")),
            Transform::from_translation(coord.position()),
            Chunk::new(coord),
            NeedsRemesh,
        )).id();

        if let Some(chunk) = self.get_chunk_mut(coord) {
            Arc::make_mut(chunk).set_entity(entity);
        } else {
            let mut chunk = ChunkData::new(coord, entity);
            generate_chunk(&mut chunk);
            self.add_chunk(coord, Arc::new(chunk));
        }

        // Existing neighbors previously treated this side as open, so their border faces change
        for compass in [
            CompassQuadrant::North,
            CompassQuadrant::East,
            CompassQuadrant::South,
            CompassQuadrant::West,
        ] {
            if let Some(neighbor) = self.get_chunk(coord + compass)
                && let Some(neighbor_entity) = neighbor.entity()
            {
                commands.entity(neighbor_entity).insert(NeedsRemesh);
            }
        }

        entity
    }

    #[cfg(until12)]
    pub fn set_voxel(&mut self, commands: &mut Commands, pos: WorldCoord, voxel: VoxelType) {
        let voxel_pos = pos.local_coord();
        let chunk_pos = pos.chunk_coord();
        let chunk = self.get_chunk_mut(chunk_pos).unwrap();
        Arc::make_mut(chunk)[voxel_pos] = voxel;

        if let Some(entity) = chunk.entity() {
            commands.entity(entity).insert(NeedsRemesh);
        }

        // Voxels on a chunk border influence the faces of the adjacent chunk
        if voxel_pos.x == 0 {
            if let Some(neighbor) = self.get_chunk(chunk_pos + CompassQuadrant::West)
                && let Some(entity) = neighbor.entity()
            {
                commands.entity(entity).insert(NeedsRemesh);
            }
        } else if voxel_pos.x == CHUNK_WIDTH - 1 {
            if let Some(neighbor) = self.get_chunk(chunk_pos + CompassQuadrant::East)
                && let Some(entity) = neighbor.entity()
            {
                commands.entity(entity).insert(NeedsRemesh);
            }
        }
        if voxel_pos.z == 0 {
            if let Some(neighbor) = self.get_chunk(chunk_pos + CompassQuadrant::South)
                && let Some(entity) = neighbor.entity()
            {
                commands.entity(entity).insert(NeedsRemesh);
            }
        } else if voxel_pos.z == CHUNK_WIDTH - 1 {
            if let Some(neighbor) = self.get_chunk(chunk_pos + CompassQuadrant::North)
                && let Some(entity) = neighbor.entity()
            {
                commands.entity(entity).insert(NeedsRemesh);
            }
        }
    }

    #[cfg(since13)]
    pub fn set_voxel(&mut self, commands: &mut Commands, pos: WorldCoord, voxel: VoxelType) {
        let voxel_pos = pos.local_coord();
        let chunk_pos = pos.chunk_coord();
        let chunk = self.chunks.get_mut(&chunk_pos).unwrap();
        Arc::make_mut(chunk)[voxel_pos] = voxel;

        self.notify_chunk(commands, chunk_pos);

        // Voxels on a chunk border influence the faces of the adjacent chunk
        if voxel_pos.x == 0 {
            self.notify_chunk(commands, chunk_pos + CompassQuadrant::West);
        } else if voxel_pos.x == CHUNK_WIDTH - 1 {
            self.notify_chunk(commands, chunk_pos + CompassQuadrant::East);
        }
        if voxel_pos.z == 0 {
            self.notify_chunk(commands, chunk_pos + CompassQuadrant::South);
        } else if voxel_pos.z == CHUNK_WIDTH - 1 {
            self.notify_chunk(commands, chunk_pos + CompassQuadrant::West);
        }
    }
}

impl Index<&Chunk> for VoxelWorld {
    type Output = ChunkData;

    fn index(&self, chunk: &Chunk) -> &Self::Output {
        self.chunks.get(&chunk.coord())
            .expect("A chunk entity cannot exist without its data")
    }
}

impl IndexMut<&Chunk> for VoxelWorld {
    fn index_mut(&mut self, chunk: &Chunk) -> &mut Self::Output {
        Arc::make_mut(
            self.chunks.get_mut(&chunk.coord())
                .expect("A chunk entity cannot exist without its data"),
        )
    }
}

#[cfg(since13)]
impl VoxelWorld {
    pub fn spawn_chunk(&mut self, commands: &mut Commands, coord: ChunkCoord) -> Entity {
        let entity = commands.spawn((
            Name::new(format!("Chunk {coord}")),
            Transform::from_translation(coord.position()),
            Chunk::new(coord),
        )).id();

        if let Some(chunk) = self.chunks.get_mut(&coord) {
            Arc::make_mut(chunk).set_entity(entity);
        } else {
            #[cfg(until13)]
            let chunk = ChunkData::new(coord, entity);

            #[cfg(since14)]
            let chunk = ChunkData::load(coord, entity)
                .unwrap_or_else(|| ChunkData::new(coord, entity));

            self.add_chunk(coord, Arc::new(chunk));
        }

        commands.trigger(ChunkDataChanged { entity });

        entity
    }

    pub fn get_chunk_neighbor(&self, pos: ChunkCoord, dx: i32, dz: i32) -> Option<&ChunkData> {
        let neighbor_pos = ChunkCoord::new(pos.x + dx, pos.z + dz);
        self.chunks.get(&neighbor_pos).map(Arc::as_ref)
    }

    /// Whether all neighbors required by `phase` have completed the phase before it.
    pub fn neighbors_ready_for(&self, coord: ChunkCoord, phase: ChunkGenerationPhase) -> bool {
        phase.neighbor_requirement()
            .offsets()
            .all(|(dx, dz)| {
                self.get_chunk_neighbor(coord, dx, dz)
                    .is_some_and(|neighbor| neighbor.generation_phase >= phase)
            })
    }

    /// Notifies the chunk at `coord` that its, or its neighbors' voxel data has changed.
    /// This triggers the chunk to advance to the next generation phase, if applicable.
    pub fn notify_chunk(&self, commands: &mut Commands, coord: ChunkCoord) {
        if let Some(entity) = self.get_chunk(coord).and_then(|chunk| chunk.entity()) {
            info!("Notifying chunk {} of voxel data change", coord);
            commands.trigger(ChunkDataChanged { entity });
        }
    }

    pub fn notify_neighbors(&self, commands: &mut Commands, coord: ChunkCoord) {
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            self.notify_chunk(commands, ChunkCoord::new(coord.x + dx, coord.z + dz));
        }
    }
}

#[cfg(since10)]
impl VoxelWorld {
    fn is_loaded(&self, coord: ChunkCoord) -> bool {
        self.chunks.get(&coord)
            .is_some_and(|chunk| chunk.entity().is_some())
    }
}

#[cfg(only05)]
pub fn chunk_debug_ui(mut contexts: EguiContexts, world: Res<VoxelWorld>) -> Result {
    egui::Window::new("Chunks")
        .show(contexts.ctx_mut()?, |ui| {
            ui.label(format!("Loaded chunks: {}", world.chunks.len()));

            ui.separator();

            for (coord, chunk) in &world.chunks {
                let solid_voxels = chunk.iter()
                    .filter(|(_, v)| !matches!(v, crate::VoxelType::Air))
                    .count();

                ui.collapsing(
                    format!("Chunk {coord}"),
                |ui| ui.label(format!("Solid voxels: {solid_voxels}")),
                );
            }
        });

    Ok(())
}

#[cfg(until05)]
fn setup_chunk_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let material = materials.add(
        Color::linear_rgb(0.2, 0.7, 0.3)
    );

    commands.insert_resource(ChunkMaterial(material));
}

#[cfg(all(since06, until14))]
fn setup_chunk_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let texture = asset_server.load("spritesheet.png");

    let material = materials.add(StandardMaterial {
        base_color_texture: Some(texture),
        perceptual_roughness: 0.9,
        reflectance: 0.1,
        ..default()
    });

    commands.insert_resource(ChunkMaterial(material));

    #[cfg(since07)]
    commands.insert_resource(VoxelAtlas(TextureAtlasLayout::from_grid(
        UVec2::splat(ATLAS_COLUMNS * ATLAS_ROWS),
        ATLAS_COLUMNS,
        ATLAS_ROWS,
        None,
        None,
    )));
}

#[cfg(since15)]
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

/// Starts building the mesh of every chunk marked with [`NeedsRemesh`] on a background thread.
/// A chunk that is still being meshed stays marked, and is meshed again once that task is done.
#[cfg(since08)]
fn remesh_chunks(
    mut commands: Commands,
    world: Res<VoxelWorld>,
    #[cfg(until14)] atlas: Res<VoxelAtlas>,
    #[cfg(since15)] atlas: Res<SharedVoxelAtlas>,
    chunks: Query<(Entity, &Chunk), (With<NeedsRemesh>, Without<MeshTask>)>,
) {
    let pool = AsyncComputeTaskPool::get();

    for (entity, chunk) in &chunks {
        let Some(data) = world.get_chunk(chunk.coord()) else {
            continue;
        };

        // The task cannot borrow the world, so it gets shared handles to the data it needs
        let data = Arc::clone(data);
        let coord = chunk.coord();
        let neighbors = ChunkNeighbors::from(coord, &world.chunks);
        #[cfg(until14)]
        let atlas = atlas.0.clone();
        #[cfg(since15)]
        let atlas = Arc::clone(&atlas.0);
        let task = pool.spawn(async move {
            #[cfg(until14)]
            let mesh = data.build_mesh(&neighbors, &atlas);
            #[cfg(since15)]
            let mesh = data.build_mesh(&neighbors, coord, &atlas);
            mesh
        });

        commands.entity(entity)
            .remove::<NeedsRemesh>()
            .insert(MeshTask(task));
    }
}

/// Applies the meshes of finished [`MeshTask`]s.
#[cfg(since08)]
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
                // The chunk already has a mesh, so replace it
                meshes.insert(&handle.0, mesh)?;
            }
            None => {
                // The chunk does not have a mesh yet, so spawn one
                commands.entity(entity).insert(Mesh3d(meshes.add(mesh)));
            }

        }

        commands.entity(entity).remove::<MeshTask>();
    }

    Ok(())
}

#[cfg(since10)]
fn spawn_chunks_around_player(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    material: Res<ChunkMaterial>,
    player: Single<&Transform, With<Player>>,
    #[cfg(until12)] config: Res<crate::TerrainConfig>,
) {
    let center = ChunkCoord::from_position(player.translation.x, player.translation.z);

    for dz in -CHUNK_LOAD_RADIUS..=CHUNK_LOAD_RADIUS {
        for dx in -CHUNK_LOAD_RADIUS..=CHUNK_LOAD_RADIUS {
            let coord = ChunkCoord::new(center.x + dx, center.z + dz);

            if world.is_loaded(coord) {
                continue;
            }

            #[cfg(until12)]
            let entity = world.spawn_chunk(&mut commands, coord,
                |chunk| crate::generate_chunk(chunk, &config));
            #[cfg(since13)]
            let entity = world.spawn_chunk(&mut commands, coord);
            commands.entity(entity).insert(MeshMaterial3d(material.0.clone()));
        }
    }
}

#[cfg(since10)]
fn retire_distant_chunks(
    mut commands: Commands,
    player: Single<&Transform, With<Player>>,
    chunks: Query<(Entity, &Chunk, Has<NeedsDespawn>)>,
) {
    let center = ChunkCoord::from_position(player.translation.x, player.translation.z);

    for (entity, chunk, retired) in &chunks {
        let in_range = (chunk.coord().x - center.x).abs() <= CHUNK_LOAD_RADIUS
            && (chunk.coord().z - center.z).abs() <= CHUNK_LOAD_RADIUS;

        if !in_range && !retired {
            commands.entity(entity).insert(NeedsDespawn);
        } else if in_range && retired {
            commands.entity(entity).remove::<NeedsDespawn>();
        }
    }
}

#[cfg(since10)]
fn despawn_retired_chunks(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    retired_chunks: Query<(Entity, &Chunk), With<NeedsDespawn>>,
) {
    for (entity, chunk) in &retired_chunks {
        trace!("Despawning chunk {}", chunk.coord());
        // Keep the voxel data; only the entity goes away
        if let Some(data) = world.chunks.get_mut(&chunk.coord()) {
            Arc::make_mut(data).clear_entity();

            #[cfg(since14)]
            data.save();
        }

        commands.entity(entity).despawn();
    }
}

#[cfg(since13)]
fn generate_chunk_height(chunk: &mut ChunkData, config: &TerrainConfig) {
    let heights = config.height.area2(config.seed, chunk.coord().x, chunk.coord().z);
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

#[cfg(since13)]
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

    let chunk = world.get_chunk(chunk.coord()).unwrap();

    // Ignore events for entities that have since been replaced by a reloaded chunk.
    if chunk.entity() == Some(event.entity) {
        commands.entity(event.entity).insert(NeedsGenerationCheck);
    }
}

#[cfg(since13)]
fn poll_chunk_generation_tasks(
    mut commands: Commands,
    mut world: ResMut<VoxelWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    #[cfg(since16)] structures: Res<Structures>,
    mut tasks: Query<(Entity, &Chunk, &mut ChunkGenerationTask, Option<&Mesh3d>)>,
) -> Result<(), BevyError> {
    for (entity, chunk, mut task, mesh3d) in &mut tasks {
        let Some(result) = block_on(future::poll_once(&mut task.0)) else {
            continue;
        };

        match result {
            #[cfg(until15)]
            ChunkGenerationResult::Data(generated) => {
                let data = &mut world[&chunk];

                assert_eq!(data.generation_phase, ChunkGenerationPhase::HeightGeneration);

                data.set_voxels(*generated);
                data.generation_phase = data.generation_phase.next();
                world.notify_chunk(&mut commands, chunk.coord());
                world.notify_neighbors(&mut commands, chunk.coord());
            }
            #[cfg(since16)]
            ChunkGenerationResult::Data(phase, generated) => {
                let data = &mut world[chunk];

                assert_eq!(data.generation_phase, phase);

                data.set_voxels(*generated);
                data.generation_phase = data.generation_phase.next();
                world.notify_chunk(&mut commands, chunk.coord());
                world.notify_neighbors(&mut commands, chunk.coord());
            }
            #[cfg(since16)]
            ChunkGenerationResult::Structures(origins) => {
                assert_eq!(world[chunk].generation_phase, ChunkGenerationPhase::GenerateStructures);

                for origin in origins {
                    world.place_structure(&structures.tree, origin);
                }

                let data = &mut world[chunk];
                data.generation_phase = data.generation_phase.next();
                world.notify_chunk(&mut commands, chunk.coord());
                world.notify_neighbors(&mut commands, chunk.coord());
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

                let data = &mut world[&chunk];
                assert!(data.generation_phase >= ChunkGenerationPhase::MeshGeneration);
                data.generation_phase = data.generation_phase.next();
            }
        }

        commands.entity(entity).remove::<ChunkGenerationTask>();
    }

    Ok(())
}

#[cfg(since13)]
fn start_chunk_generation_tasks(
    mut commands: Commands,
    world: Res<VoxelWorld>,
    config: Res<TerrainConfig>,
    #[cfg(until14)] atlas: Res<VoxelAtlas>,
    #[cfg(since15)] atlas: Res<SharedVoxelAtlas>,
    chunks: Query<(Entity, &Chunk), (With<NeedsGenerationCheck>, Without<ChunkGenerationTask>)>,
) {
    let pool = AsyncComputeTaskPool::get();

    for (entity, chunk) in &chunks {
        let data = &world.get_chunk(chunk.coord()).unwrap();

        let phase = data.generation_phase;
        commands.entity(entity).remove::<NeedsGenerationCheck>();

        if !world.neighbors_ready_for(chunk.coord(), match phase {
            ChunkGenerationPhase::Done => ChunkGenerationPhase::MeshGeneration,
            _ => phase,
        }) {
            continue;
        }

        let task = match phase {
            #[cfg(until15)]
            ChunkGenerationPhase::HeightGeneration => {
                let data = Arc::clone(data);
                let config = config.clone();
                info!("Generating height data for chunk {}", chunk.coord());
                pool.spawn(async move {
                    let mut generated = Box::new((*data).clone());
                    generate_chunk_height(&mut generated, &config);
                    ChunkGenerationResult::Data(generated)
                })
            }
            #[cfg(since16)]
            ChunkGenerationPhase::HeightGeneration => {
                let data = Arc::clone(data);
                let config = config.clone();
                trace!("Generating {:?} for chunk {}", phase, chunk.coord());
                pool.spawn(async move {
                    let mut generated = Box::new((*data).clone());
                    match phase {
                        ChunkGenerationPhase::HeightGeneration => generate_chunk_height(&mut generated, &config),
                        _ => unreachable!(),
                    }
                    ChunkGenerationResult::Data(phase, generated)
                })
            }
            #[cfg(since16)]
            ChunkGenerationPhase::GenerateStructures => {
                let coord = chunk.coord();
                let config = config.clone();
                trace!("Generating structures for chunk {}", coord);
                pool.spawn(async move {
                    ChunkGenerationResult::Structures(generate_structures(coord, &config))
                })
            }
            ChunkGenerationPhase::MeshGeneration | ChunkGenerationPhase::Done => {
                if !world.neighbors_ready_for(chunk.coord(), ChunkGenerationPhase::MeshGeneration) {
                    continue;
                }

                let data = Arc::clone(data);
                let coord = chunk.coord();
                let neighbors = ChunkNeighbors::from(coord, &world.chunks);
                #[cfg(until14)]
                let atlas = atlas.0.clone();
                #[cfg(since15)]
                let atlas = Arc::clone(&atlas.0);
                trace!("Generating mesh for chunk {}", coord);
                pool.spawn(async move {
                    #[cfg(until14)]
                    let mesh = ChunkGenerationResult::Mesh(data.build_mesh(&neighbors, &atlas));
                    #[cfg(since15)]
                    let mesh = ChunkGenerationResult::Mesh(data.build_mesh(&neighbors, coord, &atlas));
                    mesh
                })
            }
        };

        commands.entity(entity).insert(ChunkGenerationTask(task));
    }
}

#[cfg(since14)]
impl VoxelWorld {
    pub fn save(&self) {
        for chunk in self.chunks.values() {
            chunk.save();
        }
    }
}

#[cfg(since16)]
impl VoxelWorld {
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
}

#[cfg(since16)]
fn generate_structures(coord: ChunkCoord, config: &TerrainConfig) -> Vec<WorldCoord> {
    /// Local (x, z) columns of each chunk that get a tree: one in the middle, and one in the corner
    /// whose leaves overflow into three neighbouring chunks.
    const TREE_COLUMNS: [(usize, usize); 2] = [
        (CHUNK_WIDTH / 2, CHUNK_WIDTH / 2),
        (CHUNK_WIDTH - 1, CHUNK_WIDTH - 1),
    ];

    let heights = config.height.area2(config.seed, coord.x, coord.z);
    let mut origins = Vec::new();
    for (x, z) in TREE_COLUMNS {
        let world_x = coord.x * CHUNK_WIDTH as i32 + x as i32;
        let world_z = coord.z * CHUNK_WIDTH as i32 + z as i32;

        // Trees stand on top of the grass
        let height = (heights[x + z * CHUNK_WIDTH] as i32 as usize).clamp(0, CHUNK_HEIGHT - 1);
        let world_y = height as i32 + 1;
        origins.push(WorldCoord::new(world_x, world_y, world_z));
    }
    origins
}
