# Phase'd chunk generation

Before we can start adding structures, we need to make sure that the chunk generation is robust enough to handle them.
Namely, whenever we generate a structure at a chunk boundary, we need to make sure that the structure is not cut off by the not-yet-existent neighbouring chunk.

In the previous chapter be briefly discussed the idea of generating chunks in phases.
Now, we will take a first step in doing so.

Chunk height data and meshes are generated in separate phases on the async compute pool, followed by a `Done` state once the initial mesh has been built.
A chunk must not be meshed until its height data and the height data of all four horizontal neighbours are available.
This ensures that mesh generation always has the neighbouring voxel data needed to decide which boundary faces are visible.
Later remeshes keep the chunk in `Done`; they do not repeat height generation.

Each chunk has at most one `ChunkGenerationTask`, whose result identifies the completed phase. Dropping the component when a chunk is unloaded cancels its task; retained data resumes from its last completed phase when the chunk is loaded again. Height data is not exposed for voxel interaction until its task finishes.

When chunk data becomes available or changes, an entity event marks the affected chunk for a generation check. Finishing height generation also notifies the four neighbours, since they may now have everything they need. Edits only notify the edited chunk and, on a border, the adjacent chunk. Notifications received while a task is running remain pending so that a mesh built from an older snapshot is rebuilt afterward. Active tasks are polled each frame, but a chunk waiting on its neighbours costs nothing until one of them changes.

With a 5x5 chunk generation area, the inner 3x3 chunks can have all four neighbours ready for mesh generation.
The outer ring acts as a height-data buffer.
It might be better to use a separate radius for each phase, but I feel that might become cumbersome when there are many phases.
Follow whichever approach you think is best.

## Checking the results

Using the editor, inspect the chunks around the player: the inner 3x3 area should have meshes while chunks in the outer ring wait for their neighbours' height data.
As the player moves, the meshed area should follow without gaps or duplicate boundary faces.

You might also want to add a nice visualization to the telemetry server to show how many chunks are in each phase.
