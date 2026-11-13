use bevy::{
    prelude::*,
    pbr::wireframe::{WireframeConfig, WireframePlugin},
};
#[cfg(since12)]
use bevy_inspector_egui::prelude::*;

#[derive(Default)]
pub struct DebugGizmosPlugin {
    /// Whether mesh wireframes are initially enabled.
    pub wireframe: bool,
}

#[cfg(since12)]
#[derive(Default, Resource, Reflect, InspectorOptions)]
#[reflect(Resource, InspectorOptions)]
struct DebugGizmos {
    #[inspector()]
    chunk_borders: bool,
}

impl Plugin for DebugGizmosPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
                WireframePlugin::default(),
            ))
            .insert_resource(WireframeConfig {
                global: self.wireframe,
                ..default()
            });

        #[cfg(since12)]
        app.init_resource::<DebugGizmos>()
            .register_type::<DebugGizmos>();
    }
}
