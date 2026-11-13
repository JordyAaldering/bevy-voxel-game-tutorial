use crate::{ChunkData, ChunkCoord};

use better_default::Default;
use bevy::{
    prelude::*,
    platform::collections::HashMap,
};
#[cfg(feature = "chapter05")]
use bevy_inspector_egui::{
    egui,
    bevy_egui::{EguiContexts, EguiPrimaryContextPass},
};

#[derive(Default)]
pub struct VoxelWorldPlugin;

#[derive(Default, Resource)]
pub struct VoxelWorld {
    chunks: HashMap<ChunkCoord, ChunkData>,
}

impl Plugin for VoxelWorldPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "chapter05")]
        app.add_systems(EguiPrimaryContextPass, chunk_debug_ui);
        app.init_resource::<VoxelWorld>();
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

#[cfg(feature = "chapter05")]
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
