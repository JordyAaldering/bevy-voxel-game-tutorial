//!
//! Voronoi (cellular) noise: the distance to the nearest of a set of feature points,
//! with one pseudo-random point per unit grid cell.
//!

const PRIME_X: i64 = 0x5205402B9270C86F;
const PRIME_Y: i64 = 0x598CD327003817B5;
const PRIME_Z: i64 = 0x5BCC226E9FA0BACB;
const HASH_MULTIPLIER: i64 = 0x53A3F72DEEC546F5;

/// 2D Voronoi noise in `[-1, 1]`: -1 at a feature point, rising towards 1 at cell borders.
pub fn noise2(seed: i64, x: f64, z: f64) -> f32 {
    let xb = x.floor() as i64;
    let yb = z.floor() as i64;

    // The nearest point is always in the 3x3 cells around the sample, because every cell holds a point
    let mut nearest = f64::INFINITY;
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (cx, cy) = (xb + dx, yb + dz);
            let hash = hash(seed, &[cx.wrapping_mul(PRIME_X), cy.wrapping_mul(PRIME_Y)]);
            let px = cx as f64 + unit(hash);
            let py = cy as f64 + unit(hash >> 24);
            nearest = nearest.min((px - x) * (px - x) + (py - z) * (py - z));
        }
    }

    distance_to_noise(nearest.sqrt())
}

/// 3D Voronoi noise in `[-1, 1]`: -1 at a feature point, rising towards 1 at cell borders.
pub fn noise3(seed: i64, x: f64, y: f64, z: f64) -> f32 {
    distance_to_noise(cell3(seed, x, y, z).distance)
}

/// The feature point closest to a position.
pub struct Cell {
    /// Distance to the feature point, in cells.
    pub distance: f64,
    /// Hash of the cell the feature point is in, for deriving random properties of that cell.
    #[allow(unused)]
    pub hash: i64,
}

/// Finds the nearest feature point in 3D. Cells are one unit wide.
pub fn cell3(seed: i64, x: f64, y: f64, z: f64) -> Cell {
    let xb = x.floor() as i64;
    let yb = y.floor() as i64;
    let zb = z.floor() as i64;

    let mut nearest = Cell { distance: f64::INFINITY, hash: 0 };
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (cx, cy, cz) = (xb + dx, yb + dy, zb + dz);
                let hash = hash(seed, &[cx.wrapping_mul(PRIME_X), cy.wrapping_mul(PRIME_Y), cz.wrapping_mul(PRIME_Z)]);
                let px = cx as f64 + unit(hash);
                let py = cy as f64 + unit(hash >> 16);
                let pz = cz as f64 + unit(hash >> 32);
                let distance = ((px - x) * (px - x) + (py - y) * (py - y) + (pz - z) * (pz - z)).sqrt();
                if distance < nearest.distance {
                    nearest = Cell { distance, hash };
                }
            }
        }
    }
    nearest
}

/// Nearest-point distances are mostly below 1, so clamp there to fill `[-1, 1]`.
fn distance_to_noise(distance: f64) -> f32 {
    (distance.min(1.0) * 2.0 - 1.0) as f32
}

fn hash(seed: i64, primed: &[i64]) -> i64 {
    let mut hash = seed;
    for &p in primed {
        hash ^= p;
    }
    hash = hash.wrapping_mul(HASH_MULTIPLIER);
    hash ^= hash >> 32;
    hash = hash.wrapping_mul(HASH_MULTIPLIER);
    hash ^ (hash >> 29)
}

/// Uses 24 bits of the hash to give a value in `[0, 1)`.
fn unit(hash: i64) -> f64 {
    (hash & 0xFF_FFFF) as f64 / (1 << 24) as f64
}
