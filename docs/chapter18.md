# Shader programming

We now have a bunch of compute-intensive noise algorithms.
We can offload these to the GPU using shaders to improve performance.
However, this is not easy.
If we use a blocking compute shader, computations will stall the game and cause stuttering.
Instead, we should use _async readback_: the cave generation phase waits for results to come in before continuing.

## Plan

For this chapter, move 3D Voronoi sampling for caves to the GPU.
The CPU implementation already provides chunk-sized batches and uses `f32` coordinates, `i32` seeds, and wrapping `u32` hashes.
The seeded Voronoi shader mirrors the CPU algorithm, but is not yet connected to chunk generation.
The default cave configuration still uses OpenSimplex2S; a Voronoi GPU backend would apply only when the cave noise type is configured as Voronoi.
Leave height generation, ores, structures, and meshing on the CPU.

1. **Define the noise batch.** Describe a 3D grid using its world-space origin, dimensions, seed, and `NoiseSettings`.
   The result is a flat array of `f32` values with a documented index order shared by the CPU and shader.
   The shader applies the same offsets, octaves, frequency, persistence, lacunarity, amplitude, and bias as the CPU batch implementation.

2. **Add a compute shader.** Use the seeded 3D Voronoi function in a compute shader and evaluate one grid position per invocation.
   Sample in world coordinates so neighboring chunks use the same noise field, and guard invocations outside the grid.
   Match the CPU's 32-bit seed and wrapping hash operations.

3. **Set up GPU jobs and readback.** Add a render-side compute pipeline with parameter and output buffers.
   Queue noise batches from the main world, dispatch them when the pipeline and buffers are ready, and copy each output buffer for asynchronous readback after its dispatch.
   Deliver each completed array back to the main world without blocking a frame.
   Limit in-flight jobs and retain their buffers until readback completes.

4. **Connect jobs to chunk generation.** Extend `ChunkGenerationTask` to represent a pending GPU job, or bridge readback into its existing task interface.
   Start a noise batch during `CaveGeneration` and keep the chunk in that phase until its result arrives.
   Associate each job with a unique ID, chunk entity, coordinate, and phase so results for unloaded, replaced, or otherwise stale chunks are discarded.
   Report pipeline and readback failures explicitly; do not leave a chunk waiting indefinitely.

5. **Apply the result on the CPU.** Change `generate_caves` to consume the GPU-returned array in place of its current CPU noise batch.
   Keep the existing threshold, terrain-height limit, and bottom-layer rules.
   Once the cave data is applied, complete the task, advance the phase, and notify the chunk and its neighbors as before.
   For cave configurations using a noise algorithm other than Voronoi, continue using the CPU implementation until a GPU implementation exists.
   Support automatic, CPU, and GPU backend selection: automatic mode reports GPU failures and retries on the CPU, while explicit GPU mode reports them without silently falling back.

6. **Preserve lifecycle and world semantics.** Unloading a chunk cancels or abandons its pending job; already submitted GPU work may still finish and its result must be ignored.
   Saves continue to record the last completed phase, so interrupted cave generation can restart.
   CPU and GPU `f32` arithmetic may still differ at thresholds, so decide how generator versions and existing worlds should handle those differences.
