mod coords;
mod density;
mod editor;
mod inventory;
mod meshdata;
mod raycast;
mod screenshot;
mod structure;
mod voxel_atlas;

use crate::{
    coords::*, editor::{EditorPlugin, EditorState}, inventory::CreativeInventoryPlugin, structure::{Structure, StructureVoxel}, voxel_atlas::{VoxelAtlas, VoxelAtlasAssets, build_voxel_atlas},
};

use std::sync::Arc;

use bevy::{input::mouse::AccumulatedMouseMotion, platform::collections::HashMap, prelude::*, window::{CursorGrabMode, CursorOptions, PrimaryWindow}};
use bevy_inspector_egui::bevy_egui::EguiPlugin;

pub const CHUNK_WIDTH: usize = 0;
pub const CHUNK_HEIGHT: usize = 0;

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
    const ALL: [Self; 10] = [
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
    const ALL: [Self; 10] = [
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

    pub const fn is_block_id(self, id: BlockType) -> bool {
        self.is_block() && (self.0 & Self::TYPE_MASK) == id as u16
    }

    pub const fn is_terrain_id(self, id: TerrainType) -> bool {
        self.is_terrain() && (self.0 & Self::TYPE_MASK) == id as u16
    }

    pub const fn block_id(self) -> Option<u16> {
        if self.is_block() {
            Some(self.0 & Self::TYPE_MASK)
        } else {
            None
        }
    }

    pub const fn terrain_id(self) -> Option<u16> {
        if self.is_terrain() {
            Some(self.0 & Self::TYPE_MASK)
        } else {
            None
        }
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

#[derive(Resource)]
struct SharedVoxelAtlas(Arc<VoxelAtlas>);

#[derive(Component)]
struct PlayerCamera;

#[derive(Component)]
struct EditorCamera;

fn game_view_captured(
    inventory: Res<inventory::CreativeInventoryState>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
) -> bool {
    cursor.grab_mode == CursorGrabMode::Locked
        && !inventory.open
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Resource)]
enum GameMode {
    #[default]
    Spectator,
}

#[derive(Default, Component)]
struct Player;

fn spawn_player(
    mut commands: Commands,
    editor_state: Res<EditorState>,
) {
    commands.spawn((
            Name::new("Player"),
            Player,
            Projection::Perspective(PerspectiveProjection {
                fov: 0.9,
                ..default()
            }),
            Msaa::Off,
            InheritedVisibility::default(),
            Transform::from_xyz(1.5, 2.0, 4.0)
                .looking_at(vec3(0.5, 0.5, 0.5), Vec3::Y),
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
                    translation: Vec3::new(0.0, 1.6, 0.0),
                    ..default()
                },
            ));
        });
}

fn player_look(
    mouse: Res<AccumulatedMouseMotion>,
    mut camera: Single<&mut Transform, (With<PlayerCamera>, Without<Player>)>,
    mut player: Single<&mut Transform, (With<Player>, Without<PlayerCamera>)>,
) {
    let delta = mouse.delta;

    if delta != Vec2::ZERO {
        // Horizontal movement rotates the entire player around the world Y axis
        player.rotate_y(-delta.x * 0.0025);
        // Vertical movement rotates only the camera around its local X axis
        let (yaw, mut pitch, roll) = camera.rotation.to_euler(EulerRot::YXZ);

        pitch -= delta.y * 0.0025;
        pitch = pitch.clamp(-1.4, 1.4);

        camera.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, roll);
    }
}

fn move_spectator(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    player: Single<(&mut Transform, &mut Player)>,
) {
    let (mut transform, _) = player.into_inner();

    let delta = vec3(
        f32::from(keyboard.pressed(KeyCode::KeyD)) - f32::from(keyboard.pressed(KeyCode::KeyA)),
        f32::from(keyboard.pressed(KeyCode::Space)) - f32::from(keyboard.pressed(KeyCode::ShiftLeft)),
        f32::from(keyboard.pressed(KeyCode::KeyW)) - f32::from(keyboard.pressed(KeyCode::KeyS)),
    );

    if delta != Vec3::ZERO {
        let direction = delta.x * transform.right() + delta.y * Vec3::Y + delta.z * transform.forward();
        transform.translation += 10.0 * time.delta_secs() * direction.normalize();
    }
}

fn toggle_cursor(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    editor_state: Res<EditorState>,
    window: Single<&Window, With<PrimaryWindow>>,
    inventory: Res<inventory::CreativeInventoryState>,
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
        && !inventory.open
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

#[derive(Resource)]
struct ChunkMaterial(Handle<StandardMaterial>);

fn setup_chunk_material(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let (atlas, image) = build_voxel_atlas(&mut images)
        .unwrap_or_else(|e| panic!("Failed to build voxel atlas: {e}"));

    let texture = images.add(image);
    let layout = atlas_layouts.add(atlas.layout().clone());

    commands.insert_resource(SharedVoxelAtlas(Arc::new(atlas)));
    commands.insert_resource(VoxelAtlasAssets {
        image: texture.clone(),
        layout,
    });

    let material = materials.add(StandardMaterial {
        base_color_texture: Some(texture),
        perceptual_roughness: 0.9,
        reflectance: 0.1,
        ..default()
    });

    commands.insert_resource(ChunkMaterial(material));
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest()),
            EguiPlugin::default(),
            EditorPlugin,
            CreativeInventoryPlugin,
        ))
        .init_resource::<GameMode>()
        .init_resource::<StructureWorld>()
        .add_systems(PreStartup, setup_chunk_material)
        .add_systems(Startup, (
            setup_scene,
            spawn_player,
        ))
        .add_systems(Update, (
            toggle_cursor,
            player_look,
            move_spectator,
            draw_origin,
        ))
        .run()
}

fn setup_scene(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            illuminance: 10_000.0,
            ..default()
        },
        Transform::from_rotation(
            Quat::from_euler(EulerRot::XYZ, -0.8, -0.8, 0.0),
        ),
    ));
}

#[derive(Resource, Default)]
struct StructureWorld {
    voxels: HashMap<WorldCoord, StructureVoxel>,
}

fn draw_origin(mut gizmos: Gizmos) {
    let origin = Vec3::ZERO;
    let size = 0.5;

    // Three axes through the structure origin.
    gizmos.line(
        origin - Vec3::X * size,
        origin + Vec3::X * size,
        Color::srgb(1.0, 0.0, 0.0),
    );

    gizmos.line(
        origin - Vec3::Y * size,
        origin + Vec3::Y * size,
        Color::srgb(0.0, 1.0, 0.0),
    );

    gizmos.line(
        origin - Vec3::Z * size,
        origin + Vec3::Z * size,
        Color::srgb(0.0, 0.0, 1.0),
    );

    gizmos.cube(
        Transform::from_translation(origin)
            .with_scale(Vec3::splat(0.15)),
        Color::WHITE,
    );
}
