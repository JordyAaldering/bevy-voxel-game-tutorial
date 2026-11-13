use bevy::{
    prelude::*,
    pbr::wireframe::{WireframeConfig, WireframePlugin},
};

#[derive(Default)]
pub struct DebugGizmosPlugin {
    /// Whether mesh wireframes are initially enabled.
    pub wireframe: bool,
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
    }
}
