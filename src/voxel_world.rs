use crate::{ChunkData, ChunkCoord};

use better_default::Default;
use bevy::{
    prelude::*,
    platform::collections::HashMap,
};
#[cfg(all(feature = "chapter05", not(feature = "chapter06")))]
use bevy_inspector_egui::{egui, bevy_egui::{EguiContexts, EguiPrimaryContextPass}};

#[derive(Default)]
pub struct VoxelWorldPlugin;

#[derive(Default, Resource)]
pub struct VoxelWorld {
    chunks: HashMap<ChunkCoord, ChunkData>,
}

#[derive(Resource)]
pub struct ChunkMaterial(pub Handle<StandardMaterial>);

impl Plugin for VoxelWorldPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(all(feature = "chapter05", not(feature = "chapter06")))]
        app.add_systems(EguiPrimaryContextPass, chunk_debug_ui);

        app.init_resource::<VoxelWorld>()
            .add_systems(PreStartup, setup_chunk_material);
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

    pub fn iter(&self) -> impl Iterator<Item = (&ChunkCoord, &ChunkData)> {
        self.chunks.iter()
    }
}

#[cfg(all(not(feature = "chapter06")))]
fn setup_chunk_material(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let material = materials.add(
        Color::linear_rgb(0.2, 0.7, 0.3)
    );

    commands.insert_resource(ChunkMaterial(material));
}

#[cfg(all(feature = "chapter06", not(feature = "chapter07")))]
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

#[cfg(all(feature = "chapter05", not(feature = "chapter06")))]
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
