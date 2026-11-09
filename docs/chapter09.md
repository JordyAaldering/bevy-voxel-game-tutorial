# Terrain generation

So far, our chunks have been filled with simple patterns.
Now we'll use noise to generate a continuous landscape across the world.
The same world coordinates must always produce the same result, so the terrain stays consistent when neighboring chunks are generated at different times.

## Noise algorithms

Noise functions turn coordinates into smoothly changing values.
They are useful for terrain because nearby positions receive similar values, unlike purely random values, which would produce abrupt changes from one block to the next.

Perlin noise is a familiar choice, but we'll use OpenSimplex2S, the smooth variant of OpenSimplex2.
The [OpenSimplex2 source](https://github.com/KdotJPG/OpenSimplex2) and the [OpenSimplex2S reference implementation](https://github.com/KdotJPG/OpenSimplex2/blob/master/java/OpenSimplex2S.java) are useful references.
You can also use an existing noise crate instead of writing or adapting an implementation yourself.

At a high level, OpenSimplex2S works like this:

1. Transform the sample coordinates into a skewed lattice space.
   This lets the algorithm use a simplex-shaped tiling rather than the square grid of the input coordinates.
2. Find the nearby lattice vertices that can contribute to the sample.
3. Use the seed and each vertex's lattice coordinate to choose a repeatable gradient direction.
4. For each vertex, compare its gradient with the vector from that vertex to the sample.
   Fade contributions smoothly with distance so distant vertices have no effect.
5. Add the contributions together to get the noise value.

The result is deterministic for a given seed and position, and changes smoothly as the position moves.
The 2D form is enough for a heightmap: sample each horizontal x/z position to get a height.
The 3D form can later be used for volumetric features such as caves.
A 4D form is often useful when a fourth coordinate, such as time, is needed for animation, but we do not need it for this terrain.

If you adapt an existing implementation, focus on the dimensions and sampling interface the game needs rather than rewriting the algorithm from scratch.
In Rust, pay attention to integer overflow in the hash: where the source relies on wraparound, use wrapping arithmetic rather than ordinary operations that can panic in debug builds.

## Visualizing noise

Before using a noise function to generate a world, it helps to see what it produces.
The `examples/noise.rs` example samples a 2D grid, maps the noise range from `[-1, 1]` to grayscale values from `[0, 255]`, and saves the result as an image.
Run it with `cargo run --example noise` and inspect `noise.png`.

Keeping this visualization in `examples` makes it a standalone development tool rather than part of the game.
It can quickly reveal problems such as a flat result, unexpected repetition, or features at the wrong scale.
If the noise code changes, regenerate the image and compare it before wiring the result into terrain.

## Testing the noise

The image is useful for visual inspection, but it does not replace automated tests.
Add tests for important properties of the noise function, such as deterministic output for a fixed seed and position, and different output when the seed changes.
Also test the settings and chunk-sampling helpers so their world-coordinate mapping agrees across chunk boundaries.

## Tuning noise

A single noise layer gives broad, smooth shapes.
To add finer details, sample additional layers called _octaves_.
Each octave uses a higher frequency and a lower amplitude than the previous one; combining them adds smaller features on top of the larger ones.

The terrain settings control how the noise is sampled and scaled:

- `frequency` controls the size of the features.
Lower frequency stretches the pattern over a larger area; higher frequency makes features smaller and more frequent.
- `octaves` controls how many noise layers are combined.
- `persistence` is the amplitude multiplier for each successive octave.
- `lacunarity` is the frequency multiplier for each successive octave.
- `amplitude` scales the combined noise into a height variation.
- `bias` shifts that variation up or down to set the approximate base height.
- `offset` moves the sampled pattern without changing its shape.

Adding more octaves costs more to evaluate, so start with a small number and add them only if the terrain benefits from the extra detail.

## Benchmarking the noise

When the noise function is in place, use Criterion to measure its performance and catch regressions when changing the implementation or adding octaves.
Benchmark representative workloads, such as sampling a chunk-sized heightmap, rather than relying only on a single noise call.
Treat small differences cautiously: benchmark results can vary between machines and may not reflect the cost in the full game.

## GPU implementation

Noise generation is a perfect candidate for GPU implementation.
It parallelizes very well, and is completely separate from the game logic.
However, executing GPU code is not trivial, so we'll save this for a later chapter, when performance becomes a concern.

## Terrain generation

Use the 2D noise as a heightmap.
For every horizontal world position `(x, z)`, sample the noise to get the height of that column.
It is important to use world coordinates rather than coordinates local to the chunk: otherwise, every chunk would start the same pattern again at its own origin, and the terrain would not line up at chunk boundaries.

The `NoiseSettings::area2` helper samples every x/z position in a chunk and returns the results in row-major order.
The index is `x + z * CHUNK_WIDTH`, matching the order used to fill the heightmap.
It converts the chunk coordinate to a world-space origin before sampling, so adjacent chunks sample adjacent positions.
A single-position sample is also useful for queries such as determining the player's spawn height.

Put the seed and height settings in a `TerrainConfig` resource.
A seed lets you reproduce a terrain layout while still making other worlds possible by changing one value.
Initialize the resource when building the app and pass it to both chunk generation and player spawning.
For example, the default settings use a base height near 20 blocks and a height variation of about 10 blocks; frequency determines how broadly those hills spread across the world.

For each sampled column, convert the noise value to a valid voxel height and clamp it to the chunk's vertical bounds.
Set the top voxel to grass, the two layers beneath it to dirt, and the remaining voxels below to stone.
This gives us a simple material profile while keeping the surface shape controlled by noise.
Add `Stone` to `VoxelType` and select its atlas tile in the texture lookup function.

Finally, spawn the player above the terrain rather than at a fixed height.
Sample the same height settings at the player's starting x/z position and place the player's feet one block above that surface.
Use the same seed and settings as chunk generation so the player position matches the terrain that will be generated beneath them.
