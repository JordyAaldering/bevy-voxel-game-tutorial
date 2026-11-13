#[allow(unused_imports)]
use crate::*;

use std::{net::SocketAddr, time::Duration};

use async_compat::Compat;
use axum::{
    Router,
    extract::{State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::{Html, Response},
    routing,
};
use bevy::{
    prelude::*,
    tasks::IoTaskPool,
    time::common_conditions::on_timer,
    diagnostic::{DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin},
};
use tokio::sync::watch;

/// Serves the telemetry dashboard from an async task on Bevy's IO task pool.
pub struct TelemetryPlugin;

// Resource used by Bevy to publish the latest snapshot.
#[derive(Resource)]
struct TelemetryPublisher(watch::Sender<Telemetry>);

#[derive(Clone, Debug, Default, Resource)]
#[derive(serde::Serialize)]
struct ChunkTelemetry {
    height_generation: usize,
    ore_generation: usize,
    cave_generation: usize,
    generate_structures: usize,
    mesh_generation: usize,
    in_progress: usize,
    done: usize,
    unloaded: usize,
}

#[derive(Clone, Debug, Default)]
#[derive(serde::Serialize)]
struct Telemetry {
    frame_rate: f64,
    frame_time: f64,
    entity_count: f64,
    chunks: ChunkTelemetry,
}

impl Plugin for TelemetryPlugin {
    fn build(&self, app: &mut App) {
        let (sender, receiver) = watch::channel(Telemetry::default());

        // Axum requires a Tokio reactor, which `Compat` provides to the future.
        IoTaskPool::get()
            .spawn(Compat::new(run_server(receiver)))
            .detach();

        app.insert_resource(TelemetryPublisher(sender))
            .init_resource::<ChunkTelemetry>()
            .add_systems(Update,
                (
                    collect_telemetry,
                    #[cfg(since20)] collect_chunk_telemetry,
                )
                .chain()
                .run_if(on_timer(Duration::from_millis(250)))
            );
    }
}

async fn run_server(receiver: watch::Receiver<Telemetry>) {
    let app = Router::new()
        .route("/", routing::get(index))
        .route("/ws", routing::get(websocket))
        .with_state(receiver);

    let address = SocketAddr::from(([127, 0, 0, 1], 3000));

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .unwrap();

    info!("Telemetry dashboard: http://{address}");

    axum::serve(listener, app)
        .await
        .unwrap();
}

#[cfg(since20)]
fn collect_chunk_telemetry(
    world: Res<VoxelWorld>,
    tasks: Query<&ChunkGenerationTask>,
    mut telemetry: ResMut<ChunkTelemetry>,
) {
    let mut snapshot = ChunkTelemetry::default();
    for chunk in world.chunks.values() {
        if chunk.entity.is_none() {
            snapshot.unloaded += 1;
            continue;
        }

        let count = match chunk.generation_phase {
            ChunkGenerationPhase::HeightGeneration => &mut snapshot.height_generation,
            ChunkGenerationPhase::OreGeneration => &mut snapshot.ore_generation,
            ChunkGenerationPhase::CaveGeneration => &mut snapshot.cave_generation,
            ChunkGenerationPhase::GenerateStructures => &mut snapshot.generate_structures,
            ChunkGenerationPhase::MeshGeneration => &mut snapshot.mesh_generation,
            ChunkGenerationPhase::Done
                if chunk.entity.is_some_and(|entity| tasks.contains(entity)) =>
            {
                &mut snapshot.mesh_generation
            }
            ChunkGenerationPhase::Done => {
                snapshot.done += 1;
                continue;
            }
        };

        *count += 1;
        snapshot.in_progress += 1;
    }

    *telemetry = snapshot;
}

fn collect_telemetry(
    diagnostics: Res<DiagnosticsStore>,
    publisher: Res<TelemetryPublisher>,
    chunks: Res<ChunkTelemetry>,
) {
    let value = |path| {
        diagnostics.get(path)
            .and_then(|d| d.smoothed())
            .unwrap_or_default()
    };

    let snapshot = Telemetry {
        frame_rate: value(&FrameTimeDiagnosticsPlugin::FPS),
        frame_time: value(&FrameTimeDiagnosticsPlugin::FRAME_TIME),
        entity_count: value(&EntityCountDiagnosticsPlugin::ENTITY_COUNT),
        chunks: chunks.clone(),
    };

    publisher.0.send_replace(snapshot);
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../ws/telemetry.html"))
}

async fn websocket(upgrade: WebSocketUpgrade, receiver: State<watch::Receiver<Telemetry>>) -> Response {
    upgrade.on_upgrade(move |socket| stream_telemetry(socket, receiver.0))
}

async fn stream_telemetry(mut socket: WebSocket, mut receiver: watch::Receiver<Telemetry>) {
    loop {
        let snapshot = receiver.borrow_and_update().clone();

        let Ok(json) = serde_json::to_string(&snapshot) else {
            break;
        };

        if socket.send(Message::Text(json.into())).await.is_err() {
            break;
        }

        // Wait for a new snapshot rather than busy-polling
        if receiver.changed().await.is_err() {
            break;
        }
    }
}
