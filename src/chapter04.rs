mod chunk;
mod coords;
mod debug_gizmos;
mod meshdata;
mod player;
mod scene;
mod screenshot;

use crate::{
    chunk::*,
    coords::*,
    debug_gizmos::*,
    meshdata::*,
    player::*,
    scene::*,
    screenshot::*,
    voxel::*,
};

use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_inspector_egui::{
    bevy_egui::prelude::*,
    quick::WorldInspectorPlugin,
};

pub const CHUNK_WIDTH: usize = 3;
pub const CHUNK_HEIGHT: usize = 4;

mod voxel {
    #[derive(Clone, Copy)]
    pub enum VoxelType {
        Air,
        Grass,
    }

    impl VoxelType {
        pub fn is_transparent(self) -> bool {
            matches!(self, VoxelType::Air)
        }
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            EguiPlugin::default(),
            WorldInspectorPlugin::default(),
            ScreenshotPlugin::default(),
            DebugGizmosPlugin { wireframe: true, ..default() },
            PlayerPlugin::default(),
        ))
        .add_systems(Startup, (
            global_illumination.spawn(),
            chunk.spawn(),
        ))
        .run()
}

fn is_focused(cursor: Single<&CursorOptions, With<PrimaryWindow>>) -> bool {
    cursor.grab_mode == CursorGrabMode::Locked
}

fn chunk() -> impl Scene {
    let coord = ChunkCoord::new(0, 0);
    let chunk = generate_chunk(coord);
    bsn! {
        Name(format!("Chunk {coord}"))
        Transform::from_translation(coord.position())
        Mesh3d(asset_value(build_mesh(&chunk)))
        MeshMaterial3d<StandardMaterial>(asset_value(Color::linear_rgb(0.2, 0.7, 0.3)))
    }
}

fn generate_chunk(coord: ChunkCoord) -> ChunkData {
    let mut chunk = ChunkData::new(coord);

    for z in 0..CHUNK_WIDTH {
        let max_y = CHUNK_HEIGHT / 2;
        for y in 0..=max_y {
            for x in 0..CHUNK_WIDTH {
                let pos = VoxelCoord::new(x, y, z);
                chunk[pos] = VoxelType::Grass;
            }
        }
    }

    chunk
}

fn build_mesh(
    chunk: &ChunkData,
) -> Mesh {
    let mut mesh = MeshData::default();

    for (voxel_pos, _) in chunk.iter()
        .filter(|(_, v)| !matches!(v, VoxelType::Air))
    {
        for direction in Direction::ALL {
            if is_face_visible(chunk, voxel_pos, direction) {
                let origin = voxel_pos.local_position();
                mesh.add_quad(direction, origin);
            }
        }
    }

    mesh.build()
}

fn is_face_visible(
    chunk: &ChunkData,
    voxel_pos: VoxelCoord,
    direction: Direction,
) -> bool {
    chunk.get_neighbor_voxel(voxel_pos, direction)
        .is_none_or(VoxelType::is_transparent)
}
