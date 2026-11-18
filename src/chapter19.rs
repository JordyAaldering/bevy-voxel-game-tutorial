mod biome;
mod chunk;
mod chunk_neighbors;
mod coords;
mod debug_gizmos;
mod density;
mod editor;
mod gamemode;
mod marching;
mod meshdata;
mod noise;
mod ore;
mod physics;
mod player;
mod raycast;
mod scene;
mod screenshot;
mod structure;
mod telemetry;
mod voxel_atlas;
mod voxel_world;

use crate::{
    biome::*,
    chunk::*,
    chunk_neighbors::*,
    coords::*,
    debug_gizmos::*,
    density::*,
    editor::*,
    gamemode::*,
    meshdata::*,
    noise::*,
    ore::*,
    physics::*,
    player::*,
    raycast::*,
    scene::*,
    screenshot::*,
    structure::*,
    telemetry::*,
    voxel::*,
    voxel_atlas::*,
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

mod voxel {
    /// A compact representation of a voxel type.
    ///
    /// A voxel can represent air, a block type, or a terrain type. These categories
    /// are encoded into a single `u16` to keep voxel storage memory-efficient.
    ///
    /// The 16 bits are divided into two fields:
    ///
    /// - The upper 3 bits identify the voxel category.
    /// - The lower 13 bits identify the specific type within that category.
    ///
    /// This allows for up to 8 categories, each with up to 8192 distinct types.
    ///
    /// The category discriminants are:
    ///
    /// | Category | Bits  |
    /// |----------|-------|
    /// | Air      | `000` |
    /// | Block    | `001` |
    /// | Terrain  | `010` |
    ///
    /// The individual block and terrain types are defined as separate enums.
    /// Their discriminants are stored in the lower 13 bits, allowing them to
    /// retain the type safety and readability of enums without requiring
    /// a larger representation for every voxel.
    ///
    /// A regular enum containing these categories would require the compiler
    /// to represent both the category discriminant and its associated payload.
    /// By encoding both explicitly, we guarantee that each `VoxelType` occupies
    /// exactly two bytes.
    ///
    /// Most operations can work directly on the packed representation using
    /// bit masks, avoiding the need to unpack the value. The `unpack` method is
    /// provided for cases where the actual `BlockType` or `TerrainType` is needed.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[derive(wincode::SchemaWrite, wincode::SchemaRead)]
    #[derive(serde::Serialize, serde::Deserialize)]
    #[repr(transparent)]
    pub struct VoxelType(u16);

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum VoxelTypeItem {
        Air,
        Block(BlockType),
        Terrain(TerrainType),
    }

    impl Into<VoxelType> for VoxelTypeItem {
        fn into(self) -> VoxelType {
            match self {
                Self::Air => VoxelType::AIR,
                Self::Block(block) => VoxelType::from_block(block),
                Self::Terrain(terrain) => VoxelType::from_terrain(terrain),
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[derive(wincode::SchemaWrite, wincode::SchemaRead)]
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "lowercase")]
    #[repr(u16)]
    pub enum BlockType {
        Log,
        Leaves,
        BrickGrey,
        BrickRed,
        Cactus,
        CottonBlue,
        CottonGreen,
        CottonRed,
        CottonWhite,
        Glass,
    }

    impl BlockType {
        pub const ALL: [Self; 10] = [
            Self::Log,
            Self::Leaves,
            Self::BrickGrey,
            Self::BrickRed,
            Self::Cactus,
            Self::CottonBlue,
            Self::CottonGreen,
            Self::CottonRed,
            Self::CottonWhite,
            Self::Glass,
        ];
    }

    impl TryFrom<u16> for BlockType {
        type Error = String;

        fn try_from(value: u16) -> std::prelude::v1::Result<Self, Self::Error> {
            Ok(match value {
                0 => Self::Log,
                1 => Self::Leaves,
                2 => Self::BrickGrey,
                3 => Self::BrickRed,
                4 => Self::Cactus,
                5 => Self::CottonBlue,
                6 => Self::CottonGreen,
                7 => Self::CottonRed,
                8 => Self::CottonWhite,
                9 => Self::Glass,
                _ => return Err(format!("Invalid block type: {value}")),
            })
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[derive(wincode::SchemaWrite, wincode::SchemaRead)]
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "lowercase")]
    #[repr(u16)]
    pub enum TerrainType {
        Grass,
        Dirt,
        Stone,
        StoneCoal,
        StoneIron,
        StoneGold,
        StoneDiamond,
        Sand,
        Snow,
        Ice,
    }

    impl TerrainType {
        pub const ALL: [Self; 10] = [
            Self::Grass,
            Self::Dirt,
            Self::Stone,
            Self::StoneCoal,
            Self::StoneIron,
            Self::StoneGold,
            Self::StoneDiamond,
            Self::Sand,
            Self::Snow,
            Self::Ice,
        ];
    }

    impl TryFrom<u16> for TerrainType {
        type Error = String;

        fn try_from(value: u16) -> std::prelude::v1::Result<Self, Self::Error> {
            Ok(match value {
                0 => Self::Grass,
                1 => Self::Dirt,
                2 => Self::Stone,
                3 => Self::StoneCoal,
                4 => Self::StoneIron,
                5 => Self::StoneGold,
                6 => Self::StoneDiamond,
                7 => Self::Sand,
                8 => Self::Snow,
                9 => Self::Ice,
                _ => return Err(format!("Invalid terrain type: {value}")),
            })
        }
    }

    impl VoxelType {
        /// The lower 13 bits are the type, the upper 3 bits are the category.
        const TYPE_MASK: u16 = 0x1FFF;
        const CATEGORY_MASK: u16 = 0xE000;

        const DISCRIMINANT_AIR: u16 = 0b000 << 13;
        const DISCRIMINANT_BLOCK: u16 = 0b001 << 13;
        const DISCRIMINANT_TERRAIN: u16 = 0b010 << 13;

        pub const AIR: Self = Self(Self::DISCRIMINANT_AIR);

        pub const fn from_block(block: BlockType) -> Self {
            Self(Self::DISCRIMINANT_BLOCK | (block as u16))
        }

        pub const fn from_terrain(terrain: TerrainType) -> Self {
            Self(Self::DISCRIMINANT_TERRAIN | (terrain as u16))
        }

        pub const fn is_transparent(self) -> bool {
            self.is_air()
        }

        pub const fn is_solid(self) -> bool {
            !self.is_air()
        }

        pub const fn is_air(self) -> bool {
            (self.0 & Self::CATEGORY_MASK) == Self::DISCRIMINANT_AIR
        }

        pub const fn is_block(self) -> bool {
            (self.0 & Self::CATEGORY_MASK) == Self::DISCRIMINANT_BLOCK
        }

        pub const fn is_terrain(self) -> bool {
            (self.0 & Self::CATEGORY_MASK) == Self::DISCRIMINANT_TERRAIN
        }

        pub const fn is_terrain_id(self, id: TerrainType) -> bool {
            self.is_terrain() && (self.0 & Self::TYPE_MASK) == id as u16
        }

        pub fn unpack(self) -> Result<VoxelTypeItem, String> {
            let discriminant = 0xE000 & self.0;
            let payload = 0x1FFF & self.0;
            Ok(match discriminant {
                Self::DISCRIMINANT_AIR => VoxelTypeItem::Air,
                Self::DISCRIMINANT_BLOCK => VoxelTypeItem::Block(BlockType::try_from(payload)?),
                Self::DISCRIMINANT_TERRAIN => VoxelTypeItem::Terrain(TerrainType::try_from(payload)?),
                _ => return Err(format!("Invalid voxel discriminant: {discriminant:#X}")),
            })
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
            GpuPlugin::default(),
        ))
        .insert_resource(WorldConfig::load_or_create().unwrap_or_else(|e| panic!("Failed to load world config: {e}")))
        .insert_resource(Structures::load().unwrap_or_else(|e| panic!("Failed to load structures: {e}")))
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
