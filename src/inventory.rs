use crate::{
    BlockType,
    PlayerCamera,
    SharedVoxelAtlas,
    TerrainType,
    VoxelType,
    VoxelTypeItem,
    editor::EditorState,
    game_view_captured,
    spawn_player,
    voxel_atlas::VoxelAtlasAssets,
};

use bevy::{
    prelude::*,
    asset::uuid::Uuid,
    camera::NormalizedRenderTarget,
    input::mouse::{MouseScrollUnit, MouseWheel},
    picking::{PickingSystems, pointer::{Location, PointerAction, PointerId, PointerInput, PointerLocation}},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_inspector_egui::egui;

const INVENTORY_POINTER: PointerId = PointerId::Custom(Uuid::from_u128(0x513fdb1910924cd3b8855f7cd53a1841));

pub struct CreativeInventoryPlugin;

#[derive(Default, Resource)]
pub struct CreativeInventoryState {
    pub open: bool,
    cursor_was_locked: bool,
}

#[derive(Component)]
struct CreativeInventoryRoot;

#[derive(Component)]
struct InventoryGrid;

#[derive(Default, Resource)]
pub struct HeldInventoryItem(pub Option<VoxelTypeItem>);

#[derive(Component)]
struct CreativeInventoryItem(VoxelTypeItem);

#[derive(Component)]
struct CursorItemPreview;

impl Plugin for CreativeInventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CreativeInventoryState>()
            .init_resource::<HeldInventoryItem>()
            .add_systems(
                Startup,
                (spawn_creative_inventory.after(spawn_player), spawn_inventory_pointer),
            )
            .add_systems(First, forward_inventory_pointer.in_set(PickingSystems::PostInput))
            .add_systems(PreUpdate, sync_inventory_pointer
                .after(PickingSystems::ProcessInput)
                .before(PickingSystems::Backend))
            .add_systems(
                Update,
                (
                    toggle_creative_inventory,
                    scroll_creative_inventory,
                    update_cursor_preview,
                ),
            );
    }
}

fn spawn_creative_inventory(
    mut commands: Commands,
    atlas: Res<VoxelAtlasAssets>,
    voxel_atlas: Res<SharedVoxelAtlas>,
    camera: Single<Entity, With<PlayerCamera>>,
) {
    let items = BlockType::ALL
        .into_iter()
        .map(VoxelTypeItem::Block)
        .chain(TerrainType::ALL.into_iter().map(VoxelTypeItem::Terrain));

    commands.spawn((
        Name::new("Creative Inventory"),
        CreativeInventoryRoot,
        UiTargetCamera(*camera),
        GlobalZIndex(10),
        Node {
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            display: Display::None,
            ..default()
        },
    ))
    .with_children(|root| {
        root.spawn((
            Name::new("Cursor Item Preview"),
            CursorItemPreview,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: px(36),
                height: px(36),
                display: Display::None,
                ..default()
            },
            ImageNode::from_atlas_image(
                atlas.image.clone(),
                TextureAtlas {
                    layout: atlas.layout.clone(),
                    index: 0,
                },
            ),
            GlobalZIndex(30),
            Pickable::IGNORE,
        ));

        root.spawn((
            Name::new("Inventory Panel"),
            ZIndex(1),
            Node {
                width: px(640),
                max_width: percent(90),
                height: percent(75),
                max_height: px(560),
                flex_direction: FlexDirection::Column,
                padding: px(16).all(),
                row_gap: px(12),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.08, 0.08, 0.95)),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("Inventory"),
                TextFont {
                    font_size: 24.0.into(),
                    ..default()
                },
            ));

            panel.spawn((
                Name::new("Inventory Grid"),
                InventoryGrid,
                ScrollPosition::default(),
                Node {
                    display: Display::Grid,
                    width: percent(100),
                    flex_grow: 1.0,
                    min_height: px(0),
                    grid_template_columns: RepeatedGridTrack::flex(5, 1.0),
                    column_gap: px(8),
                    row_gap: px(8),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.02, 0.02, 0.02, 0.5)),
            ))
            .with_children(|grid| {
                for item in items {
                    let (voxel, name) = match item {
                        VoxelTypeItem::Block(block) => (
                            VoxelType::from_block(block),
                            format!("{block:?}"),
                        ),
                        VoxelTypeItem::Terrain(terrain) => (
                            VoxelType::from_terrain(terrain),
                            format!("{terrain:?}"),
                        ),
                        VoxelTypeItem::Air => unreachable!(),
                    };

                    let texture_index = voxel_atlas.0.icon(voxel);

                    grid.spawn((
                        CreativeInventoryItem(item),
                        Node {
                            height: px(88),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            row_gap: px(4),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.2, 0.2, 0.2, 0.9)),
                    ))
                    .observe(on_inventory_item_clicked)
                    .with_children(|cell| {
                        cell.spawn((
                            ImageNode::from_atlas_image(
                                atlas.image.clone(),
                                TextureAtlas {
                                    layout: atlas.layout.clone(),
                                    index: texture_index,
                                },
                            ),
                            Node {
                                width: px(44),
                                height: px(44),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ));

                        cell.spawn((
                            Text::new(name),
                            TextFont {
                                font_size: 12.0.into(),
                                ..default()
                            },
                            Pickable::IGNORE,
                        ));
                    });
                }
            });
        });

        root.spawn((
            Name::new("Inventory Backdrop"),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        ))
        .observe(clear_held_item);
    });
}

