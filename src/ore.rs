use crate::{TerrainType, noise::{NoiseSettings, NoiseType}};

use serde::{Deserialize, Serialize};

/// Voronoi cells are this many times wider than the largest cluster radius.
pub(crate) const CELL_SIZE_FACTOR: f32 = 4.0;
/// A type of ore, which spawns in clusters inside stone.
///
/// Clusters are placed with Voronoi noise: every cell has one feature point, which is the center
/// of a cluster. `rarity` controls the threshold and hence the cluster radius.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ore {
    pub name: String,
    /// The voxel that replaces stone.
    pub voxel: TerrainType,
    /// Lowest world height (y) the ore is found at, inclusive.
    pub min_height: i32,
    /// Highest world height (y) the ore is found at, inclusive.
    pub max_height: i32,
    /// Approximately scales cluster volume, between 0.0 and 1.0.
    pub rarity: f32,
    /// Maximum radius of clusters, in blocks.
    pub cluster_size: f32,
}

impl Ore {
    pub(crate) fn noise_settings(&self) -> NoiseSettings {
        NoiseSettings {
            noise_type: NoiseType::Voronoi,
            frequency: if self.cluster_size > 0.0 { 1.0 / (self.cluster_size * CELL_SIZE_FACTOR) } else { 0.0 },
            ..Default::default()
        }
    }

    pub(crate) fn contains_noise(&self, y: i32, value: f32) -> bool {
        y >= self.min_height && y <= self.max_height && self.cluster_size > 0.0
            && self.rarity > 0.0 && value <= -1.0 + 0.5 * self.rarity.clamp(0.0, 1.0).cbrt()
    }
}
