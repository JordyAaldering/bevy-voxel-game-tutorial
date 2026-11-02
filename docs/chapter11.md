# Performance debugging

At this point, I would like to see some performance statistics.
What is my framerate? How many chunks are currently loaded? What is my memory usage? etc.
However, I don't want to put all of this in the game view.

![](https://external-content.duckduckgo.com/iu/?u=https%3A%2F%2Fgamepedia.cursecdn.com%2Fminecraft_gamepedia%2F1%2F18%2FDebugScreen.png&f=1&nofb=1&ipt=c6355da97e9a77c3a9d119ca871bf3f97922691cd487a843e0e9c61cad8e323d&ipo=images)

I find this very hard to read.
Rather, some plots and interactivity would be much nicer.

What we can do is to a browser-based debug dashboard.
- Bevy exposes selected metrics and debug settings to a WebSocket.
- A browser dashboard displays live graphs, counters, etc.

https://github.com/tokio-rs/axum/blob/main/examples/websockets/src/main.rs
https://docs.rs/axum/latest/axum/extract/ws/

```bash
cargo add axum --features ws
cargo add tokio --features rt-multi-thread,macros,net,sync,time
cargo add async-compat
cargo add serde --features derive
cargo add serde_json
```

- `axum` is a web framework for Rust, which we can use to serve a dashboard.
- `tokio` is an async runtime for Rust, which axum is built on.
- `async-compat` lets us run tokio-based futures on Bevy's own async task pools, so we don't need to manage a separate thread or runtime ourselves.
- `serde` and `serde_json` are used to serialize the metrics data to JSON, which can be easily consumed by the browser dashboard.

We wrap the server in a `TelemetryPlugin`, just like we did for the screenshot and physics plugins.
The plugin spawns the server as an async task on Bevy's `IoTaskPool`, so it runs outside the game loop and never blocks it.
Axum expects a tokio reactor, which Bevy's task pools don't provide, so we wrap the future in `Compat`.
The plugin also adds a system that publishes a snapshot of the diagnostics every 250 milliseconds, through a `tokio::sync::watch` channel.

```rust
impl Plugin for TelemetryPlugin {
    fn build(&self, app: &mut App) {
        let (sender, receiver) = watch::channel(Telemetry::default());

        IoTaskPool::get()
            .spawn(Compat::new(run_server(receiver)))
            .detach();

        app.insert_resource(TelemetryPublisher(sender))
            .add_systems(Update, collect_telemetry
                .run_if(on_timer(Duration::from_millis(250))));
    }
}
```

Adding it to the game is then a single line: `TelemetryPlugin`.

You can really take this quite far, and implement a full-featured dashboard with multiple pages, graphs, etc.
I'm no web developer, so I'll keep it simple, but feel free to add more features to the dashboard at this point!

You could even go a step further and make the game configurable from the dashboard, for example for showing gizmos.
However, I prefer to keep the dashboard read-only, and instead use the Bevy `bevy-inspector-egui` plugin for that.

## Feature gate

If you ever share your game, you probably don't want to ship the debug dashboard with it.
We can gate the debug dashboard, and the necessary dependencies, behind a feature flag.

https://doc.rust-lang.org/cargo/reference/features.html

I omit it for now, because I want to avoid recompilations when enabling/disabling certain flags.

## Toggle-able gizmos

I'll use the `bevy-inspector-egui` again to add a toggle for showing gizmos, just like we did for the camera settings.
To avoid a blowup of the number of egui windows, I'll group all debug settings into a single window: `DebugSettingsView`.

This is a bit of a pain, because I still want CameraSettings and DebugGizmos to be separate resources: the player_look function should not have access to DebugGizmos; that's just poor design.
However, I also don't want to have two (or more) separate egui windows for the debug settings, because that would be a mess.
We could manually create a window, but that would also mean manually implementing the sliders etc.
Luckily, we can let the window of each resource be automatically drawn.
Then, we can use a similar approach as we did for the chunks window.

```rust
fn debug_settings_ui(world: &mut World) {
    let Ok(egui_context) = world
        .query_filtered::<&mut EguiContext, With<PrimaryEguiContext>>()
        .single(world)
    else {
        return;
    };

    let mut ctx = egui_context.clone();

    egui::Window::new("Settings").show(ctx.get_mut(), |ui| {
        ui.heading("Camera Settings");
        ui.push_id("camera_settings", |ui| {
            bevy_inspector::ui_for_resource::<CameraSettings>(world, ui);
        });

        ui.separator();

        ui.heading("Debug Gizmos");
        ui.push_id("debug_gizmos", |ui| {
            bevy_inspector::ui_for_resource::<DebugGizmos>(world, ui);
        });
    });
}
```
