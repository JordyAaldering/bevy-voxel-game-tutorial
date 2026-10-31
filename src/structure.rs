use crate::{VoxelType, aabb::Aabb, coords::{Direction, WorldCoord}};

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

impl Structure {
    /// Bounding box of this structure.
    pub fn aabb(&self) -> Aabb {
        let mut min = WorldCoord::ZERO;
        let mut max = WorldCoord::ZERO;

        for voxel in &self.voxels {
            min = min.min(voxel.pos);
            max = max.max(voxel.pos);
        }

        Aabb { min, max }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct StructureVoxel {
    /// Offset relative to the structure origin (can be negative).
    pub pos: WorldCoord,
    pub voxel: VoxelType,
    /// Orientation, for voxels that have one (e.g. logs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    /// Probability of this voxel being placed, between 0.0 and 1.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probability: Option<f32>,
}
