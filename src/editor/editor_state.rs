use super::{EditorTab, EditorTabViewer, ViewTarget};

use std::any::TypeId;

use bevy::{
    prelude::*,
    camera::RenderTarget,
};
use bevy_inspector_egui::{
    bevy_inspector::hierarchy::SelectedEntities,
    egui,

};
use egui_dock::{DockArea, DockState, NodeIndex, Style};

#[derive(Resource)]
pub(super) struct EditorState {
    state: DockState<EditorTab>,
    active_tab: Option<EditorTab>,
    game_view: ViewTarget,
    editor_view: ViewTarget,
    selected_entities: SelectedEntities,
    selected_resource: Option<(TypeId, String)>,
}

impl Default for EditorState {
    fn default() -> Self {
        let mut state = DockState::new(vec![EditorTab::GameView, EditorTab::EditorView]);
        let tree = state.main_surface_mut();
        let [game, _inspector] = tree.split_right(NodeIndex::root(), 0.75, vec![EditorTab::Inspector]);
        let [game, _hierarchy] = tree.split_left(game, 0.2, vec![EditorTab::Hierarchy]);
        let [_game, _bottom] = tree.split_below(game, 0.8, vec![EditorTab::Resources]);
        Self {
            state,
            active_tab: None,
            game_view: ViewTarget::default(),
            editor_view: ViewTarget::default(),
            selected_entities: SelectedEntities::default(),
            selected_resource: None,
        }
    }
}

impl EditorState {
    pub(super) fn set_game_view(&mut self, view: ViewTarget) {
        self.game_view = view;
    }

    pub(super) fn set_editor_view(&mut self, view: ViewTarget) {
        self.editor_view = view;
    }

    pub(super) fn get_game_view(&self) -> &ViewTarget {
        &self.game_view
    }

    pub(super) fn get_editor_view(&self) -> &ViewTarget {
        &self.editor_view
    }

    pub(super) fn is_editor_view_visible(&self) -> bool {
        self.editor_view.is_visible()
    }

    /// Whether the game view is the tab that was last clicked in.
    pub(super) fn is_game_view_active(&self) -> bool {
        self.active_tab == Some(EditorTab::GameView)
    }

    pub(super) fn is_editor_view_active(&self) -> bool {
        self.active_tab == Some(EditorTab::EditorView)
    }

    pub(super) fn editor_view_contains(&self, position: Vec2) -> bool {
        self.editor_view.contains(position)
    }

    pub(super) fn game_camera_target(&self) -> RenderTarget {
        self.game_view.image.clone().into()
    }

    pub(super) fn editor_camera_target(&self) -> RenderTarget {
        self.editor_view.image.clone().into()
    }

    pub(super) fn ui(&mut self, ui: &mut egui::Ui, world: &mut World) {
        // Only tabs that are visible this frame report a rect
        self.game_view.rect = None;
        self.editor_view.rect = None;

        let mut viewer = EditorTabViewer {
            world,
            game_view: &mut self.game_view,
            editor_view: &mut self.editor_view,
            selected_entities: &mut self.selected_entities,
            selected_resource: &mut self.selected_resource,
        };

        DockArea::new(&mut self.state)
            .style(Style::from_egui(&ui.global_style()))
            .show_inside(ui, &mut viewer);

        // Keep the last focused tab, so clicking empty space doesn't deactivate it
        if let Some((_, tab)) = self.state.find_active_focused() {
            self.active_tab = Some(tab.clone());
        }
    }
}
