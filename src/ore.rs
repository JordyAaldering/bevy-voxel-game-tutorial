use crate::{VoxelType, noise::voronoi};

use serde::{Deserialize, Serialize};

/// Voronoi cells are this many times wider than the largest cluster radius, so clusters
/// stay well apart and never get cut off by the edge of their cell.
const CELL_SIZE_FACTOR: f32 = 4.0;
/// The smallest a cluster gets, as a fraction of `Ore::cluster_size`.
const MIN_CLUSTER_FRACTION: f32 = 0.5;

const RADIUS_SALT: u32 = 0x5241_4449;
const RARITY_SALT: u32 = 0x5241_5249;

/// A type of ore, which spawns in clusters inside stone.
///
/// Clusters are placed with Voronoi noise: every cell has one feature point, which is the center
/// of a cluster if the cell is picked by `rarity`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ore {
    pub name: String,
    /// The voxel that replaces stone.
    pub voxel: VoxelType,
    /// Lowest world height (y) the ore is found at, inclusive.
    pub min_height: i32,
    /// Highest world height (y) the ore is found at, inclusive.
    pub max_height: i32,
    /// Probability that a Voronoi cell holds a cluster, between 0.0 and 1.0.
    /// A cell is about `4 * cluster_size` blocks wide, so smaller clusters need a lower rarity
    /// to give the same amount of ore in total.
    pub rarity: f32,
    /// Radius of the largest clusters, in blocks. Each cluster is between half this and the full size.
    pub cluster_size: f32,
}

impl Ore {
    /// Whether this ore occupies the voxel at the given world position.
    /// A pure function of the position and seed, so chunks generate independently.
    /// `seed` should differ per ore, so that clusters of different ores do not coincide.
    pub fn contains(&self, seed: i32, x: i32, y: i32, z: i32) -> bool {
        if y < self.min_height || y > self.max_height || self.cluster_size <= 0.0 {
            return false;
        }

        let cell_size = self.cluster_size * CELL_SIZE_FACTOR;
        let cell = voronoi::cell3(seed, x as f32 / cell_size, y as f32 / cell_size, z as f32 / cell_size);

        if unit(cell.hash, RARITY_SALT) >= self.rarity {
            return false;
        }

        let fraction = MIN_CLUSTER_FRACTION + (1.0 - MIN_CLUSTER_FRACTION) * unit(cell.hash, RADIUS_SALT);
        cell.distance * cell_size <= self.cluster_size * fraction
    }
}

/// Value in `[0, 1)` derived from a cell hash. The salt makes properties of one cell independent.
fn unit(hash: u32, salt: u32) -> f32 {
    let mut h = hash ^ salt.wrapping_mul(0x9e37_79b9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h >> 8) as f32 / (1u32 << 24) as f32
}
