use bevy::prelude::*;

pub(super) fn register(app: &mut App) {
    #[cfg(until08)]
    app.add_systems(Startup, player.spawn());
    #[cfg(since09)]
    app.add_systems(Startup, spawn_player);
}

#[cfg(since07)]
#[derive(Clone, Default, Component)]
pub struct Player {
    pub velocity: Vec3,
    pub grounded: bool,
}

#[cfg(since07)]
#[derive(Clone, Copy, Default, Component)]
pub struct PlayerCamera;

#[cfg(since07)]
impl Player {
    const EYE_HEIGHT: f32 = 1.6;
}

#[cfg(until06)]
fn player() -> impl Scene {
    let position = {
        #[cfg(until03)] { vec3(1.5, 2.0, 4.0) }
        #[cfg(since04)] { vec3(5.0, 6.0, 11.0) }
    };

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

#[cfg(all(since07, until08))]
fn player() -> impl Scene {
    bsn! {
        #Player
        Player
        Transform::from_xyz(4.5, 5.0, 4.5)
        Projection::Perspective(PerspectiveProjection {
            fov: 0.9
        })
        Msaa::Off
        InheritedVisibility
        Children [
            PlayerCamera
            Camera3d
            Transform::from_xyz(0.0, Player::EYE_HEIGHT, 0.0)
        ]
    }
}

#[cfg(all(since09, until13))]
fn spawn_player(
    mut commands: Commands,
    config: Res<crate::TerrainConfig>,
    #[cfg(since12)] editor_state: Res<crate::EditorState>,
) {
    let world_height = config.height.sample2(config.seed, 0.0, 0.0) + 1.0;

    commands.spawn((
            Name::new("Player"),
            Player::default(),
            Projection::Perspective(PerspectiveProjection {
                fov: 0.9,
                ..default()
            }),
            Msaa::Off,
            InheritedVisibility::default(),
            Transform::from_xyz(0.5, world_height, 0.5),
        ))
        .with_children(|commands| {
            commands.spawn((
                PlayerCamera,
                Camera3d::default(),
                Transform::from_xyz(0.0, Player::EYE_HEIGHT, 0.0),
                #[cfg(since12)]
                editor_state.game_camera_target(),
            ));
        });
}

#[cfg(since14)]
fn spawn_player(
    mut commands: Commands,
    config: Res<crate::TerrainConfig>,
    editor_state: Res<crate::EditorState>,
) {
    let world_height = config.height.sample2(config.seed, 0.0, 0.0) + 1.0;

    let save = crate::WorldSave::load();
    let position = save
        .as_ref()
        .map(|save| Vec3::from(save.player_position))
        .unwrap_or(Vec3::new(1.5, world_height, 1.5));
    let rotation = save
        .as_ref()
        .map(|save| Quat::from_array(save.player_rotation))
        .unwrap_or(Quat::IDENTITY);
    let camera_rotation = save
        .as_ref()
        .map(|save| Quat::from_array(save.camera_rotation))
        .unwrap_or(Quat::IDENTITY);
    let velocity = save
        .map(|save| Vec3::from(save.player_velocity))
        .unwrap_or(Vec3::ZERO);

    commands.spawn_scene(player(
        position,
        rotation,
        camera_rotation,
        velocity,
        editor_state.game_camera_target(),
    ));
}

#[cfg(since14)]
fn player(
    position: Vec3,
    rotation: Quat,
    camera_rotation: Quat,
    velocity: Vec3,
    camera_target: bevy::camera::RenderTarget,
) -> impl Scene {
    bsn! {
        #Player
        Player { velocity }
        Projection::Perspective(PerspectiveProjection {
            fov: 0.9
        })
        Msaa::Off
        InheritedVisibility
        Transform {
            translation: position,
            rotation,
        }
        Children [
            PlayerCamera
            Camera3d
            Transform {
                translation: Vec3::new(0.0, Player::EYE_HEIGHT, 0.0),
                rotation: camera_rotation,
            }
            template(move |_| Ok(camera_target.clone()))
        ]
    }
}
