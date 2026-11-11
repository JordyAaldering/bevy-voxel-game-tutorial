use super::{NoiseGenerationTask, NoiseSettings, NoiseType, opensimplex2s};

use crate::{CHUNK_HEIGHT, CHUNK_WIDTH};

use std::{collections::VecDeque, sync::{Arc, Mutex, mpsc}, time::{Duration, Instant}};
use bevy::{
    prelude::*,
    render::{
        Render, RenderApp,
        render_resource::{
            BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedComputePipelineId, CachedPipelineState,
            ComputePipeline, ComputePipelineDescriptor, PipelineCache, ShaderStages,
            binding_types::{storage_buffer, storage_buffer_read_only},
        },
        renderer::{RenderAdapterInfo, RenderDevice, RenderQueue},
    },
    shader::{Shader, ShaderCacheError},
    tasks::AsyncComputeTaskPool,
};
use wgpu::util::DeviceExt;

const VOXELS: usize = CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH;
const MAX_IN_FLIGHT: usize = 4;
const TIMEOUT: Duration = Duration::from_secs(60);

#[cfg(test)]
#[path = "gpu_test.rs"]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode { Auto, Cpu, Gpu }

impl Mode {
    fn from_env() -> Result<Self, String> {
        match std::env::var("BEVY_VOXEL_NOISE_BACKEND") {
            Ok(value) => match value.as_str() {
                "auto" => Ok(Self::Auto),
                "cpu" => Ok(Self::Cpu),
                "gpu" => Ok(Self::Gpu),
                _ => Err(format!("invalid BEVY_VOXEL_NOISE_BACKEND {value:?}; expected auto, cpu, or gpu")),
            },
            Err(std::env::VarError::NotPresent) => Ok(Self::Auto),
            Err(error) => Err(format!("invalid BEVY_VOXEL_NOISE_BACKEND: {error}")),
        }
    }
}

#[derive(Resource, Clone, Default)]
struct PipelineBridge(Arc<Mutex<PipelineState>>);

#[derive(Default)]
struct PipelineState {
    shader: Option<Handle<Shader>>,
    result: Option<Result<ComputePipeline, String>>,
}

pub struct NoisePlugin;

impl Plugin for NoisePlugin {
    fn build(&self, app: &mut App) {
        let bridge = PipelineBridge::default();
        app.insert_resource(bridge.clone())
            .add_systems(Startup, initialize)
            .add_systems(Update, poll);
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.insert_resource(bridge).add_systems(Render, compile_pipeline);
        }
    }
}

fn compile_pipeline(bridge: Res<PipelineBridge>, cache: Res<PipelineCache>, mut id: Local<Option<CachedComputePipelineId>>) {
    let mut state = bridge.0.lock().expect("noise pipeline lock poisoned");
    if state.result.is_some() { return; }
    if id.is_none() {
        let Some(shader) = state.shader.clone() else { return };
        let layout = BindGroupLayoutDescriptor::new("noise batch", &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (storage_buffer_read_only::<Vec<u32>>(false), storage_buffer::<Vec<f32>>(false)),
        ));
        *id = Some(cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("3D noise batch".into()), layout: vec![layout], shader,
            entry_point: Some("noise_batch".into()), ..default()
        }));
    }
    let pipeline = id.expect("pipeline was queued");
    match cache.get_compute_pipeline_state(pipeline) {
        CachedPipelineState::Ok(_) => state.result = Some(Ok(cache.get_compute_pipeline(pipeline).unwrap().clone())),
        CachedPipelineState::Err(ShaderCacheError::ShaderNotLoaded(_) | ShaderCacheError::ShaderImportNotYetAvailable) => {}
        CachedPipelineState::Err(error) => state.result = Some(Err(error.to_string())),
        CachedPipelineState::Queued | CachedPipelineState::Creating(_) => {}
    }
}

struct Request {
    settings: NoiseSettings,
    seed: i32,
    chunk_x: i32,
    chunk_z: i32,
    sender: async_channel::Sender<Result<Vec<f32>, String>>,
}

impl Request {
    fn cpu(self) {
        AsyncComputeTaskPool::get().spawn(async move {
            if !self.sender.is_closed() {
                let values = self.settings.area3(self.seed, self.chunk_x, self.chunk_z);
                let _ = self.sender.send(Ok(values)).await;
            }
        }).detach();
    }
}