fn on_inventory_item_clicked(
    mut click: On<PointerClick>,
    items: Query<&CreativeInventoryItem>,
    mut held: ResMut<HeldInventoryItem>,
) {
    let Ok(item) = items.get(click.entity) else {
        return;
    };

    held.0 = if held.0 == Some(item.0) {
        None
    } else {
        Some(item.0)
    };

    click.propagate(false);
}

fn clear_held_item(
    mut click: On<PointerClick>,
    mut held: ResMut<HeldInventoryItem>,
) {
    held.0 = None;
    click.propagate(false);
}

fn spawn_inventory_pointer(mut commands: Commands) {
    commands.spawn(INVENTORY_POINTER);
}

fn cancel_inventory_pointer(commands: &mut Commands, location: Location) {
    commands.write_message(PointerInput::new(INVENTORY_POINTER, location.clone(), PointerAction::Cancel));
    for button in PointerButton::iter() {
        commands.write_message(PointerInput::new(INVENTORY_POINTER, location.clone(), PointerAction::Release(button)));
    }
}

fn game_view_position(rect: Option<egui::Rect>, position: Vec2, viewport_size: Vec2) -> Option<Vec2> {
    let rect = rect?;
    if !rect.contains(egui::pos2(position.x, position.y)) || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return None;
    }
    let local = position - vec2(rect.min.x, rect.min.y);
    Some(local / vec2(rect.width(), rect.height()) * viewport_size)
}

fn forward_inventory_pointer(
    mut commands: Commands,
    mut inputs: MessageReader<PointerInput>,
    editor: Res<EditorState>,
    inventory: Res<CreativeInventoryState>,
    camera: Single<&Camera, With<PlayerCamera>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut pointers: Query<(&PointerId, &mut PointerLocation)>,
) {
    let Some((_, mut pointer)) = pointers.iter_mut().find(|(id, _)| **id == INVENTORY_POINTER) else {
        return;
    };
    let viewport_size = camera.logical_viewport_size();
    let map_position = |position| {
        (inventory.open && editor.is_game_view_visible() && editor.is_game_view_active())
            .then(|| viewport_size.and_then(|size| game_view_position(editor.get_game_view().rect, position, size)))
            .flatten()
    };
    let target = NormalizedRenderTarget::Image(editor.get_game_view().image.clone().into());
    for input in inputs.read().filter(|input| input.pointer_id == PointerId::Mouse) {
        let Some(position) = map_position(input.location.position) else {
            if let Some(location) = pointer.location.take() {
                cancel_inventory_pointer(&mut commands, location);
            }
            continue;
        };
        let location = Location { target: target.clone(), position };
        let previous = pointer.location.as_ref().map(|location| location.position).unwrap_or(position);
        pointer.location = Some(location.clone());
        if matches!(input.action, PointerAction::Move { .. }) || previous != position {
            commands.write_message(PointerInput::new(
                INVENTORY_POINTER, location.clone(), PointerAction::Move { delta: position - previous },
            ));
        }
        if !matches!(input.action, PointerAction::Move { .. }) {
            commands.write_message(PointerInput::new(INVENTORY_POINTER, location, input.action));
        }
    }

    let position = window.cursor_position().and_then(map_position);
    if let Some(position) = position {
        let location = Location { target, position };
        if pointer.location.as_ref() != Some(&location) {
            let previous = pointer.location.as_ref().map(|location| location.position).unwrap_or(position);
            pointer.location = Some(location.clone());
            commands.write_message(PointerInput::new(
                INVENTORY_POINTER, location, PointerAction::Move { delta: position - previous },
            ));
        }
    } else if let Some(location) = pointer.location.take() {
        cancel_inventory_pointer(&mut commands, location);
    }
}

