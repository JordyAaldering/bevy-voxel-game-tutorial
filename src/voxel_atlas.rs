use crate::{coords::Direction, BlockType, TerrainType, VoxelType};

use std::{collections::BTreeSet, fs, path::Path};

use better_default::Default;
use bevy::{
    prelude::*,
    asset::RenderAssetUsages,
    image::{CompressedImageFormats, ImageSampler, ImageType, TextureAtlasBuilder},
    platform::collections::HashMap,
};

const VOXELS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/voxels");

/// A voxel's inventory icon and either one set of faces or multiple variants.
#[derive(serde::Deserialize)]
struct VoxelSpec {
    icon: String,
    #[serde(flatten)]
    faces: VoxelFaces,
}

/// A voxel is either described by a single set of faces, or by a list of
/// alternative looks of which one is picked per voxel position.
#[derive(serde::Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum VoxelFaces {
    Faces(FaceSpec),
    Variants { variants: Vec<FaceSpec> },
}

impl VoxelFaces {
    fn variants(&self) -> &[FaceSpec] {
        match self {
            Self::Faces(spec) => std::slice::from_ref(spec),
            Self::Variants { variants } => variants,
        }
    }
}

#[derive(Resource)]
pub struct VoxelAtlasAssets {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
}

#[derive(Resource)]
pub struct VoxelAtlas {
    layout: TextureAtlasLayout,
    /// Atlas index per voxel type, used for inventory icons.
    icons: HashMap<VoxelType, usize>,
    /// Atlas index per voxel type, variant, and direction. Air is never rendered and has no variants.
    faces: HashMap<VoxelType, Vec<[usize; 6]>>,
}

impl VoxelAtlas {
    pub fn icon(&self, voxel: VoxelType) -> usize {
        *self.icons.get(&voxel).expect("voxel has no atlas icon")
    }

    /// `variant_seed` picks one of the voxel's variants; the same seed always gives the same variant.
    pub fn texture(&self, voxel: VoxelType, direction: Direction, variant_seed: u32) -> usize {
        let variants = self.faces.get(&voxel).expect("voxel has no atlas entry");
        variants[variant_seed as usize % variants.len()][direction as usize]
    }

    pub fn layout(&self) -> &TextureAtlasLayout {
        &self.layout
    }
}

/// Describes which image is used on each face of a voxel, as found in a category's
/// section of `voxels.json`.
/// More specific faces take precedence: `front`/`back`/`left`/`right`, then
/// `sides` (horizontal faces only) or `top`/`bottom`, then `all`.
#[derive(Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FaceSpec {
    all: Option<String>,
    top: Option<String>,
    bottom: Option<String>,
    sides: Option<String>,
    front: Option<String>,
    back: Option<String>,
    left: Option<String>,
    right: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct VoxelSpecs {
    blocks: HashMap<BlockType, VoxelSpec>,
    terrain: HashMap<TerrainType, VoxelSpec>,
}

impl FaceSpec {
    fn resolve(&self, direction: Direction) -> Option<&str> {
        match direction {
            Direction::Top    => self.top.as_ref(),
            Direction::Bottom => self.bottom.as_ref(),
            Direction::Front  => self.front.as_ref().or(self.sides.as_ref()),
            Direction::Back   => self.back.as_ref().or(self.sides.as_ref()),
            Direction::Left   => self.left.as_ref().or(self.sides.as_ref()),
            Direction::Right  => self.right.as_ref().or(self.sides.as_ref()),
        }
        .or(self.all.as_ref())
        .map(String::as_str)
    }
}

/// Builds the atlas from the images in `assets/voxels`, as described by `voxels.json`.
pub fn build_voxel_atlas(images: &mut Assets<Image>) -> Result<(VoxelAtlas, Image), String> {
    let json_path = Path::new(VOXELS_DIR).join("voxels.json");
    let json = fs::read_to_string(&json_path).map_err(|e| format!("{}: {e}", json_path.display()))?;
    let specs: VoxelSpecs = serde_json::from_str(&json)
        .map_err(|e| format!("{}: {e}", json_path.display()))?;

    // Resolve every face to an image name, remembering the unique names
    let mut names = BTreeSet::new();
    let mut resolved = Vec::new();
    let mut resolved_icons = Vec::new();

    for (voxel, spec) in BlockType::ALL.into_iter()
        .map(|block| (VoxelType::from_block(block), specs.blocks.get(&block)))
        .chain(TerrainType::ALL.into_iter().map(|terrain| {
            (VoxelType::from_terrain(terrain), specs.terrain.get(&terrain))
        }))
    {
        let Some(spec) = spec else {
            return Err(format!("no entry for {voxel:?} in voxels.json"));
        };

        names.insert(spec.icon.clone());
        resolved_icons.push((voxel, spec.icon.clone()));

        if spec.faces.variants().is_empty() {
            return Err(format!("{voxel:?} has an empty list of variants in voxels.json"));
        }

        for (variant, face_spec) in spec.faces.variants().iter().enumerate() {
            for direction in Direction::iter() {
                let name = face_spec.resolve(direction)
                    .ok_or_else(|| format!("{voxel:?} variant {variant} has no image for its {direction:?} face"))?;
                names.insert(name.to_owned());
                resolved.push((voxel, variant, direction, name.to_owned()));
            }
        }
    }

    let mut loaded = Vec::new();
    for name in &names {
        let path = Path::new(VOXELS_DIR).join(format!("{name}.png"));
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let image = Image::from_buffer(
            &bytes,
            ImageType::Extension("png"),
            CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            RenderAssetUsages::default(),
        ).map_err(|e| format!("{}: {e}", path.display()))?;
        loaded.push((name, images.add(image)));
    }

    let mut builder = TextureAtlasBuilder::default();
    for (_, handle) in &loaded {
        builder.add_texture(Some(handle.id()), images.get(handle).unwrap());
    }

    let (layout, sources, image) = builder.build().map_err(|e| format!("failed to pack atlas: {e}"))?;

    let indices: HashMap<&str, usize> = loaded.iter()
        .map(|(name, handle)| (name.as_str(), sources.texture_index(handle.id()).unwrap()))
        .collect();

    let icons = resolved_icons.into_iter()
        .map(|(voxel, name)| (voxel, indices[name.as_str()]))
        .collect();

    let mut faces = HashMap::default();
    for (voxel, variant, direction, name) in resolved {
        let variants = faces.entry(voxel).or_insert_with(Vec::new);
        if variants.len() <= variant {
            variants.resize(variant + 1, [0; 6]);
        }

        variants[variant][direction as usize] = indices[name.as_str()];
    }

    Ok((VoxelAtlas { layout, icons, faces }, image))
}
