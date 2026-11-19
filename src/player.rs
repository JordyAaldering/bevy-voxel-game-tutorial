mod player_entity;
mod player_input;

#[allow(unused_imports)]
pub use player_entity::*;

use bevy::prelude::*;

#[derive(Default)]
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        player_entity::register(app);
        player_input::register(app);
    }
}
