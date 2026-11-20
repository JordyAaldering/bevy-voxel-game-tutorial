mod player_entity;
mod player_input;

#[allow(unused_imports)]
pub use player_entity::*;

use crate::GameViewport;

use bevy::prelude::*;

#[derive(Default)]
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameViewport>();
        player_entity::register(app);
        player_input::register(app);
    }
}
