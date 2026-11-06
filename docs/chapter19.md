# Towards a smooth terrain

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
