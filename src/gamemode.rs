use crate::{Player, is_focused};

use bevy::prelude::*;

#[derive(Default)]
pub struct GameModePlugin;

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Resource)]
pub enum GameMode {
    #[default]
    Survival,
    Spectator,
}

impl Plugin for GameModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameMode>()
            .add_systems(Update, (
                toggle_gamemode.run_if(is_focused),
            ));
    }
}

pub fn is_gamemode_survival(mode: Res<GameMode>) -> bool {
    *mode == GameMode::Survival
}

pub fn is_gamemode_spectator(mode: Res<GameMode>) -> bool {
    *mode == GameMode::Spectator
}

fn toggle_gamemode(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut player: Single<&mut Player>,
    mut mode: ResMut<GameMode>,
) {
    if keyboard.just_pressed(KeyCode::KeyF) {
        *mode = match *mode {
            GameMode::Survival => GameMode::Spectator,
            GameMode::Spectator => GameMode::Survival,
        };
        // Don't retain old vertical momentum
        player.velocity = Vec3::ZERO;
    }
}
