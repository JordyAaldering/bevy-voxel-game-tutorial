pub use input_settings::InputSettings;

use bevy::prelude::*;

#[derive(Default)]
pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<InputSettings>();

        app.insert_resource(
            #[cfg(until13)] InputSettings::default(),
            #[cfg(since14)] InputSettings::load(),
        );

        #[cfg(since14)]
        app.add_systems(Last, (
            save_on_exit.in_set(bevy::app::OnAppExitSystems),
        ));
    }
}

#[cfg(since14)]
pub fn save_on_exit(
    mut exits: MessageReader<AppExit>,
    input_settings: Res<InputSettings>,
) {
    if exits.read().next().is_none() {
        return;
    }

    input_settings.save();
}

mod input_settings {
    use better_default::Default;
    use bevy::prelude::*;
    use bevy_inspector_egui::prelude::*;

    #[derive(Default, Resource, Reflect, InspectorOptions)]
    #[derive(serde::Serialize, serde::Deserialize)]
    #[reflect(Resource, InspectorOptions)]
    #[serde(default)]
    pub struct InputSettings {
        #[default(0.0025)]
        #[inspector(min = 0.0001, max = 0.01)]
        pub mouse_sensitivity: f32,
    }

    #[cfg(since14)]
    impl InputSettings {
        const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/savedata/settings");

        fn path() -> std::path::PathBuf {
            std::path::Path::new(Self::DIR).join("camera.json")
        }

        pub(super)fn save(&self) {
            let result = serde_json::to_string_pretty(self)
                .map_err(|e| e.to_string())
                .and_then(|json| crate::write_atomic(&Self::path(), json.as_bytes()).map_err(|e| e.to_string()));
            if let Err(error) = result {
                error!("Failed to save camera settings: {error}");
            }
        }

        /// Falls back to the defaults if the file is missing or invalid.
        pub(super) fn load() -> Self {
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
}