fn sync_inventory_pointer(
    editor: Res<EditorState>,
    inventory: Res<CreativeInventoryState>,
    camera: Single<&Camera, With<PlayerCamera>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut pointers: Query<(&PointerId, &mut PointerLocation)>,
) {
    let Some((_, mut pointer)) = pointers.iter_mut().find(|(id, _)| **id == INVENTORY_POINTER) else {
        return;
    };
    let location = if inventory.open && editor.is_game_view_visible() && editor.is_game_view_active() {
        window.cursor_position()
            .zip(camera.logical_viewport_size())
            .and_then(|(position, size)| game_view_position(editor.get_game_view().rect, position, size))
            .map(|position| Location {
                target: NormalizedRenderTarget::Image(editor.get_game_view().image.clone().into()),
                position,
            })
    } else {
        None
    };
    pointer.location = location;
}

fn update_cursor_preview(
    held: Res<HeldInventoryItem>,
    state: Res<CreativeInventoryState>,
    window: Single<&Window, With<PrimaryWindow>>,
    editor: Res<EditorState>,
    camera: Single<&Camera, With<PlayerCamera>>,
    atlas: Res<VoxelAtlasAssets>,
    voxel_atlas: Res<SharedVoxelAtlas>,
    preview: Single<(&mut Node, &mut ImageNode), With<CursorItemPreview>>,
) {
    let (mut node, mut image) = preview.into_inner();

    let Some(item) = held.0.filter(|_| state.open) else {
        node.display = Display::None;
        return;
    };

    let Some(position) = window.cursor_position()
        .zip(camera.logical_viewport_size())
        .and_then(|(position, size)| game_view_position(editor.get_game_view().rect, position, size))
    else {
        node.display = Display::None;
        return;
    };

    node.display = Display::Flex;
    node.left = px(position.x + 16.0);
    node.top = px(position.y + 16.0);

    let voxel: VoxelType = item.into();
    let texture_index = voxel_atlas.0.icon(voxel);

    if image.texture_atlas.as_ref().is_none_or(|atlas| atlas.index != texture_index) {
        *image = ImageNode::from_atlas_image(
            atlas.image.clone(),
            TextureAtlas {
                layout: atlas.layout.clone(),
                index: texture_index,
            },
        );
    }
}

fn toggle_creative_inventory(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<CreativeInventoryState>,
    mut held: ResMut<HeldInventoryItem>,
    mut root: Single<&mut Node, With<CreativeInventoryRoot>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if !keyboard.just_pressed(KeyCode::KeyE) {
        return;
    }

    state.open = !state.open;

    if state.open {
        state.cursor_was_locked = cursor.grab_mode == CursorGrabMode::Locked;

        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;

        root.display = Display::Flex;
    } else {
        root.display = Display::None;

        // Closing the inventory also drops the held item.
        held.0 = None;

        if state.cursor_was_locked {
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        }
    }
}

fn scroll_creative_inventory(
    mut mouse: MessageReader<MouseWheel>,
    state: Res<CreativeInventoryState>,
    grid: Single<(&mut ScrollPosition, &ComputedNode), With<InventoryGrid>>,
) {
    if !state.open {
        return;
    }

    let (mut position, computed) = grid.into_inner();

    let max_y = (
        computed.content_size().y - computed.size().y
    ).max(0.0) * computed.inverse_scale_factor();

    for event in mouse.read() {
        let delta = match event.unit {
            MouseScrollUnit::Line => event.y * 40.0,
            MouseScrollUnit::Pixel => event.y,
        };

        position.y = (position.y - delta).clamp(0.0, max_y);
    }
}

pub struct HotbarPlugin;

impl Plugin for HotbarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hotbar>()
            .add_systems(Startup, spawn_hotbar.after(spawn_player))
            .add_systems(Update, (
                (
                    hotbar_keyboard_input,
                    hotbar_scroll_input,
                ).run_if(game_view_captured),
                (
                    update_hotbar_ui,
                    update_hotbar_icons,
                ).after(hotbar_keyboard_input),
            ));
    }
}

#[derive(Resource)]
pub struct Hotbar {
    slots: [VoxelTypeItem; 10],
    selected: usize,
}

/// GUI component for a single hotbar slot, which holds the icon of the block
/// and a visual indicator if it is selected.
#[derive(Component)]
struct HotbarSlot {
    index: usize,
}

#[derive(Component)]
struct HotbarIcon {
    index: usize,
}

