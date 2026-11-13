mod chunk;
mod chunk_neighbors;
mod coords;
mod debug_gizmos;
mod editor;
mod gamemode;
mod meshdata;
mod noise;
mod physics;
mod player;
mod raycast;
mod screenshot;
mod telemetry;
mod voxel_atlas_ch15;
mod voxel_world;

use crate::{
    chunk::*,
    chunk_neighbors::*,
    coords::*,
    debug_gizmos::*,
    editor::*,
    gamemode::*,
    meshdata::*,
    noise::*,
    physics::*,
    player::*,
    raycast::*,
    screenshot::*,
    telemetry::*,
    voxel::*,
    voxel_atlas_ch15::*,
    voxel_world::*,
};

use std::{fs, io, path::{Path, PathBuf}};

use bevy::{
    prelude::*,
    app::OnAppExitSystems,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_inspector_egui::bevy_egui::prelude::*;

pub const CHUNK_WIDTH: usize = 32;
pub const CHUNK_HEIGHT: usize = 64;

const SAVE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/savedata/world");

#[derive(Clone, Resource)]
struct TerrainConfig {
    seed: i32,
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

mod voxel {
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
        pub const ALL: [Self; 21] = [
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

        pub fn is_transparent(self) -> bool {
            matches!(self, VoxelType::Air)
        }

        pub fn is_solid(self) -> bool {
            !matches!(self, VoxelType::Air)
        }
    }
}

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

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest()),
            EguiPlugin::default(),
            EditorPlugin,
            TelemetryPlugin,
            ScreenshotPlugin::default(),
            DebugGizmosPlugin::default(),
            GameModePlugin::default(),
            PlayerPlugin::default(),
            PhysicsPlugin::new(is_gamemode_survival),
            VoxelWorldPlugin::default(),
        ))
        .init_resource::<TerrainConfig>()
        .add_systems(Startup, (
            global_illumination.spawn(),
        ))
        .add_systems(Last, (
            save_on_exit.in_set(OnAppExitSystems),
        ))
        .run()
}

fn is_focused(cursor: Single<&CursorOptions, With<PrimaryWindow>>) -> bool {
    cursor.grab_mode == CursorGrabMode::Locked
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

fn global_illumination() -> impl Scene {
    bsn! {
        DirectionalLight {
            shadow_maps_enabled: true,
            shadow_normal_bias: 1.0,
            shadow_depth_bias: 0.05,
        }
        Transform::from_rotation(
            Quat::from_euler(EulerRot::XYZ, 6.0, -7.0, 0.0)
        )
    }
}

fn save_on_exit(
    mut exits: MessageReader<AppExit>,
    world: Res<VoxelWorld>,
    player: Single<(&Transform, &Player)>,
    camera: Single<&Transform, (With<PlayerCamera>, Without<Player>)>,
) {
    if exits.read().next().is_none() {
        return;
    }

    world.save();

    let (transform, player) = *player;
    WorldSave {
        player_position: transform.translation.to_array(),
        player_rotation: transform.rotation.to_array(),
        camera_rotation: camera.rotation.to_array(),
        player_velocity: player.velocity.to_array(),
    }
    .save();
}
