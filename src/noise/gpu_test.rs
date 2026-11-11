use super::*;

use bevy::{
    app::PluginsState,
    render::RenderPlugin,
    window::WindowPlugin,
    winit::WinitPlugin,
};

#[test]
fn noise_cpu_matches_gpu() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                // Avoid leaving pipeline compilation running as the test process exits.
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .disable::<WinitPlugin>(),
        GpuPlugin {
            mode: Mode::Auto,
        },
    ));

    while app.plugins_state() == PluginsState::Adding {
        std::thread::sleep(Duration::from_millis(10));
    }

    app.finish();
    app.cleanup();
    app.update();

    assert!(
        app.world().resource::<NoiseBackend>().gpu.is_some(),
        "GPU noise unavailable; run with a compatible adapter and BEVY_VOXEL_NOISE_BACKEND=auto or gpu"
    );

    let mut requests = Vec::new();
    for noise_type in [NoiseType::Voronoi, NoiseType::OpenSimplex2S] {
        for (octaves, seed, (x, z)) in [
            (0, 0, (0, 0)),
            (1, -123456789, (-2, 1)),
            (3, i32::MAX, (3, -4)),
            (1, i32::MIN, (-7, -6)),
        ] {
            let settings = NoiseSettings {
                noise_type,
                octaves,
                frequency: 0.13,
                offset: Vec2::new(-3.2, 1.7),
                persistence: 0.6,
                lacunarity: 1.9,
                amplitude: 1.7,
                bias: -0.3,
            };

            let task = app.world_mut()
                .resource_mut::<NoiseBackend>()
                .area3(settings, seed, x, z);
            requests.push((settings, seed, (x, z), task));
        }
    }

    let start = Instant::now();
    while requests.iter()
        .any(|(_, _, _, task)| task.receiver.is_empty())
    {
        app.update();
        assert!(start.elapsed() < TIMEOUT, "noise batches timed out");
    }

    assert!(
        app.world().resource::<NoiseBackend>().gpu.is_some(),
        "GPU noise failed or fell back to the CPU"
    );

    for (settings, seed, (x, z), task) in requests {
        let actual = task.receiver.try_recv().unwrap().unwrap();
        let expected = settings.area3(seed, x, z);

        assert_eq!(actual.len(), expected.len());
        assert!(actual.iter().all(|value| value.is_finite()));

        let max_error = actual.iter().zip(expected).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);

        assert!(
            max_error < 0.0002,
            "{:?}, {} octaves: GPU noise differs by {max_error}",
            settings.noise_type, settings.octaves
        );
    }
}
