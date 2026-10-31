# Breaking and placing blocks

To determine where to place or destroy a block we need to determine what we are looking at.
Typically, this is done using 'raycasts'.
But again, because we are in our simple voxel world we can actually do this entirely mathematically.

We use the 'DDA' algorithm.
https://en.wikipedia.org/wiki/Digital_differential_analyzer_(graphics_algorithm)

We can use this information to determine which block we are looking at, and should break.

For debugging purposes we also add an outline to the face we are looking at.
Gizmos are perfect for this.

The DDA algorithm also tells us which face of the block we are looking at, so we can also use this same information to determine where to place a block.

## Refactoring

Before we continue, some refactoring is in order.
We now need to keep track of the meshes we created.
To do this, we move the created mesh inside the Chunk struct.
Each chunk then becomes responsible for updating its own chunk.
This means we need to move some logic.

## Re-meshing dirty chunks

Meshes are now local to each chunk.
Whenever a change occurs (for example, a block is placed or removed, but this could also be a tree growing), we mark that chunk as 'dirty'.
We don't immediately update the chunk.
Instead, we mark it as dirty, and the `World` becomes responsible for coordinating which chunks update when.
We do this because the world can then decide when it is a good time to re-mesh and to batch multiple meshing operations together.

However, I was not yet sure whether this still applies to ECS systems.
It seems those might naturally expose the desired dirty-tracking.
Time for some googling.

It seems we have a few alternatives.

We can add a marker:
```rust
#[derive(Component)]
struct NeedsRemesh;
```

Or we can make `Chunk` a `Component`, and then use Bevy's `Changed<Chunk>` feature to automatically detect changes.
Because the latter may also respond to changes that do not relate to the mesh, and because it does not respond to changes necessary to neighbouring chunks, I don't think that is the right approach.
However, `NeedsRemesh` seems like a very natural fit for ECS. We anyway already have an entity for the chunk: the mesh itself.

Remember that, if we modify a voxel on the edge of a chunk, we also need to modify the neighbouring chunk.

### Async tasks

We don't want to block the main thread while meshing chunks, which would otherwise cause stuttering.
For this we use [async tasks](https://bevy-cheatbook.github.io/fundamentals/async-compute.html) ([example](https://bevy.org/examples/async-tasks/async-channel-pattern)).
Async tasks run in a separate thread, which means the main game loop can continue while the mesh is being built, possibly across multiple frames.

To setup asynchronous generation of meshes we need a few things:

- A component definition containing the mesh task: `Task<Mesh>`.
  This component is added to a chunk entity when it is marked as dirty, and removed when the task is finished.
- A chunk may be modified while another chunk is reading it, so we need to make the world contain a hashmap of `Arc<ChunkData>` instead of `ChunkData`.
  This allows us to hand out shared references to chunks, while still allowing the main thread to modify them.
- A function to poll the tasks each frame, and insert the finished mesh into the assets when it is done.
- A function that checks for dirty chunks, and spawns a task to build the mesh for each dirty chunk.
- The task itself, which builds the mesh for a chunk.

---

Is NeedsRemesh still needed?

A task cannot borrow the `VoxelWorld`, because the main thread keeps modifying it.
Instead, the task gets shared handles to everything it needs: the chunk, its four neighbours (to hide faces on the chunk border), and a copy of the atlas layout.
To make this cheap the world stores chunks as `Arc<ChunkData>`, so handing a chunk to a task only copies a pointer, not the voxels.
When the main thread modifies a chunk it uses `Arc::make_mut`.
This is 'copy-on-write': the voxels are only copied if a task is still reading that chunk, so the task always sees a consistent snapshot, and in the common case nothing is copied at all.
That is also why `build_mesh` no longer takes the world, but only the neighbouring chunk data.

The process now has two steps, each its own system:

1. `remesh_chunks` finds chunks marked with `NeedsRemesh`, removes the marker, and spawns a task whose result is stored in a `MeshTask` component.
2. `poll_mesh_tasks` checks every `MeshTask` each frame without blocking (`future::poll_once`). Once a task is finished, the mesh is inserted into the assets (or replaces the existing one) and the `MeshTask` is removed.

Some details follow from this:
- A chunk is only meshed by one task at a time. If a voxel changes while its task is running, `NeedsRemesh` is added again, but `remesh_chunks` ignores chunks that still have a `MeshTask`. The finished (now slightly stale) mesh is applied, after which the chunk is meshed again with the newest data.
- If a chunk is unloaded, its entity is despawned. This drops the `MeshTask`, which cancels the task.
- Meshes now appear a frame or more after the change, so there can be a short delay before an edit becomes visible.

### Entity events

TODO: it seems that bevy has tables of entities, grouping them by their components. That means this entity moves around by adding and removing the NeedsRemesh component.
Need to determine exactly how this works, and whether our approach is problematic.
It might be that `EntityEvent` is a better approach: https://taintedcoders.com/bevy/events

Using events has some difficulties, because we need to ensure it only happens once even if multiple sources cause a remesh, and we need to gracefully handle when a chunk is marked as dirty but is then unloaded.
You would have to profile to be sure, but I don't think the performance hit of moving the entity to a different _archetype_ will be significant compared to the much higher cost of re-meshing.
