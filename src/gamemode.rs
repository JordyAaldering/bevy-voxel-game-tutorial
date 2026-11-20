use better_default::Default;
use bevy::prelude::*;

#[derive(Default)]
pub struct GameModePlugin {
    #[default(GameMode::Survival)]
    starting_gamemode: GameMode,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Resource)]
pub enum GameMode {
    Survival,
    Spectator,
}

impl Plugin for GameModePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.starting_gamemode)
            .add_systems(Update, (
                toggle_gamemode.run_if(crate::is_focused),
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
    mut mode: ResMut<GameMode>,
) {
    if keyboard.just_pressed(KeyCode::KeyF) {
        *mode = match *mode {
            GameMode::Survival => GameMode::Spectator,
            GameMode::Spectator => GameMode::Survival,
        };
    }
}