struct Job {
    request: Request,
    buffer: wgpu::Buffer,
    result: Mutex<mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>,
    started: Instant,
}

#[derive(Resource)]
pub struct NoiseBackend {
    mode: Mode,
    gpu: Option<Gpu>,
    queued: VecDeque<Request>,
    failed: Option<String>,
}

struct Gpu {
    device: RenderDevice,
    queue: RenderQueue,
    pipeline: Option<ComputePipeline>,
    started: Instant,
    jobs: Vec<Job>,
}

impl NoiseBackend {
    pub fn area3(&mut self, settings: NoiseSettings, seed: i32, chunk_x: i32, chunk_z: i32) -> NoiseGenerationTask {
        let (sender, receiver) = async_channel::bounded(1);
        if let Some(error) = &self.failed {
            let _ = sender.try_send(Err(error.clone()));
            return NoiseGenerationTask { receiver };
        }
        let request = Request { settings, seed, chunk_x, chunk_z, sender };
        if self.mode == Mode::Cpu {
            request.cpu();
        } else {
            self.queued.push_back(request);
        }
        NoiseGenerationTask { receiver }
    }

    fn fail(&mut self, error: String) {
        let mut requests: Vec<_> = self.queued.drain(..).collect();
        if let Some(gpu) = self.gpu.take() {
            requests.extend(gpu.jobs.into_iter().map(|job| job.request));
        }
        match self.mode {
            Mode::Auto => {
                error!("GPU noise failed: {error}; switching to CPU");
                self.mode = Mode::Cpu;
                for request in requests { request.cpu(); }
            }
            Mode::Gpu => {
                error!("GPU noise failed: {error}");
                for request in requests { let _ = request.sender.try_send(Err(error.clone())); }
                self.failed = Some(error);
            }
            Mode::Cpu => unreachable!(),
        }
    }
}

fn initialize(
    mut commands: Commands, device: Option<Res<RenderDevice>>, queue: Option<Res<RenderQueue>>,
    adapter: Option<Res<RenderAdapterInfo>>, mut shaders: ResMut<Assets<Shader>>, bridge: Res<PipelineBridge>,
) {
    let mode = Mode::from_env().unwrap_or_else(|error| panic!("{error}"));
    let mut backend = NoiseBackend { mode, gpu: None, queued: VecDeque::new(), failed: None };
    if mode != Mode::Cpu {
        match (device, queue, adapter) {
            (Some(_), Some(_), Some(adapter)) if mode == Mode::Auto && adapter.device_type == wgpu::DeviceType::Cpu => {
                info!("Noise generation uses CPU: software adapter");
                backend.mode = Mode::Cpu;
            }
            (Some(device), Some(queue), Some(_)) => {
                let limits = device.limits();
                if limits.max_compute_invocations_per_workgroup < 64 || limits.max_compute_workgroup_size_x < 64 || limits.max_storage_buffers_per_shader_stage < 2 {
                    if mode == Mode::Gpu { panic!("GPU noise: insufficient compute capabilities"); }
                    error!("GPU noise unavailable: insufficient compute capabilities; using CPU");
                    backend.mode = Mode::Cpu;
                } else {
                    let shader = shaders.add(Shader::from_wesl(concat!(
                        include_str!("voronoi.wesl"), "\n",
                        include_str!("opensimplex2s.wesl"), "\n",
                        include_str!("noise_batch.wesl"),
                    ), "embedded://noise_batch.wesl"));
                    bridge.0.lock().expect("noise pipeline lock poisoned").shader = Some(shader);
                    backend.gpu = Some(Gpu { device: device.clone(), queue: queue.clone(), pipeline: None, started: Instant::now(), jobs: Vec::new() });
                }
            }
            _ if mode == Mode::Gpu => panic!("GPU noise: render device is unavailable"),
            _ => { error!("GPU noise unavailable: no render device; using CPU"); backend.mode = Mode::Cpu; }
        }
    }
    commands.insert_resource(backend);
}

