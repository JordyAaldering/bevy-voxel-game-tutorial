use bevy::prelude::*;

pub(super) fn register(app: &mut App) {
    app.add_systems(Startup, spawn_player);
}

// =====================================
// Player entity components and markers.
// =====================================

#[cfg(since07)]
const PLAYER_EYE_HEIGHT: f32 = 1.6;

#[cfg(since07)]
#[derive(Clone, Default, Component)]
pub struct Player;

#[cfg(since07)]
#[derive(Clone, Default, Component)]
pub struct PlayerCamera;

// =======================================
// Systems for spawning the player entity.
// =======================================

#[cfg(until03)]
fn spawn_player(mut commands: Commands) {
    commands.spawn_scene(player(
        vec3(1.5, 2.0, 4.0)
    ));
}

#[cfg(all(since04, until06))]
fn spawn_player(mut commands: Commands) {
    commands.spawn_scene(player(
        vec3(5.0, 6.0, 11.0)
    ));
}

#[cfg(all(since07, until08))]
fn spawn_player(mut commands: Commands) {
    commands.spawn_scene(player(
        vec3(4.5, 5.0, 4.5),
        Quat::IDENTITY,
        Quat::IDENTITY,
    ));
}

#[cfg(all(since09, until13))]
fn spawn_player(
    mut commands: Commands,
    terrain: Res<crate::TerrainConfig>,
    #[cfg(since12)] editor_state: Res<crate::EditorState>,
) {
    let height = terrain.height.sample2(terrain.seed, 0.0, 0.0);

    commands.spawn_scene(player(
        vec3(0.5, height + 1.0, 0.5),
        Quat::IDENTITY,
        Quat::IDENTITY,
        #[cfg(since12)] editor_state.game_camera_target(),
    ));
}

#[cfg(since14)]
fn spawn_player(
    mut commands: Commands,
    #[cfg(until16)] terrain: Res<crate::TerrainConfig>,
    #[cfg(since17)] terrain: Res<crate::WorldConfig>,
    editor_state: Res<crate::EditorState>,
) {
    let save = crate::WorldSave::load();

    let position = save
        .as_ref()
        .map(|save| Vec3::from(save.player_position))
        .unwrap_or_else(|| {
            let height = {
                #[cfg(until16)] { terrain.height.sample2(terrain.seed, 0.0, 0.0) }
                #[cfg(since17)] { terrain.column(0, 0).height as f32 }
            };
            Vec3::new(1.5, height + 1.0, 1.5)
        });
    let rotation = save
        .as_ref()
        .map(|save| Quat::from_array(save.player_rotation))
        .unwrap_or(Quat::IDENTITY);
    let camera_rotation = save
        .as_ref()
        .map(|save| Quat::from_array(save.camera_rotation))
        .unwrap_or(Quat::IDENTITY);

    commands.spawn_scene(player(
        position,
        rotation,
        camera_rotation,
        editor_state.game_camera_target(),
    ));
}

// ================================================
// Bevy scene notation (bsn) for the player entity.
// ================================================

#[cfg(until06)]
fn player(
    position: Vec3,
) -> impl Scene {
    bsn!{
        #Player
        Camera3d
        Transform::from_translation(position)
            .looking_at(vec3(0.5, 0.5, 0.5), Vec3::Y)
        Projection::Perspective(PerspectiveProjection {
            fov: 0.9,
        })
        Msaa::Off
    }
}

#[cfg(all(since07, until11))]
fn player(
    position: Vec3,
    rotation: Quat,
    camera_rotation: Quat,
) -> impl Scene {
    bsn! {
        #Player
        Player
        Transform {
            translation: position,
            rotation,
        }
        Projection::Perspective(PerspectiveProjection {
            fov: 0.9
        })
        InheritedVisibility
        Children [
            PlayerCamera
            Camera3d
            Msaa::Off
            Transform {
                translation: Vec3::new(0.0, PLAYER_EYE_HEIGHT, 0.0),
                rotation: camera_rotation,
            }
        ]
    }
}

#[cfg(since12)]
fn player(
    position: Vec3,
    rotation: Quat,
    camera_rotation: Quat,
    camera_target: bevy::camera::RenderTarget,
) -> impl Scene {
    bsn! {
        #Player
        Player
        Transform {
            translation: position,
            rotation,
        }
        Projection::Perspective(PerspectiveProjection {
            fov: 0.9
        })
        InheritedVisibility
        Children [
            PlayerCamera
            Camera3d
            Msaa::Off
            Transform {
                translation: Vec3::new(0.0, PLAYER_EYE_HEIGHT, 0.0),
                rotation: camera_rotation,
            }
            template(move |_| Ok(camera_target.clone()))
        ]
    }
}
