use std::{
    fs::{self, File}, io::BufWriter, path::PathBuf, time::{Duration, SystemTime, UNIX_EPOCH},
};

use better_default::Default;
use bevy::{
    prelude::*, render::view::screenshot::{Screenshot, save_to_disk}, time::common_conditions::on_timer,
};
use gif::{Encoder, Frame, Repeat};

const GIF_FPS: f32 = 10.0;

#[derive(Default)]
pub struct ScreenshotPlugin {
    #[default("docs/img".into())]
    pub screenshot_dir: PathBuf,
    #[default("docs/img/tmp".into())]
    pub gif_tmp_dir: PathBuf,
    #[default("docs/img".into())]
    pub gif_out_dir: PathBuf,
}

#[derive(Resource)]
struct ScreenshotConfig {
    screenshot_dir: PathBuf,
    gif_tmp_dir: PathBuf,
    gif_out_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, States)]
enum GifRecordingState {
    #[default]
    Available,
    Recording,
    Processing,
}

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ScreenshotConfig {
                screenshot_dir: self.screenshot_dir.clone(),
                gif_tmp_dir: self.gif_tmp_dir.clone(),
                gif_out_dir: self.gif_out_dir.clone(),
            })
            .add_systems(Update, (
                take_screenshot,
                start_gif_recording,
                capture_gif_frame
                    .run_if(on_timer(Duration::from_secs_f32(1.0 / GIF_FPS)))
                    .run_if(in_state(GifRecordingState::Recording)),
            ))
            .add_systems(OnExit(GifRecordingState::Recording),
                process_gif,
            );
    }
}

/// Captures the whole window, including egui and the dock, when F12 is pressed.
fn take_screenshot(
    keys: Res<ButtonInput<KeyCode>>,
    config: Res<ScreenshotConfig>,
    mut commands: Commands,
) -> Result<(), BevyError> {
    if !keys.just_pressed(KeyCode::F12) {
        return Ok(());
    }

    fs::create_dir_all(&config.screenshot_dir)?;

    let secs = get_system_seconds();
    let path = config.screenshot_dir.join(format!("screenshot_{secs}.png"));
    info!("Saving screenshot to {}", path.display());

    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));

    Ok(())
}

fn start_gif_recording(
    config: Res<ScreenshotConfig>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GifRecordingState>>,
    mut next_state: ResMut<NextState<GifRecordingState>>,
) -> Result<(), BevyError> {
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if !(shift && keys.just_pressed(KeyCode::F12)) {
        return Ok(());
    }

    next_state.set(match state.get() {
        GifRecordingState::Available => {
            fs::create_dir_all(&config.gif_tmp_dir)
                .context(format!("Failed to create {}", config.gif_tmp_dir.display()))?;
            GifRecordingState::Recording
        },
        GifRecordingState::Recording => {
            fs::create_dir_all(&config.gif_out_dir)
                .context(format!("Failed to create {}", config.gif_out_dir.display()))?;
            GifRecordingState::Processing
        }
        GifRecordingState::Processing => GifRecordingState::Processing,
    });

    Ok(())
}

fn capture_gif_frame(
    config: Res<ScreenshotConfig>,
    mut commands: Commands,
    mut counter: Local<u32>,
) -> Result<(), BevyError> {
    let path = config.screenshot_dir.join(format!("frame_{:.06}.png", *counter));
    info!("Saving frame to {}", path.display());

    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));

    *counter += 1;
    Ok(())
}

fn process_gif(
    config: Res<ScreenshotConfig>,
    mut next_state: ResMut<NextState<GifRecordingState>>,
) -> Result<(), BevyError> {
    let mut frames = Vec::new();
    for file in fs::read_dir(&config.gif_tmp_dir)?
    {
        frames.push(file?.path())
    }

    frames.sort();

    let first = frames.first()
        .context(format!("No frames found in {}", config.gif_tmp_dir.display()))?;
    let first_image = image::open(first)?;

    let width = u16::try_from(first_image.width())?;
    let height = u16::try_from(first_image.height())?;

    let secs = get_system_seconds();
    let path = config.gif_out_dir.join(format!("output_{secs}.gif"));
    let file = File::create(&path)?;

    let mut encoder = Encoder::new(
        BufWriter::new(file),
        width,
        height,
        &[],
    )?;

    encoder.set_repeat(Repeat::Infinite)?;

    let delay = (100.0 / GIF_FPS).round() as u16;

    for path in frames {
        let mut image = image::open(&path)?
            .to_rgba8();

        if image.dimensions() != (width as u32, height as u32) {
            return Err(BevyError::error(
                format!("Frame {} has unexpected dimensions", path.display())
            ));
        }

        let mut frame = Frame::from_rgba_speed(width, height, image.as_mut(), 10);
        frame.delay = delay;
        encoder.write_frame(&frame)?;
    }

    next_state.set(GifRecordingState::Available);
    Ok(())
}

fn get_system_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