impl Gpu {
    fn submit(&mut self, request: Request) -> Result<(), (Request, String)> {
        let size = (VOXELS * 4) as u64;
        let limits = self.device.limits();
        if size > limits.max_buffer_size || size > limits.max_storage_buffer_binding_size as u64
            || VOXELS.div_ceil(64) > limits.max_compute_workgroups_per_dimension as usize
            || request.settings.octaves > u32::MAX as usize
        {
            return Err((request, "noise batch exceeds GPU limits".into()));
        }
        let settings = request.settings;
        let mut words = vec![
            (request.chunk_x * CHUNK_WIDTH as i32) as u32, (request.chunk_z * CHUNK_WIDTH as i32) as u32,
            CHUNK_WIDTH as u32, CHUNK_HEIGHT as u32, request.seed as u32,
            settings.offset.x.to_bits(), settings.offset.y.to_bits(), settings.octaves as u32,
            settings.frequency.to_bits(), settings.persistence.to_bits(),
            match settings.noise_type { NoiseType::Voronoi => 0, NoiseType::OpenSimplex2S => 1 },
        ];
        words.extend(opensimplex2s::GRADIENTS_3D.iter().map(|value| value.to_bits()));
        words.extend([settings.lacunarity.to_bits(), settings.amplitude.to_bits(), settings.bias.to_bits()]);
        let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let raw = self.device.wgpu_device();
        let params = raw.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("noise parameters"), contents: &bytes, usage: wgpu::BufferUsages::STORAGE });
        let output = raw.create_buffer(&wgpu::BufferDescriptor { label: Some("noise output"), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let buffer = raw.create_buffer(&wgpu::BufferDescriptor { label: Some("noise readback"), size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let pipeline = self.pipeline.as_ref().expect("pipeline is ready");
        let bind = raw.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("noise bindings"), layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() }, wgpu::BindGroupEntry { binding: 1, resource: output.as_entire_binding() }],
        });
        let mut encoder = raw.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(VOXELS.div_ceil(64) as u32, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &buffer, 0, size);
        self.queue.submit([encoder.finish()]);
        let (tx, result) = mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |status| { let _ = tx.send(status); });
        self.jobs.push(Job { request, buffer, result: Mutex::new(result), started: Instant::now() });
        Ok(())
    }
}

fn poll(mut backend: ResMut<NoiseBackend>, bridge: Res<PipelineBridge>) {
    if backend.gpu.is_none() { return; }
    let gpu = backend.gpu.as_mut().unwrap();
    if gpu.pipeline.is_none() {
        let result = bridge.0.lock().expect("noise pipeline lock poisoned").result.clone();
        match result {
            Some(Ok(pipeline)) => { info!("Using GPU noise generation"); gpu.pipeline = Some(pipeline); }
            Some(Err(error)) => { backend.fail(format!("noise shader: {error}")); return; }
            None if gpu.started.elapsed() > TIMEOUT => { backend.fail("noise pipeline timed out".into()); return; }
            None => return,
        }
    }
    let mut i = 0;
    while i < backend.gpu.as_ref().unwrap().jobs.len() {
        let gpu = backend.gpu.as_mut().unwrap();
        let job = &gpu.jobs[i];
        let result = match job.result.lock().expect("noise readback lock poisoned").try_recv() {
            Ok(Ok(())) => Some(Ok(())),
            Ok(Err(error)) => Some(Err(format!("noise readback: {error}"))),
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("noise readback disconnected".into())),
            Err(mpsc::TryRecvError::Empty) if job.started.elapsed() > TIMEOUT => Some(Err("noise readback timed out".into())),
            Err(mpsc::TryRecvError::Empty) => None,
        };
        match result {
            None => { i += 1; }
            Some(Err(error)) => { backend.fail(error); return; }
            Some(Ok(())) => {
                let job = gpu.jobs.swap_remove(i);
                let bytes = match job.buffer.slice(..).get_mapped_range() {
                    Ok(mapped) => mapped.to_vec(),
                    Err(error) => {
                        backend.queued.push_back(job.request);
                        backend.fail(format!("noise readback mapping: {error}"));
                        return;
                    }
                };
                job.buffer.unmap();
                let values = bytes.chunks_exact(4).map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap())).collect();
                let _ = job.request.sender.try_send(Ok(values));
            }
        }
    }
    while backend.gpu.as_ref().unwrap().jobs.len() < MAX_IN_FLIGHT {
        let Some(request) = backend.queued.pop_front() else { break };
        if request.sender.is_closed() { continue; }
        if let Err((request, error)) = backend.gpu.as_mut().unwrap().submit(request) {
            backend.queued.push_front(request);
            backend.fail(error);
            return;
        }
    }
}
