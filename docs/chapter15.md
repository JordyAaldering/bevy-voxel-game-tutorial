# Scaling voxel types

We only have grass, dirt, and stone, so add new voxel types for logs and leaves.
My spritesheet does not have a leaf texture, so I need to replace it.
Rather than creating another spritesheet, now is a good time to actually split the spritesheet into separate images.

_But I thought that was bad for performance?_

Yes, but we have a secret weapon: [`TextureAtlasBuilder`](https://docs.rs/bevy/latest/bevy/prelude/struct.TextureAtlasBuilder.html).
We already used the `TextureAtlas` before with the entire spritesheet, but we can actually build the atlas dynamically from separate images.
It's a bit more work now, but makes it much easier to add new voxel types later, whilst not giving up any performance.

Another problem is that leaves might be see-through.
We'll ignore that for now, and just pick a texture that is not transparent.
Solving this is actually quite tricky, and needs some shader work, which probably will deserve its own chapter.

## Steps performed

1. **Split the spritesheet.** The separate images live in `assets/voxels/`, and `spritesheet.png` is no longer used.
2. **Add the voxel types.** `VoxelType` gets `Log` and `Leaves`, appended at the end so existing saved chunks stay valid.
   It must remain a field-less enum, as every voxel in every chunk is stored as one, and fields would blow up chunk memory.
   It also derives `Hash` and `Deserialize` (with `rename_all = "lowercase"`), so the JSON keys map directly onto the variants.
   `VoxelType::ALL` lists all variants, and `voxel as usize` is used as a table index.
3. **Describe the faces in JSON.** A single `assets/voxels/voxels.json` maps each voxel type to its images:
   ```json
   "grass": { "top": "grass_top", "bottom": "dirt", "sides": "grass_side" },
   "dirt":  { "all": "dirt" }
   ```
   Keys are `all`, `top`, `bottom`, `sides`, `front`, `back`, `left`, `right`.
   Values are PNG names without extension.
   (Good thing we just implemented serialization...)

   Some blocks have alternative looks, such as the two kinds of diamond ore.
   Instead of a face description, such a voxel lists `variants`, each being a regular face description:
   ```json
   "stonediamond": { "variants": [ { "all": "stone_diamond" }, { "all": "stone_diamond_alt" } ] }
   ```
   Variants are a property of the texture, not of the voxel type: `StoneDiamond` stays a single `VoxelType`, so the chunk format and gameplay logic are unaffected.
   Which variant a voxel shows is a hash of its world position (`variant_seed`) modulo the number of variants.
   This is deterministic, so a voxel keeps its look when its chunk is remeshed or reloaded, and nothing has to be saved.
   All faces of one voxel use the same variant, so a block never mixes looks.
4. **Parse it with serde.** `FaceSpec` has one `Option<String>` per key (`deny_unknown_fields`, so typos are errors).
   `FaceSpec::resolve(direction)` picks the most specific entry: the individual face, then `top`/`bottom` or `sides`, then `all`.
   The file is read as `HashMap<VoxelType, VoxelSpec>`, where the untagged enum `VoxelSpec` is either a single `FaceSpec` or a list of `variants`.
5. **Resolve and collect names.** For every solid voxel, each of its variants, and each of the six directions, resolve the image name.
   Missing entries or faces are reported as errors.
   Unique names are collected in a `BTreeSet`, which gives a deterministic order.
6. **Load the images.** Each PNG is decoded with `Image::from_buffer` (sRGB, `ImageSampler::nearest()`) and added to `Assets<Image>`.
   This is synchronous at startup, so the atlas is ready before any chunk is meshed.
7. **Pack the atlas.** Add every image to a `TextureAtlasBuilder` and call `build()`.
   This returns the `TextureAtlasLayout`, a `TextureAtlasSources` and the combined `Image`.
8. **Build the lookup table.** `TextureAtlasSources::texture_index` maps each image handle to its index in the layout.
   `VoxelAtlas { layout, faces }` stores `faces: Vec<Vec<[usize; 6]>>`, indexed by voxel type, variant, and direction.
   `VoxelAtlas::texture(voxel, direction, variant_seed)` replaces the old hardcoded `VoxelType::texture`, and the `ATLAS_*` constants are removed.
9. **Use it.** `setup_chunk_material` calls `build_voxel_atlas`, uses the resulting image as the material's `base_color_texture`, and inserts `VoxelAtlas` as a resource.
   `build_mesh` now takes `&VoxelAtlas`, and passes `atlas.layout` to `MeshData` for the UV calculation.