impl Default for Hotbar {
    fn default() -> Self {
        Self {
            slots: [
                VoxelTypeItem::Terrain(TerrainType::Grass),
                VoxelTypeItem::Terrain(TerrainType::Dirt),
                VoxelTypeItem::Terrain(TerrainType::Stone),
                VoxelTypeItem::Terrain(TerrainType::Sand),
                VoxelTypeItem::Terrain(TerrainType::Ice),
                VoxelTypeItem::Block(BlockType::Log),
                VoxelTypeItem::Block(BlockType::Leaves),
                VoxelTypeItem::Block(BlockType::BrickGrey),
                VoxelTypeItem::Block(BlockType::CottonBlue),
                VoxelTypeItem::Block(BlockType::Glass),
            ],
            selected: 0,
        }
    }
}

impl Hotbar {
    pub fn selected(&self) -> VoxelTypeItem {
        self.slots[self.selected]
    }
}

fn spawn_hotbar(
    mut commands: Commands,
    hotbar: Res<Hotbar>,
    atlas: Res<VoxelAtlasAssets>,
    voxel_atlas: Res<SharedVoxelAtlas>,
    camera: Single<Entity, With<PlayerCamera>>,
) {
    commands.spawn((
        UiTargetCamera(*camera),
        GlobalZIndex(20),
        Pickable::IGNORE,
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexEnd,
            align_items: AlignItems::Center,
            padding: px(24).bottom(),
            ..default()
        },
    ))
    .with_children(|ui| {
        ui.spawn((
            Name::new("Hotbar"),
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(4),
                padding: px(4).all(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.05, 0.05, 0.8)),
        ))
        .with_children(|bar| {
            for (index, slot) in hotbar.slots.iter().copied().enumerate() {
                let voxel_type: VoxelType = slot.into();

                let texture_index = voxel_atlas.0.icon(voxel_type);

                bar.spawn((
                    HotbarSlot { index },
                    Node {
                        width: px(48),
                        height: px(48),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.3, 0.3, 0.3, 0.9)),
                ))
                .observe(on_hotbar_slot_clicked)
                .with_children(|slot| {
                    slot.spawn((
                        HotbarIcon { index },
                        ImageNode::from_atlas_image(
                            atlas.image.clone(),
                            TextureAtlas {
                                layout: atlas.layout.clone(),
                                index: texture_index,
                            },
                        ),
                        Node {
                            width: px(36),
                            height: px(36),
                            ..default()
                        },
                        Pickable::IGNORE,
                    ));
                });
            }
        });
    });
}

fn on_hotbar_slot_clicked(
    mut click: On<PointerClick>,
    slots: Query<&HotbarSlot>,
    mut hotbar: ResMut<Hotbar>,
    mut held: ResMut<HeldInventoryItem>,
) {
    let Ok(slot) = slots.get(click.entity) else {
        return;
    };

    if let Some(item) = held.0.take() {
        hotbar.slots[slot.index] = item;
    }

    click.propagate(false);
}

fn update_hotbar_icons(
    hotbar: Res<Hotbar>,
    voxel_atlas: Res<SharedVoxelAtlas>,
    mut icons: Query<(&HotbarIcon, &mut ImageNode)>,
) {
    if !hotbar.is_changed() {
        return;
    }

    for (icon, mut image) in &mut icons {
        let voxel: VoxelType = hotbar.slots[icon.index].into();
        let texture_index = voxel_atlas.0.icon(voxel);

        if let Some(texture_atlas) = image.texture_atlas.as_mut() {
            texture_atlas.index = texture_index;
        }
    }
}

fn hotbar_keyboard_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut hotbar: ResMut<Hotbar>,
) {
    let keys = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
        KeyCode::Digit0,
    ];

    for (index, key) in keys.iter().enumerate() {
        if keyboard.just_pressed(*key) {
            hotbar.selected = index;
            return;
        }
    }
}

fn hotbar_scroll_input(
    mut mouse: MessageReader<MouseWheel>,
    mut hotbar: ResMut<Hotbar>,
) {
    for event in mouse.read() {
        if event.y < 0.0 {
            hotbar.selected = (hotbar.selected + 1) % 10;
        } else if event.y > 0.0 {
            hotbar.selected = (hotbar.selected + 9) % 10;
        }
    };
}

fn update_hotbar_ui(
    hotbar: Res<Hotbar>,
    mut slots: Query<(&HotbarSlot, &mut BackgroundColor)>,
) {
    if !hotbar.is_changed() {
        return;
    }

    for (slot, mut background) in &mut slots {
        background.0 = if slot.index == hotbar.selected {
            Color::WHITE
        } else {
            Color::BLACK
        };
    }
}
