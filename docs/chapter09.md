# Terrain Generation

Rather than using the old-school Perlin noise algorithm, I'll opt for the OpenSimplex2 algorithm.
In particular, I select the OpenSimplex2S variant (https://github.com/KdotJPG/OpenSimplex2/blob/master/java/OpenSimplex2S.java).
Instead of evaluating the BCC lattice precisely, it applies a geometric "skewing" factor to the coordinates before processing them.
This produces slightly smoother results, but is much faster.
Once we combine multiple noise layers, and since our world is blocky anyway, this difference should not be visible.

## Simplex Noise

Don't read the Wikipedia article on Simplex noise, it is not very helpful.
However, I found this video pretty clear: https://youtu.be/DxUY42r_6Cg.
Some things are still a little unclear, but if you can explain that video in your own words you really need all you need to know.

This is one of those cases where you can definitely skip this section and simply use a Perlin noise or Simplex crate, or whatever you would want to use.

I'll go with OpenSimplex2S.
It seems that it is slightly slower than OpenSimplex2, but produces slightly nicer output.

https://github.com/KdotJPG/OpenSimplex2
https://lib.rs/crates/opensimplex2

Basic idea:
1. Place gradient vectors on a lattice.
2. Find nearby lattice points.
3. Compute gradient contributions.
4. Blend them smoothly.

```
sample position
 v
locate nearby lattice vertices
 v
generate pseudorandom gradients
 v
weight by distance
 v
sum contributions
 v
noise value
```

There's 2d, 3d, and 4d variants of the algorithm.
The 2d variant is the simplest to start with, and we can use it to determine the height of the world.
Later, once we generate caves, we can use the 3d variant.
We likely won't need the 4d variant; its fourth dimension is typically used for time-varying effect, like cloud movement, or animations and procedural textures.

There's very little explanation of the actual algorithm, so we'll have to make do with the source code and descriptions of the Simplex and Perlin algorithms.
https://en.wikipedia.org/wiki/Simplex_noise
Ultimately, they all follow a similar approach, so if we understand Simplex/Perlin we should be able to map those ideas to OpenSimplex2S.

---

1. Transform/skew input coordinates. This essentially transforms the grid into a honeycomb pattern, making the distance between points equal.
2. The values in the coordinate (x,y,z) are sorted in increasing order, e.g. (y,x,z). Then, ...?
3. The vertex is hashed to determine the gradient.
4. The contribution of each vertex of the simplex is combined. The input coordinate is subtracted to unskew the vector. The dot product then gives the gradient value. d^2 gives the squared distance to the point.

## Testing the noise

To test whether the noise function is correct, I've created a test file `examples/noise.rs`.
We also need the `image` crate: `cargo add --dev image`.
I deliberately use the `examples` folder.
I don't put it in the main sources, because I don't want to ship the `image` crate with the game.
The `tests` folder is not suitable, because it is intended for many unit and integration tests run in parallel, not for visualizing an output.

When our editor becomes more capable, this visualization can also be moved into the game.
That would allow us to play around with settings and visualize the results directly.

Turns out, there's a subtle difference in C# and Rust.
In C#, the default overflow behaviour is to silently wrap the value.
In the case of Rust, overflows will always cause a panic.
Thus, we replace potentially overflowing and underflowing integer operations with `wrapping_add` etc.

## Benchmarking the noise

Another thing we can do is to benchmark the performance of the noise function.
With the `criterion` crate we can conveniently test whether changes to the noise function have a negative impact on performance.
(Though, always be aware that what works on your system might not translate, so don't waste time optimizing for few percentage changes.)

## GPU implementation

Is this hard with Bevy?
Would be cool.

## Terrain generation

We can now use this `noise2` function, or whatever 2d noise function you have from your selected crate, to determine the height of each x-y coordinate in the world.
Thus, for every world position x and y, `noise2(x, y)` gives the `z` height at that position in the world.

We also add another voxel type: stone.
The topmost voxel, at z, becomes grass, the three below it become dirt, and all others below it become stone.

We also need to make sure to spawn the player at the correct height, so they don't appear in the ground.
In the future we might also need to check, for example, if the player does not appear inside a structure like a tree or a building, or does not spawn right above a ravine or cave.
For now, we can just use the heightmap data.

## `NoiseSettings` and `TerrainConfig`

`NoiseSettings::area2` samples a chunk-sized heightmap in world coordinates, with
`x + z * CHUNK_WIDTH` indexing. Chunk generation consumes the batch, while a
single-position sample remains useful for finding the player spawn height.
