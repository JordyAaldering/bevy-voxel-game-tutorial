use crate::PlayerCamera;

use better_default::Default;
use bevy::{
    prelude::*,
    window::PrimaryWindow,
};

#[derive(Default, Resource)]
pub struct GameViewport {
    #[default(GameViewportMode::Window)]
    mode: GameViewportMode,
}

enum GameViewportMode {
    Window,
    Embedded { active: bool, rect: Option<Rect> },
}

/// The `GameCursor` resource tracks the cursor position in the game view, which may
/// be embedded in the editor or in a separate window. The position is updated at the
/// start of each frame based on the current cursor position and the viewport mode.
#[derive(Default, Resource)]
pub struct GameCursor {
    pub position: Option<Vec2>,
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

impl GameCursor {
    pub fn update(
        mut cursor: ResMut<Self>,
        viewport: Res<GameViewport>,
        window: Single<&Window, With<PrimaryWindow>>,
        camera: Single<&Camera, With<PlayerCamera>>,
    ) {
        cursor.position = window.cursor_position().and_then(|position| {
            if viewport.is_embedded() {
                camera.logical_viewport_size()
                    .and_then(|size| viewport.map_position(position, size))
            } else {
                viewport.map_position(position, Vec2::ZERO)
            }
        });
    }
}
