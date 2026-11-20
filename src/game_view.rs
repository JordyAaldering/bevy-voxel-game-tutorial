use better_default::Default;
use bevy::prelude::*;

#[derive(Default, Resource)]
pub struct GameViewport {
    #[default(GameViewportMode::Window)]
    mode: GameViewportMode,
}

enum GameViewportMode {
    Window,
    Embedded { active: bool, rect: Option<Rect> },
}

impl GameViewport {
    pub fn set_embedded(&mut self, active: bool, rect: Option<Rect>) {
        self.mode = GameViewportMode::Embedded { active, rect };
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self.mode, GameViewportMode::Embedded { .. })
    }

    pub fn is_active(&self) -> bool {
        match self.mode {
            GameViewportMode::Window => true,
            GameViewportMode::Embedded { active, rect } => active && rect.is_some(),
        }
    }

    pub fn contains(&self, position: Vec2) -> bool {
        match self.mode {
            GameViewportMode::Window => true,
            GameViewportMode::Embedded { active, rect } => {
                active && rect.is_some_and(|rect| rect.contains(position))
            }
        }
    }

    pub fn map_position(&self, position: Vec2, viewport_size: Vec2) -> Option<Vec2> {
        match self.mode {
            GameViewportMode::Window => Some(position),
            GameViewportMode::Embedded { active: true, rect: Some(rect) } => {
                let size = rect.size();
                if !rect.contains(position) || size.x <= 0.0 || size.y <= 0.0 {
                    return None;
                }

                Some((position - rect.min) / size * viewport_size)
            }
            GameViewportMode::Embedded { .. } => None,
        }
    }
}
