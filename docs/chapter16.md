# Trees, finally!

Finally, we can start _sprucing_ up our world!

Structures can write into adjacent chunks, so meshing must wait until nearby structures have been placed.
Optional `direction` field is stored in the structure format but is not yet applied.

TODO: variants, so we can have variations of the same structure type, like different trees.

### Steps performed

- Added a reusable `Structure` / `StructureVoxel` data model. Each voxel has a signed `WorldCoord` offset from the structure origin, a `VoxelType`, and an optional direction. Missing positions are left untouched; an explicit `"air"` voxel clears existing terrain.
- Made `WorldCoord` serialize as a three-element array, so structure positions in JSON are concise, for example `"pos": [0, 1, 0]`.
- Added `assets/structures/tree.json` to describe a tree as voxel offsets. The tree has a two-block log trunk and a canopy of leaves.
- Added a `Structures` resource that loads and deserializes `tree.json` during startup.
- Added `GenerateStructures` between height generation and mesh generation. Each chunk now places a tree in its center and another at its far corner, with each origin positioned one voxel above the terrain height.
- A `ChunkGenerationTask` computes structure origins asynchronously; on completion, structure placement writes voxel data into whichever chunk contains each offset. This lets the corner tree's leaves cross chunk boundaries without replacing concurrent changes to neighboring chunks.
- Added an eight-neighbour chunk list and generation readiness checks. A chunk waits for all eight neighbours to generate terrain before placing structures, and waits for their structures before meshing, so overlapping tree data is settled first.
- Notify all eight neighbours when a chunk advances a generation phase, allowing waiting chunks to resume.
- Increased the chunk load radius to three. The outer two rings provide neighbouring terrain and structure data; the inner 3x3 area can be meshed reliably.

Voxel interaction waits until the chunk and all eight neighbors have completed structure generation, so edits cannot be overwritten by an in-flight generation task. Tasks are canceled on unload and restart from the last completed phase on reload.
