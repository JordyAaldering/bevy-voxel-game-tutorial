use super::{NoiseGenerationTask, NoiseSettings, NoiseType, opensimplex2s};

use crate::{CHUNK_HEIGHT, CHUNK_WIDTH};

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use better_default::Default;
use bevy::{
    prelude::*,
    render::{
        Render,
        RenderApp,
        render_resource::{
            BindGroupLayoutDescriptor,
            BindGroupLayoutEntries,
            CachedComputePipelineId,
            CachedPipelineState,
            ComputePipeline,
            ComputePipelineDescriptor,
            PipelineCache,
            ShaderStages,
            binding_types::{storage_buffer, storage_buffer_read_only},
        },
        renderer::{
            RenderAdapterInfo,
            RenderDevice,
            RenderQueue,
        },
    },
    shader::{Shader, ShaderCacheError},
    tasks::AsyncComputeTaskPool,
};
use wgpu::util::DeviceExt;

const VOXELS: usize = CHUNK_WIDTH * CHUNK_HEIGHT * CHUNK_WIDTH;
const TIMEOUT: Duration = Duration::from_secs(60);
const MAX_IN_FLIGHT: usize = 4;

#[cfg(test)]
#[path = "gpu_test.rs"]
mod gpu_test;

#[derive(Resource, Clone, Default)]
struct PipelineBridge(Arc<Mutex<PipelineState>>);

#[derive(Default)]
struct PipelineState {
    shader: Option<Handle<Shader>>,
    result: Option<Result<ComputePipeline, String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Gpu,
    Cpu,
}

#[derive(Default)]
pub struct GpuPlugin {
    #[default(Mode::Auto)]
    pub mode: Mode,
}

#[derive(Resource)]
struct GpuConfig {
    mode: Mode,
}

impl Plugin for GpuPlugin {
    fn build(&self, app: &mut App) {
        let bridge = PipelineBridge::default();

        app.insert_resource(bridge.clone())
            .insert_resource(GpuConfig {
                mode: self.mode,
            })
            .add_systems(Startup, initialize)
            .add_systems(Update, poll);

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(bridge)
                .add_systems(Render, compile_pipeline);
        }
    }
}

