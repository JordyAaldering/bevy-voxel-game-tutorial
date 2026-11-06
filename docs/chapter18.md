# Shader programming

We now have a bunch of compute-intensive noise algorithms.
We can offload these to the GPU using shaders to improve performance.
However, this is not easy.
If we use a blocking compute shader, computations will stall the game and cause stuttering.
Instead, we should use _async readback_: the cave generation phase waits for results to come in before continuing.

## Plan

For this chapter, move 3D OpenSimplex2S sampling for caves to the GPU.
Assume the CPU noise implementation already uses `f32` and provides batch sampling; leave height generation, ores, structures, and meshing on the CPU.

1. **Define the noise batch.** Describe a 3D grid using its world-space origin, dimensions, seed, and `NoiseSettings`.
   The result is a flat array of `f32` values with a documented index order shared by the CPU and shader.
   The shader applies the same offsets, octaves, frequency, persistence, lacunarity, amplitude, and bias as the CPU batch implementation.

2. **Add a compute shader.** Port 3D OpenSimplex2S to a shader and evaluate one grid position per invocation.
   Sample in world coordinates so neighboring chunks use the same noise field, and guard invocations outside the grid.
   Agree on a shader-compatible representation for the seed and integer hashing; `f32` alone does not address the existing 64-bit integer operations.

3. **Set up GPU jobs and readback.** Add a render-side compute pipeline with parameter and output buffers.
   Queue noise batches from the main world, dispatch them when the pipeline and buffers are ready, and copy each output buffer for asynchronous readback after its dispatch.
   Deliver each completed array back to the main world without blocking a frame.
   Limit in-flight jobs and retain their buffers until readback completes.

4. **Connect jobs to chunk generation.** Extend `ChunkGenerationTask` to represent a pending GPU job, or bridge readback into its existing task interface.
   Start a noise batch during `CaveGeneration` and keep the chunk in that phase until its result arrives.
   Associate each job with a unique ID, chunk entity, coordinate, and phase so results for unloaded, replaced, or otherwise stale chunks are discarded.
   Report pipeline and readback failures explicitly; do not leave a chunk waiting indefinitely.

5. **Apply the result on the CPU.** Change `generate_caves` to consume the returned noise array instead of sampling each voxel itself.
   Keep the existing threshold, terrain-height limit, and bottom-layer rules.
   Once the cave data is applied, complete the task, advance the phase, and notify the chunk and its neighbors as before.
   For cave configurations using a noise algorithm other than OpenSimplex2S, continue using the CPU implementation until a GPU implementation exists.

6. **Preserve lifecycle and world semantics.** Unloading a chunk cancels or abandons its pending job; already submitted GPU work may still finish and its result must be ignored.
   Saves continue to record the last completed phase, so interrupted cave generation can restart.
   CPU and GPU `f32` arithmetic may still differ at thresholds, so decide how generator versions and existing worlds should handle those differences.
