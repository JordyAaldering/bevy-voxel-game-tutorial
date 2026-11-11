# Shader programming

The ore and cave phases request chunk-sized batches of 3D noise. A
`NoiseGenerationTask` returns a `Vec<f32>` regardless of whether the CPU or GPU
produced it. The surrounding `ChunkGenerationTask` represents the whole phase;
it waits for the noise and then applies ore or cave rules on the CPU.

## Backend selection

Set `BEVY_VOXEL_NOISE_BACKEND` before starting the game:

- `auto` (default) uses the render GPU, or the CPU on a software adapter.
  Recoverable pipeline or readback failures are reported and queued work is
  retried on the CPU.
- `cpu` runs batches on the CPU.
- `gpu` requires GPU execution and reports failures instead of falling back.

For example, `BEVY_VOXEL_NOISE_BACKEND=cpu cargo run`. Fatal renderer or
device failures remain subject to Bevy's renderer error policy.

## Batches

Both 3D Voronoi and OpenSimplex2S are supported. A request includes the noise
settings, seed, and chunk coordinate. The shader applies frequency, offset,
octaves, persistence, lacunarity, amplitude, and bias; output order is
`x + CHUNK_WIDTH * (y + CHUNK_HEIGHT * z)`, as for the CPU batch.
The GPU produces scalar noise, not ore decisions. The CPU applies the ore
threshold, height range, priority, and stone-only rules to the returned values.
Height and biome sampling remain on the CPU.

Requests are queued and GPU readbacks are asynchronous, with a limit on
in-flight buffers. Dropping a chunk task drops its noise receivers; completed
results are discarded if the recipient has gone away. Saved chunks retain
their last completed generation phase.

The GPU and CPU use the same seeds, hash constants and gradients. Voronoi
results are close, but not guaranteed bit-identical. The GPU OpenSimplex2S
implementation enumerates lattice points with the same selection rules as the
CPU; small floating-point differences remain, including near cave thresholds.
Changing the backend can therefore change boundary
voxels. The new scalar ore threshold also changes ore placement compared with
the previous per-cell rarity and variable-radius algorithm.

## WESL, Bevy, and wgpu

WESL extends WGSL with module and import facilities, useful as the noise
library grows. Bevy supports both and compiles WESL to WGSL. The embedded
compute shader is assembled from separate Voronoi and seeded OpenSimplex2S
samplers and a shared batch entry point. The noise module owns the CPU/GPU
service and compiles the assembled shader through Bevy's pipeline cache.

Bevy already uses wgpu and exposes render and readback helpers, so a direct
wgpu dependency is not essential. Here it exposes the same underlying wgpu
types for buffer creation, command submission, and asynchronous mapping; it
does not add a separate renderer. Explicit mapping lets the noise service
report failures and timeouts for individual batches.