fn compile_pipeline(bridge: Res<PipelineBridge>, cache: Res<PipelineCache>, mut id: Local<Option<CachedComputePipelineId>>) {
    let mut state = bridge.0.lock().expect("noise pipeline lock poisoned");

    if state.result.is_some() {
        return;
    }

    if id.is_none() {
        let Some(shader) = state.shader.clone() else {
            return
        };

        let layout = BindGroupLayoutDescriptor::new(
            "noise batch",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    storage_buffer_read_only::<Vec<u32>>(false),
                    storage_buffer::<Vec<f32>>(false),
                ),
            ),
        );

        *id = Some(cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("3D noise batch".into()), layout: vec![layout], shader,
            entry_point: Some("noise_batch".into()), ..default()
        }));
    }

    let pipeline = id.expect("pipeline was queued");
    match cache.get_compute_pipeline_state(pipeline) {
        CachedPipelineState::Ok(_) => {
            state.result = Some(Ok(cache.get_compute_pipeline(pipeline).unwrap().clone()));
        }
        CachedPipelineState::Err(ShaderCacheError::ShaderNotLoaded(_) |
        ShaderCacheError::ShaderImportNotYetAvailable) => {}
        CachedPipelineState::Err(e) => {
            state.result = Some(Err(e.to_string()));
        }
        CachedPipelineState::Queued |
        CachedPipelineState::Creating(_) => {}
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
pub struct GpuBackend {
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

impl GpuBackend {
    pub fn area3(&mut self, settings: NoiseSettings, seed: i32, chunk_x: i32, chunk_z: i32) -> NoiseGenerationTask {
        let (sender, receiver) = async_channel::bounded(1);
        if let Some(error) = &self.failed {
            let _ = sender.try_send(Err(error.clone()));
            return NoiseGenerationTask { receiver };
        }

        let request = Request {
            settings,
            seed,
            chunk_x,
            chunk_z,
            sender,
        };

        if self.mode == Mode::Cpu {
            request.cpu();
        } else {
            self.queued.push_back(request);
        }

        NoiseGenerationTask { receiver }
    }
}

fn initialize(
    mut commands: Commands,
    config: Res<GpuConfig>,
    device: Option<Res<RenderDevice>>,
    queue: Option<Res<RenderQueue>>,
    adapter: Option<Res<RenderAdapterInfo>>,
    bridge: Res<PipelineBridge>,
    mut shaders: ResMut<Assets<Shader>>,
) {
    let mut backend = GpuBackend {
        mode: config.mode,
        gpu: None,
        queued: VecDeque::new(),
        failed: None,
    };

    if config.mode != Mode::Cpu {
        match (device, queue, adapter) {
            (Some(_), Some(_), Some(adapter))
                if config.mode == Mode::Auto && adapter.device_type == wgpu::DeviceType::Cpu =>
            {
                info!("Noise generation uses CPU: software adapter");
                backend.mode = Mode::Cpu;
            }
            (Some(device), Some(queue), Some(_)) => {
                let limits = device.limits();
                if limits.max_compute_invocations_per_workgroup < 64
                    || limits.max_compute_workgroup_size_x < 64
                    || limits.max_storage_buffers_per_shader_stage < 2
                {
                    if config.mode == Mode::Gpu {
                        panic!("GPU noise: insufficient compute capabilities");
                    }
                    error!("GPU noise unavailable: insufficient compute capabilities; using CPU");
                    backend.mode = Mode::Cpu;
                } else {
                    let shader = shaders.add(Shader::from_wesl(concat!(
                        include_str!("voronoi.wesl"), "\n",
                        include_str!("opensimplex2s.wesl"), "\n",
                        include_str!("noise_batch.wesl"),
                    ), "embedded://noise_batch.wesl"));

                    bridge.0.lock().expect("noise pipeline lock poisoned").shader = Some(shader);

                    backend.gpu = Some(Gpu {
                        device: device.clone(),
                        queue: queue.clone(),
                        pipeline: None,
                        started: Instant::now(),
                        jobs: Vec::new(),
                    });
                }
            }
            _ if config.mode == Mode::Gpu => {
                panic!("GPU noise: render device is unavailable");
            }
            _ => {
                error!("GPU noise unavailable: no render device; using CPU");
                backend.mode = Mode::Cpu;
            }
        }
    }

    commands.insert_resource(backend);
}

impl Gpu {
    fn submit(&mut self, request: Request) -> Result<(), (Request, String)> {
        let size = (VOXELS * 4) as u64;
        let limits = self.device.limits();

        if size > limits.max_buffer_size
            || size > limits.max_storage_buffer_binding_size as u64
            || VOXELS.div_ceil(64) > limits.max_compute_workgroups_per_dimension as usize
            || request.settings.octaves > u32::MAX as usize
        {
            return Err((request, "noise batch exceeds GPU limits".into()));
        }

        let settings = request.settings;
        let mut words = vec![
            (request.chunk_x * CHUNK_WIDTH as i32) as u32,
            (request.chunk_z * CHUNK_WIDTH as i32) as u32,
            CHUNK_WIDTH as u32,
            CHUNK_HEIGHT as u32,
            request.seed as u32,
            settings.offset.x.to_bits(),
            settings.offset.y.to_bits(),
            settings.octaves as u32,
            settings.frequency.to_bits(),
            settings.persistence.to_bits(),
            match settings.noise_type {
                NoiseType::Voronoi => 0,
                NoiseType::OpenSimplex2S => 1,
            },
        ];
        words.extend(opensimplex2s::GRADIENTS_3D.iter().map(|value| value.to_bits()));
        words.extend([settings.lacunarity.to_bits(), settings.amplitude.to_bits(), settings.bias.to_bits()]);

        let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let raw = self.device.wgpu_device();

        let params = raw.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("noise parameters"),
            contents: &bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });

        let output = raw.create_buffer(&wgpu::BufferDescriptor {
            label: Some("noise output"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let buffer = raw.create_buffer(&wgpu::BufferDescriptor {
            label: Some("noise readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

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
        buffer.slice(..).map_async(
            wgpu::MapMode::Read,
            move |status| {
                let _ = tx.send(status);
            },
        );

        self.jobs.push(Job {
            request,
            buffer,
            result: Mutex::new(result),
            started: Instant::now(),
        });

        Ok(())
    }

    fn poll(&mut self, queued: &mut VecDeque<Request>, bridge: &PipelineBridge) -> Result<(), String> {
        if self.pipeline.is_none() {
            let result = bridge.0
                .lock()
                .expect("noise pipeline lock poisoned")
                .result
                .clone();

            match result {
                Some(Ok(pipeline)) => {
                    info!("Using GPU noise generation");
                    self.pipeline = Some(pipeline);
                }
                Some(Err(error)) => {
                    return Err(format!("noise shader: {error}"));
                }
                None if self.started.elapsed() > TIMEOUT => {
                    return Err("noise pipeline timed out".into());
                }
                None => {
                    return Ok(());
                }
            }
        }
        let mut i = 0;
        while i < self.jobs.len() {
            let job = &self.jobs[i];
            match job.result.lock().expect("noise readback lock poisoned").try_recv() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    return Err(format!("noise readback: {error}"));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("noise readback disconnected".into());
                }
                Err(mpsc::TryRecvError::Empty) if job.started.elapsed() > TIMEOUT => {
                    return Err("noise readback timed out".into());
                }
                Err(mpsc::TryRecvError::Empty) => {
                    i += 1;
                    continue;
                }
            }

            let job = self.jobs.swap_remove(i);
            let values = match job.buffer.slice(..).get_mapped_range() {
                Ok(mapped) => {
                    mapped.chunks_exact(4)
                        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
                        .collect()
                }
                Err(e) => {
                    queued.push_back(job.request);
                    return Err(format!("noise readback mapping: {e}"));
                }
            };

            job.buffer.unmap();
            let _ = job.request.sender.try_send(Ok(values));
        }

        while self.jobs.len() < MAX_IN_FLIGHT {
            let Some(request) = queued.pop_front() else { break };
            if request.sender.is_closed() { continue; }
            if let Err((request, error)) = self.submit(request) {
                queued.push_front(request);
                return Err(error);
            }
        }

        Ok(())
    }
}

fn poll(mut backend: ResMut<GpuBackend>, bridge: Res<PipelineBridge>) {
    let backend: &mut GpuBackend = &mut *backend;
    let Some(gpu) = &mut backend.gpu else {
        return
    };

    let Err(e) = gpu.poll(&mut backend.queued, &bridge) else {
        return
    };

    let mut requests: Vec<_> = backend.queued.drain(..).collect();
    if let Some(gpu) = backend.gpu.take() {
        requests.extend(gpu.jobs.into_iter().map(|job| job.request));
    }

    match backend.mode {
        Mode::Auto => {
            error!("GPU noise failed: {e}; switching to CPU");
            backend.mode = Mode::Cpu;
            for request in requests {
                request.cpu();
            }
        }
        Mode::Gpu => {
            error!("GPU noise failed: {e}");
            for request in requests {
                let _ = request.sender.try_send(Err(e.clone()));
            }
            backend.failed = Some(e);
        }
        Mode::Cpu => unreachable!(),
    }
}
