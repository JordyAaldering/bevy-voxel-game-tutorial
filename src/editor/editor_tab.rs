use super::ViewTarget;

use std::any::TypeId;

use bevy::prelude::*;
use bevy_inspector_egui::{
    bevy_inspector::{
        self,
        hierarchy::{SelectedEntities, hierarchy_ui},
        ui_for_entities_shared_components,
        ui_for_entity_with_children,
    },
    egui,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum EditorTab {
    GameView,
    EditorView,
    Hierarchy,
    Resources,
    Inspector,
}

pub(super) struct EditorTabViewer<'w> {
    pub world: &'w mut World,
    pub game_view: &'w mut ViewTarget,
    pub editor_view: &'w mut ViewTarget,
    pub selected_entities: &'w mut SelectedEntities,
    pub selected_resource: &'w mut Option<(TypeId, String)>,
}

impl egui_dock::TabViewer for EditorTabViewer<'_> {
    type Tab = EditorTab;

    fn id(&mut self, tab: &mut Self::Tab) -> egui::Id {
        egui::Id::new(format!("{tab:?}"))
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            EditorTab::GameView   => "Game".into(),
            EditorTab::EditorView => "Editor".into(),
            EditorTab::Hierarchy  => "Hierarchy".into(),
            EditorTab::Resources  => "Resources".into(),
            EditorTab::Inspector  => "Inspector".into(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            EditorTab::GameView => self.game_view.ui(ui),
            EditorTab::EditorView => self.editor_view.ui(ui),
            EditorTab::Hierarchy => {
                if hierarchy_ui(self.world, ui, self.selected_entities) {
                    *self.selected_resource = None;
                }
            }
            EditorTab::Resources => {
                let registry = self
                    .world
                    .resource::<AppTypeRegistry>()
                    .0
                    .clone();
                let registry = registry.read();

                let mut resources: Vec<_> = registry
                    .iter()
                    .filter(|registration| {
                        registration.data::<ReflectResource>().is_some()
                    })
                    .map(|registration| {
                        (
                            registration
                                .type_info()
                                .type_path_table()
                                .short_path()
                                .to_owned(),
                            registration.type_id(),
                        )
                    })
                    .collect();

                resources.sort_by(|a, b| a.0.cmp(&b.0));

                for (name, type_id) in resources {
                    let selected = self
                        .selected_resource
                        .as_ref()
                        .is_some_and(|(selected_id, _)| {
                            *selected_id == type_id
                        });

                    if ui.selectable_label(selected, &name).clicked() {
                        *self.selected_resource = Some((type_id, name));
                        self.selected_entities.clear();
                    }
                }
            }
            EditorTab::Inspector => {
                let Some((type_id, name)) =
                    self.selected_resource.as_ref()
                else {
                    match self.selected_entities.as_slice() {
                        &[entity] => ui_for_entity_with_children(self.world, entity, ui),
                        &[] => { ui.label("Select an entity or resource to inspect."); }
                        entities => ui_for_entities_shared_components(self.world, entities, ui),
                    }
                    return;
                };

                ui.heading(name);

                let registry = self
                    .world
                    .resource::<AppTypeRegistry>()
                    .0
                    .clone();
                let registry = registry.read();

                bevy_inspector::by_type_id::ui_for_resource(
                    self.world,
                    *type_id,
                    ui,
                    name,
                    &registry,
                );
            }
        }
    }
}
