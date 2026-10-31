mod direction;
mod screenshot;

use crate::{
    direction::Direction,
    screenshot::ScreenshotPlugin,
};

use better_default::Default;
use bevy::{
    prelude::*,
    asset::RenderAssetUsages,
    input::mouse::AccumulatedMouseMotion,
    mesh::{Indices, PrimitiveTopology},
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use bevy_inspector_egui::{
    prelude::*,
    bevy_egui::{
        prelude::*,
        input::EguiWantsInput,
    },
    quick::WorldInspectorPlugin,
};

#[derive(Default, Resource, Reflect, InspectorOptions)]
#[reflect(Resource, InspectorOptions)]
struct CameraSettings {
    #[default(5.0)]
    #[inspector(min = 0.1, max = 20.0)]
    move_speed: f32,
    #[default(0.0025)]
    #[inspector(min = 0.0001, max = 0.01)]
    mouse_sensitivity: f32,
}

struct Face {
    direction: Direction,
}

impl Face {
    /// Returns the four vertices of the quad.
    ///
    /// The quad is defined by the origin and direction of the face:
    /// ```text
    ///     (0,1,0)-------------------(1,1,0)
    ///          /|                  /|
    ///         / |                 / |
    ///        /  |                /  |
    /// (0,1,1)-------------(1,1,1)   |
    ///       |   |               |   |
    ///       |   |               |   |
    ///       |   |               |   |
    ///       |  (0,0,0)----------|--(1,0,0)
    ///       | /                 | /
    ///       |/                  |/
    /// (0,0,1)-------------------(1,0,1)
    /// ```
    ///
    /// Note that the z axis points forward, so the going back decreases the z coordinate.
    fn vertices(&self) -> [Vec3; 4] {
        let o = Vec3::ZERO;

        match self.direction {
            Direction::Right => [
                o + Vec3::X + Vec3::Z,
                o + Vec3::X,
                o + Vec3::X + Vec3::Y + Vec3::Z,
                o + Vec3::X + Vec3::Y,
            ],
            Direction::Left => [
                o,
                o + Vec3::Z,
                o + Vec3::Y,
                o + Vec3::Y + Vec3::Z,
            ],
            Direction::Top => [
                o + Vec3::Y + Vec3::Z,
                o + Vec3::X + Vec3::Y + Vec3::Z,
                o + Vec3::Y,
                o + Vec3::X + Vec3::Y,
            ],
            Direction::Bottom => [
                o,
                o + Vec3::X,
                o + Vec3::Z,
                o + Vec3::X + Vec3::Z,
            ],
            Direction::Front => [
                o + Vec3::Z,
                o + Vec3::X + Vec3::Z,
                o + Vec3::Y + Vec3::Z,
                o + Vec3::X + Vec3::Y + Vec3::Z,
            ],
            Direction::Back => [
                o + Vec3::X,
                o,
                o + Vec3::X + Vec3::Y,
                o + Vec3::Y,
            ],
        }
    }

    /// Returns the vertex indices for the quad (two triangles).
    ///
    /// Vertices are ordered as viewed from outside the voxel:
    /// ```text
    /// 2----3
    /// | \  |
    /// |  \ |
    /// 0----1
    /// ```
    fn indices(&self, start: u32) -> [u32; 6] {
        [
            // First triangle
            start + 0, // Bottom left
            start + 1, // Bottom right
            start + 2, // Top left
            // Second triangle
            start + 2, // Top left
            start + 1, // Bottom right
            start + 3, // Top right
        ]
    }

    /// Returns the normals of the 4 corners of the quad.
    fn normals(&self) -> [Vec3; 4] {
        [self.direction.vec3(); 4]
    }

    /// Returns the UV coordinates of the 4 corners of the quad.
    ///
    /// Note that the y-axis is flipped in UV coordinates, so the
    /// bottom left voxel actually has `y = 1` in UV coordinates.
    fn uvs(&self) -> [Vec2; 4] {
        [
            vec2(0.0, 1.0), // Bottom left
            vec2(1.0, 1.0), // Bottom right
            vec2(0.0, 0.0), // Top left
            vec2(1.0, 0.0), // Top right
        ]
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            EguiPlugin::default(),
            WorldInspectorPlugin::default(),
            ScreenshotPlugin::default(),
        ))
        .init_resource::<CameraSettings>()
        .register_type::<CameraSettings>()
        .add_systems(Startup, (
            spawn_global_illumination,
            spawn_player,
            spawn_voxel,
        ))
        .add_systems(Update, (
            toggle_cursor,
            (player_look, player_move)
                .run_if(is_focused),
        ))
        .run()
}

fn spawn_global_illumination(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            shadow_normal_bias: 1.0,
            shadow_depth_bias: 0.05,
            ..default()
        },
        Transform::from_rotation(
            Quat::from_euler(EulerRot::XYZ, 6.0, -7.5, 0.0)
        ),
    ));
}

fn spawn_player(mut commands: Commands) {
    commands.spawn((
        Name::new("Player"),
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 0.9,
            ..default()
        }),
        Msaa::Off,
        Transform::from_xyz(1.5, 2.0, 4.0)
            .looking_at(vec3(0.5, 0.5, 0.5), Vec3::Y),
    ));
}

fn spawn_voxel(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0),
        Mesh3d(meshes.add(build_mesh())),
        MeshMaterial3d(materials.add(Color::linear_rgb(0.2, 0.7, 0.3))),
    ));
}

fn build_mesh() -> Mesh {
    let mut vertices: Vec<Vec3> = Vec::with_capacity(6 * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(6 * 2 * 3);
    let mut normals: Vec<Vec3> = Vec::with_capacity(6 * 4);
    let mut uvs: Vec<Vec2> = Vec::with_capacity(6 * 4);

    for direction in Direction::iter() {
        let face = Face { direction };

        let start = vertices.len() as u32;
        vertices.extend(&face.vertices());
        indices.extend(&face.indices(start));
        normals.extend(&face.normals());
        uvs.extend(&face.uvs());
    }

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

fn is_focused(cursor: Single<&CursorOptions, With<PrimaryWindow>>) -> bool {
    cursor.grab_mode == CursorGrabMode::Locked
}

fn toggle_cursor(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    egui_wants_input: Res<EguiWantsInput>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    // Release the cursor when escape is pressed
    if keyboard.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
    // Recapture when a click occurs outside egui
    else if mouse.just_pressed(MouseButton::Left)
        && !egui_wants_input.wants_pointer_input()
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

fn player_look(
    settings: Res<CameraSettings>,
    mouse: Res<AccumulatedMouseMotion>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    let delta = mouse.delta;

    if delta != Vec2::ZERO {
        camera.rotate_y(-delta.x * settings.mouse_sensitivity);
        camera.rotate_local_x(-delta.y * settings.mouse_sensitivity);
    }
}

fn player_move(
    time: Res<Time>,
    settings: Res<CameraSettings>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    let delta = vec3(
        f32::from(keyboard.pressed(KeyCode::KeyD)) - f32::from(keyboard.pressed(KeyCode::KeyA)),
        f32::from(keyboard.pressed(KeyCode::Space)) - f32::from(keyboard.pressed(KeyCode::ShiftLeft)),
        f32::from(keyboard.pressed(KeyCode::KeyW)) - f32::from(keyboard.pressed(KeyCode::KeyS)),
    );

    if delta != Vec3::ZERO {
        let direction = delta.x * camera.right() + delta.y * Vec3::Y + delta.z * camera.forward();
        camera.translation += settings.move_speed * time.delta_secs() * direction.normalize();
    }
}
