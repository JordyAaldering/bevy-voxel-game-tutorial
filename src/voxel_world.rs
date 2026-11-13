use crate::{ChunkData, ChunkCoord};
#[cfg(since07)]
use crate::{VoxelType, WorldCoord};

use better_default::Default;
use bevy::{
    prelude::*,
    platform::collections::HashMap,
};
#[cfg(only05)]
use bevy_inspector_egui::{egui, bevy_egui::{EguiContexts, EguiPrimaryContextPass}};

#[cfg(since07)]
pub const ATLAS_COLUMNS: u32 = 9;
#[cfg(since07)]
pub const ATLAS_ROWS: u32 = 10;

#[derive(Default)]
pub struct VoxelWorldPlugin;

#[derive(Default, Resource)]
pub struct VoxelWorld {
    chunks: HashMap<ChunkCoord, ChunkData>,
}

#[cfg(since07)]
#[derive(Resource)]
pub struct ChunkAtlas(pub Handle<TextureAtlasLayout>);

#[derive(Resource)]
pub struct ChunkMaterial(pub Handle<StandardMaterial>);

impl Plugin for VoxelWorldPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(only05)]
        app.add_systems(EguiPrimaryContextPass, chunk_debug_ui);

        app.init_resource::<VoxelWorld>();
        app.add_systems(PreStartup, setup_chunk_material);

        #[cfg(since07)]
        app.add_systems(PreStartup, setup_chunk_atlas);
    }
}

impl VoxelWorld {
    pub fn add_chunk(&mut self, pos: ChunkCoord, chunk: ChunkData) {
        debug_assert!(!self.chunks.contains_key(&pos));
        self.chunks.insert(pos, chunk);
    }

    pub fn get_chunk(&self, pos: ChunkCoord) -> Option<&ChunkData> {
        self.chunks.get(&pos)
    }

    #[cfg(since07)]
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

    #[cfg(since07)]
    pub fn is_solid(&self, pos: WorldCoord) -> bool {
        self.get_voxel(pos)
            .is_some_and(VoxelType::is_solid)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ChunkCoord, &ChunkData)> {
        self.chunks.iter()
    }
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
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::splat(ATLAS_COLUMNS * ATLAS_ROWS),
        ATLAS_COLUMNS,
        ATLAS_ROWS,
        None,
        None,
    ));

    commands.insert_resource(ChunkAtlas(layout));
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
