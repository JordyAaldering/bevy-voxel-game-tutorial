# Breaking and placing blocks

## Raycasting

To break or place a block, we first need to know which voxel the player is looking at.
A physics raycast would be overkill here: voxels are arranged on a regular grid, so we can traverse that grid directly using the [DDA algorithm](https://en.wikipedia.org/wiki/Digital_differential_analyzer_(graphics_algorithm)).

Create a function that takes the world, a ray origin, a direction, and a maximum distance, and returns an optional hit.
Start at the voxel containing the camera, found by flooring each coordinate of its position.
For each axis, calculate the direction to step through the grid and the distance between successive voxel boundaries.
Begin with the distance to the next boundary on each axis; repeatedly step along whichever axis has the nearest boundary, updating that axis's distance.
At each step, query whether the new voxel is solid.
Stop and return no hit when the next boundary is beyond the interaction distance.

The face crossed on the final step is also important: it tells us which side of the hit voxel the ray entered.
Include both the voxel's world coordinate and that face in the hit result.
When the ray has a zero component on an axis, treat its next boundary distance as infinity so the traversal never steps along that axis.

Use the camera's global position and forward direction as the ray origin and direction.
For visual feedback, call the raycast each frame and draw the four edges of the hit face with a white gizmo.
For interaction, respond to a newly pressed left or right mouse button: replace the hit voxel with air to break it, or put dirt in the adjacent voxel in the hit face's direction to place it.
Put the interaction system under the same focused-controls run condition as player input, so clicks intended for egui do not edit the world.

## Refactoring

Until now, a chunk's mesh was created separately from its voxel data.
That makes it awkward to find and update the right mesh after an edit.
Instead, give each chunk an ECS entity, and attach its `Chunk` component, transform, and render components to that entity.
Store the entity ID in the corresponding `ChunkData`; the world can then update the voxel data and mark the right entity for remeshing.

Split chunk generation from mesh generation.
During world setup, create all chunk entities and data first, marking them as needing a mesh.
The remeshing system can then build meshes after the neighbouring chunk data is available.
Keep the texture-atlas layout in a resource so the mesh-building system can access it.

We also want to build meshes off the main thread.
A task cannot borrow the world while the main thread may change it, so store the chunk data as `Arc<ChunkData>`.
A task can clone an `Arc` to retain access to a consistent snapshot without copying all the voxel data.
When editing a chunk, use `Arc::make_mut`: it modifies the data directly if no other code is using it, and makes a copy if a mesh task still holds a reference.

## Re-meshing dirty chunks

Add a `NeedsRemesh` marker component to identify chunk entities whose mesh is out of date.
Whenever a voxel changes, mark its chunk.
If it lies on the left, right, front, or back edge, also mark the corresponding neighbouring chunk: the changed voxel may have made a face on that neighbour visible or hidden.
There is no need to mark chunks above or below, because the chunks in this world extend through the full vertical range.

When a chunk is spawned, mark it as dirty too.
Its neighbours may also need remeshing, because a newly available chunk can hide faces that were previously rendered at their shared border.

`NeedsRemesh` is a frequently added and removed marker, so configure it to use Bevy's sparse-set component storage.
This avoids moving chunk entities between archetypes as the marker is toggled.
Avoid using `Changed<Chunk>` as a substitute: changing voxel data in the world does not necessarily change the `Chunk` component, and a mesh can also depend on a neighbouring chunk.

### Async tasks

Use a `MeshTask` component to track chunks that are currently being meshed.
In `PostUpdate`, chain two systems: first poll existing tasks, then start tasks for newly dirty chunks.
Polling first means a chunk whose previous task has just finished can start another task in the same schedule run if it was marked dirty in the meantime.

In the polling system, check each task without waiting for it to finish.
If it is still running, leave it alone.
When it completes, insert the mesh as a `Mesh3d` if the chunk has not been meshed before; otherwise, replace the mesh asset behind the existing handle.
Then remove the `MeshTask` component.

In the remeshing system, query for entities with `NeedsRemesh` but without `MeshTask`, so a chunk never has two mesh tasks running simultaneously.
For each one, gather the chunk data, its four horizontal neighbours, and a clone of the atlas layout.
Spawn a task on Bevy's async compute pool that calls `build_mesh` using only these inputs.
Add `MeshTask` to the chunk entity and remove `NeedsRemesh` when starting the task.

The task receives `Arc` handles for the chunk and its neighbours.
This lets `build_mesh` determine whether boundary faces are visible without accessing or borrowing the live world.
If a voxel changes while the task is running, the world can add `NeedsRemesh` again.
The active task is left alone; when it completes, its mesh is applied and its task component removed.
The still-dirty chunk can then be meshed again using the latest data.
This can briefly display an out-of-date mesh, but avoids losing edits or doing duplicate work.

### An alternative: entity events

It is worth understanding what happens when we add and remove `NeedsRemesh`.
Bevy normally stores components in tables grouped by an entity's exact set of table components.
This grouping is called an archetype.
Adding or removing a table-stored component changes the entity's archetype, so Bevy has to move its table data to the appropriate storage.
This makes table components efficient to query, but repeated structural changes can have a cost.

The `NeedsRemesh` marker is different from most components: it is added and removed as chunks become dirty and get scheduled for remeshing.
By storing it in a sparse set, we avoid moving the chunk's table data every time this marker changes.
Sparse-set storage has different query trade-offs, so it is a useful option for frequently toggled markers.

Another design is to send an event or message identifying a chunk that needs remeshing, and let a system handle those requests.
This avoids changing the chunk entity's components just to record that work is pending, and can be a natural fit when many different systems request the same operation.
The handler still needs to account for duplicate requests, requests for chunks that have since been unloaded, and chunks that are already being meshed.
It also needs to preserve a request if the chunk changes while a task is in flight.

Neither approach is automatically faster: events add their own bookkeeping, while marker components make pending work visible directly in ECS queries.
For this implementation, the sparse-set marker is straightforward and handles the dirty/in-flight lifecycle clearly.
If profiling shows that it is a bottleneck, compare it with an event-based design in the actual workload.
