mod player_entity;
mod player_input;

#[allow(unused_imports)]
pub use player_entity::*;

use crate::{GameCursor, GameViewport};

use bevy::{
    prelude::*,
    picking::PickingSystems,
};

#[derive(Default)]
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameViewport>()
            .init_resource::<GameCursor>()
            .add_systems(First, (
                GameCursor::update.before(PickingSystems::PostInput),
            ));
        player_entity::register(app);
        player_input::register(app);
    }
}
