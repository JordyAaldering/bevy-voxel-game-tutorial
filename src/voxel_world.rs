use crate::{ChunkCoord, ChunkData};
#[cfg(since07)]
use crate::{VoxelType, WorldCoord};
#[cfg(since08)]
use crate::{Chunk, ChunkNeighbors, NeedsRemesh, CHUNK_WIDTH};
#[cfg(since10)]
use crate::{NeedsDespawn, Player};

#[cfg(since08)]
use std::sync::Arc;

use better_default::Default;
use bevy::{
    prelude::*,
    platform::collections::HashMap,
};
#[cfg(since08)]
use bevy::{
    math::CompassQuadrant,
    tasks::{AsyncComputeTaskPool, futures_lite::future, Task, block_on},
};
#[cfg(only05)]
use bevy_inspector_egui::{egui, bevy_egui::{EguiContexts, EguiPrimaryContextPass}};

#[cfg(since07)]
pub const ATLAS_COLUMNS: u32 = 9;
#[cfg(since07)]
pub const ATLAS_ROWS: u32 = 10;

#[cfg(since10)]
const CHUNK_LOAD_RADIUS: i32 = 2;

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

#[cfg(since07)]
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

        #[cfg(since07)]
        app.add_systems(PreStartup, setup_chunk_atlas);

        #[cfg(since08)]
        app.add_systems(PostUpdate, (
            poll_mesh_tasks,
            remesh_chunks,
        ).chain());

        #[cfg(since10)]
        app.add_systems(Update, (
            spawn_chunks_around_player,
            retire_distant_chunks,
            despawn_retired_chunks,
            poll_mesh_tasks,
            remesh_chunks,
        ).chain());
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

    pub fn is_solid(&self, pos: WorldCoord) -> bool {
        self.get_voxel(pos)
            .is_some_and(VoxelType::is_solid)
    }
}

#[cfg(since08)]
impl VoxelWorld {
    pub fn get_chunk_mut(&mut self, pos: ChunkCoord) -> Option<&mut TChunkData> {
        self.chunks.get_mut(&pos)
    }

    /// Spawns an entity for the chunk at `coord`. The voxel data is generated only
    /// if it does not exist yet, so earlier modifications survive unloading.
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

    // fn get_chunk_neighbor(&self, pos: ChunkCoord, dx: i32, dz: i32) -> Option<&ChunkData> {
    //     let neighbor_pos = ChunkCoord::new(pos.x + dx, pos.z + dz);
    //     self.chunks.get(&neighbor_pos).map(|chunk| &**chunk)
    // }
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

#[cfg(since06)]
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
}

#[cfg(since07)]
fn setup_chunk_atlas(
    mut commands: Commands,
) {
    commands.insert_resource(VoxelAtlas(TextureAtlasLayout::from_grid(
        UVec2::splat(ATLAS_COLUMNS * ATLAS_ROWS),
        ATLAS_COLUMNS,
        ATLAS_ROWS,
        None,
        None,
    )));
}

/// Starts building the mesh of every chunk marked with [`NeedsRemesh`] on a background thread.
/// A chunk that is still being meshed stays marked, and is meshed again once that task is done.
#[cfg(since08)]
fn remesh_chunks(
    mut commands: Commands,
    world: Res<VoxelWorld>,
    atlas: Res<VoxelAtlas>,
    chunks: Query<(Entity, &Chunk), (With<NeedsRemesh>, Without<MeshTask>)>,
) {
    let pool = AsyncComputeTaskPool::get();

    for (entity, chunk) in &chunks {
        let Some(data) = world.get_chunk(chunk.coord()) else {
            continue;
        };

        // The task cannot borrow the world, so it gets shared handles to the data it needs
        let data = Arc::clone(data);
        let neighbors = ChunkNeighbors::from(chunk.coord(), &world.chunks);
        let atlas: TextureAtlasLayout = atlas.0.clone();
        let task = pool.spawn(async move { data.build_mesh(&neighbors, &atlas) });

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
    config: Res<crate::TerrainConfig>,
) {
    let center = ChunkCoord::from_position(player.translation.x, player.translation.z);

    for dz in -CHUNK_LOAD_RADIUS..=CHUNK_LOAD_RADIUS {
        for dx in -CHUNK_LOAD_RADIUS..=CHUNK_LOAD_RADIUS {
            let coord = ChunkCoord::new(center.x + dx, center.z + dz);

            if world.is_loaded(coord) {
                continue;
            }

            let entity = world.spawn_chunk(&mut commands, coord,
                |chunk| crate::generate_chunk(chunk, &config));
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
        }

        commands.entity(entity).despawn();
    }
}
