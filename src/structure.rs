use crate::*;

use bevy::prelude::*;

/// Structures loaded from `assets/structures`.
#[derive(Resource)]
pub struct Structures {
    pub tree: Structure,
}

impl Structures {
    const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/structures");

    pub fn load() -> Result<Self, String> {
        Ok(Self {
            #[cfg(until19)]
            tree: Self::load_structure("tree_ch16")?,
            #[cfg(since20)]
            tree: Self::load_structure("tree")?,
        })
    }

    pub fn load_structure(name: &str) -> Result<Structure, String> {
        let path = Path::new(Self::DIR).join(format!("{name}.json"));
        let json = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&json).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Set a coordinate to air to require that space to be cleared by the structure,
/// otherwise if there happens to be any voxels already at that position it remains.
/// For trees, for example, we don't want to clear the space below it.
/// But we do want to clear the space inside a house.
/// A prefab placed relative to an origin voxel.
/// Structures must always be placed with their origin voxel at coordinate (0, 0, 0).
/// This means that the bounding box of the structure must contain the origin.
/// Voxel placement in the world is determined by the structure's origin.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Structure {
    /// Offset-from-origin voxels. Positions not listed are left untouched.
    pub voxels: Vec<StructureVoxel>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct StructureVoxel {
    /// Offset relative to the structure origin (can be negative).
    pub pos: WorldCoord,
    #[cfg(until19)]
    pub voxel: VoxelType,
    #[cfg(since20)]
    pub voxel: VoxelTypeItem,
    /// Orientation, for voxels that have one (e.g. logs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    /// Probability of this voxel being placed, between 0.0 and 1.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probability: Option<f32>,
}
