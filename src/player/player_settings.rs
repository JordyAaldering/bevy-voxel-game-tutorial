use better_default::Default;
use bevy::prelude::*;
use bevy_inspector_egui::prelude::*;

pub(super) fn register(app: &mut App) {
    app.register_type::<CameraSettings>();

    #[cfg(until13)]
    app.init_resource::<CameraSettings>();
    #[cfg(since14)]
    app.insert_resource(CameraSettings::load());

    #[cfg(only02)]
    app.add_plugins(bevy_inspector_egui::quick::ResourceInspectorPlugin::<CameraSettings>::default());

    #[cfg(since14)]
    app.add_systems(Last, (
        save_on_exit.in_set(bevy::app::OnAppExitSystems),
    ));
}

#[derive(Default, Resource, Reflect, InspectorOptions)]
#[derive(serde::Serialize, serde::Deserialize)]
#[reflect(Resource, InspectorOptions)]
#[serde(default)]
pub(super) struct CameraSettings {
    #[default(0.0025)]
    #[inspector(min = 0.0001, max = 0.01)]
    pub mouse_sensitivity: f32,
    #[default(5.0)]
    #[inspector(min = 0.1, max = 20.0)]
    pub move_speed: f32,
    #[cfg(since07)]
    /// TODO: jump strength belongs to the physics plugin, not the player settings.
    /// Move it there. The player is just responsible for setting a flag that it wants to jump,
    /// and the physics plugin should handle the actual jump logic.
    #[default(7.0)]
    #[inspector(min = 0.1, max = 20.0)]
    pub jump_strenght: f32,
}

#[cfg(since14)]
impl CameraSettings {
    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/savedata/settings");

    fn path() -> std::path::PathBuf {
        std::path::Path::new(Self::DIR).join("camera.json")
    }

    fn save(&self) {
        let result = serde_json::to_string_pretty(self)
            .map_err(|e| e.to_string())
            .and_then(|json| crate::write_atomic(&Self::path(), json.as_bytes()).map_err(|e| e.to_string()));
        if let Err(error) = result {
            error!("Failed to save camera settings: {error}");
        }
    }

    /// Falls back to the defaults if the file is missing or invalid.
    fn load() -> Self {
        use std::{fs, io};

        let json = match fs::read_to_string(Self::path()) {
            Ok(json) => json,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                error!("Failed to read camera.json: {error}");
                return Self::default();
            }
        };

        serde_json::from_str(&json).unwrap_or_else(|error| {
            error!("Failed to parse camera.json: {error}");
            Self::default()
        })
    }
}

#[cfg(since14)]
fn save_on_exit(
    mut exits: MessageReader<AppExit>,
    settings: Res<CameraSettings>,
) {
    if exits.read().next().is_none() {
        return;
    }

    settings.save();
}
