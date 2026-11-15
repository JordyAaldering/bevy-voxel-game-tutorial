mod player_entity;
mod player_input;
#[cfg(since02)]
mod player_settings;

#[allow(unused_imports)]
pub use player_entity::*;

use bevy::prelude::*;

#[derive(Default)]
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        player_entity::register(app);
        player_input::register(app);
        #[cfg(since02)]
        player_settings::register(app);
    }
}
