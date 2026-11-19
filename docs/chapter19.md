# Marching cubes

![](img/chapter19.gif)

A terrain of voxels is not very pretty.
We can smooth it out by changing the way we generate meshes.
One way to do this is with _marching cubes_.
Internally it follows a similar representation, we still have a 3D grid of voxels, but instead of making perfect cubes, the shape of the mesh is determined by the _density_ of the voxel and its neighbours.

We don't want to apply this to everything; otherwise structures like houses would look very strange and wobbly.
Instead, we follow an approach similar to _7 Days to Die_: we only use marching cubes for terrain, and keep the original voxel representation for other objects.

Before we can implement the secondary mesh generation, we need to make some extensions to the internal representation.
A voxel is no longer just _on_ or _off_, but has a _density_ value.
The density determines how solid the voxel is, and is a float between 0 and 1.
As you might remember, we typically want to group data by type (_SoA_ vs _AoS_); so we add a new array of floats to the `Chunk` class, which stores the density of each voxel.
For each voxel type, we then define with which method it should be rendered: either as a cube or with marching cubes.

## Implementation steps

1. Split each chunk's generated mesh into a block mesh and a terrain mesh, with separate child entities and mesh assets.
2. Keep the existing density array and update it when voxels change. Binary terrain meshing and collision do not use it.
3. Build a `(width + 1) x (height + 1) x (width + 1)` boolean field at voxel centers. Terrain is solid; air and blocks are empty.
4. Fill the field's positive chunk boundaries from the north, east, and north-east neighbors. Treat samples above the world as empty.
5. For each marching cell, collect its eight samples and skip cells that are entirely solid or empty. Each cell belongs to the chunk containing its minimum-coordinate sample.
6. Find the midpoint of each edge where the samples differ, connect the crossings into contours, and resolve ambiguous faces consistently between cells.
7. Triangulate each contour using only edge crossings, preferring coplanar adjacent triangles for flatter surfaces. Calculate outward normals, and choose a terrain texture and atlas UVs for each triangle.
8. Generate and update both chunk meshes together, notifying neighboring chunks when boundary edits affect their meshes.
9. Test the binary marching cases, shared faces, chunk boundaries, and the exclusion of blocks and density from the terrain field.
10. Share marching-cell triangles between rendering and player collision. Collision samples the feet and head at the player's center and the sides at several body points; the sampled surface matches the mesh.

The center-sampled surface can extend into neighboring air voxels. The `CONNECT_DIAGONALS` constant in `marching.rs` controls whether face-diagonal contacts are connected; corner-only contacts can remain separate. Rendering and physics use the same setting.
