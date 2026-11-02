use std::{
    fs::{self, File},
    io::BufWriter,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use better_default::Default;
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    time::common_conditions::on_timer,
};
use gif::{Encoder, Frame, Repeat};

#[derive(Default)]
pub struct ScreenshotPlugin {
    #[default("docs/img".into())]
    pub out_dir: PathBuf,
    #[default(15.0)]
    pub gif_fps: f32,
}

#[derive(Resource)]
struct ScreenshotConfig {
    out_dir: PathBuf,
    gif_fps: f32,
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
                out_dir: self.out_dir.clone(),
                gif_fps: self.gif_fps,
            })
            .init_state::<GifRecordingState>()
            .add_systems(Update, (
                take_screenshot,
                start_gif_recording,
                capture_gif_frame
                    .run_if(on_timer(Duration::from_secs_f32(1.0 / self.gif_fps)))
                    .run_if(in_state(GifRecordingState::Recording)),
            ))
            .add_systems(OnExit(GifRecordingState::Recording),
                process_gif,
            );
    }
}

/// Captures the whole window, including egui and the dock.
fn take_screenshot(
    keys: Res<ButtonInput<KeyCode>>,
    config: Res<ScreenshotConfig>,
    mut commands: Commands,
) -> Result<(), BevyError> {
    if !keys.just_pressed(KeyCode::F12) {
        return Ok(());
    }

    fs::create_dir_all(&config.out_dir)?;

    let secs = get_system_seconds();
    let path = config.out_dir.join(format!("screenshot_{secs}.png"));

    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));

    Ok(())
}

/// Start or stop GIF recording.
fn start_gif_recording(
    config: Res<ScreenshotConfig>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<GifRecordingState>>,
    mut next_state: ResMut<NextState<GifRecordingState>>,
) -> Result<(), BevyError> {
    if !keys.just_pressed(KeyCode::F11) {
        return Ok(());
    }

    next_state.set(match state.get() {
        GifRecordingState::Available => {
            let tmp_dir = config.out_dir.join("tmp");
            fs::create_dir_all(&tmp_dir)
                .context(format!("Failed to create {}", tmp_dir.display()))?;
            GifRecordingState::Recording
        },
        GifRecordingState::Recording => {
            fs::create_dir_all(&config.out_dir)
                .context(format!("Failed to create {}", config.out_dir.display()))?;
            GifRecordingState::Processing
        }
        GifRecordingState::Processing => GifRecordingState::Processing,
    });

    Ok(())
}

/// When in the recording state, capture a frame and save it to a temporary directory.
fn capture_gif_frame(
    config: Res<ScreenshotConfig>,
    mut commands: Commands,
    mut counter: Local<u32>,
) -> Result<(), BevyError> {
    let tmp_dir = config.out_dir.join("tmp");
    let path = tmp_dir.join(format!("frame_{:06}.png", *counter));

    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));

    *counter += 1;
    Ok(())
}

/// When entering the processing state, read all the frames from the temporary directory and encode them into a GIF.
/// Because this runs in the main thread, it will block the game for a few seconds while the GIF is being encoded.
fn process_gif(
    config: Res<ScreenshotConfig>,
    mut next_state: ResMut<NextState<GifRecordingState>>,
) -> Result<(), BevyError> {
    let mut frames = Vec::new();
    let tmp_dir = config.out_dir.join("tmp");
    for file in fs::read_dir(&tmp_dir)? {
        frames.push(file?.path())
    }

    frames.sort();

    let first = frames.first()
        .context(format!("No frames found in {}", tmp_dir.display()))?;
    let first_image = image::open(first)?;

    let width = u16::try_from(first_image.width())?;
    let height = u16::try_from(first_image.height())?;

    let secs = get_system_seconds();
    let path = config.out_dir.join(format!("output_{secs}.gif"));
    let file = File::create(&path)?;

    let delay = (100.0 / config.gif_fps).round() as u16;
    let mut encoder = Encoder::new(
        BufWriter::new(file),
        width,
        height,
        &[],
    )?;
    encoder.set_repeat(Repeat::Infinite)?;

    for path in frames {
        let mut image = image::open(&path)?
            .to_rgba8();

        if image.dimensions() != (width as u32, height as u32) {
            return Err(BevyError::error(
                format!("Frame {} has unexpected dimensions", path.display())
            ));
        }

        let mut frame = Frame::from_rgba_speed(
            width,
            height,
            image.as_mut(),
            10,
        );
        frame.delay = delay;
        encoder.write_frame(&frame)?;
    }

    // Remove the temporary directory
    fs::remove_dir_all(&tmp_dir)
        .context(format!("Failed to remove {}", tmp_dir.display()))?;

    next_state.set(GifRecordingState::Available);
    Ok(())
}

fn get_system_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
