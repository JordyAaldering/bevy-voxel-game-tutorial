//! Seeded Voronoi noise with one pseudorandom feature point per cell.

const PRIME_X: u32 = 0x9270_c86f;
const PRIME_Y: u32 = 0x0038_17b5;
const PRIME_Z: u32 = 0x9fa0_bacb;

fn mix(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

fn hash(seed: i32, x: i32, y: i32, z: i32) -> u32 {
    mix((seed as u32)
        ^ (x as u32).wrapping_mul(PRIME_X)
        ^ (y as u32).wrapping_mul(PRIME_Y)
        ^ (z as u32).wrapping_mul(PRIME_Z))
}

fn unit(hash: u32, salt: u32) -> f32 {
    (mix(hash ^ salt) >> 8) as f32 * (1.0 / 16_777_216.0)
}

/// 2D Voronoi noise in `[-1, 1]`.
pub fn noise2(seed: i32, x: f32, z: f32) -> f32 {
    let xb = x.floor() as i32;
    let zb = z.floor() as i32;
    let xf = x - xb as f32;
    let zf = z - zb as f32;
    let mut nearest = f32::INFINITY;
    for dz in -1i32..=1 {
        for dx in -1i32..=1 {
            let h = hash(seed, xb.wrapping_add(dx), zb.wrapping_add(dz), 0);
            let px = dx as f32 + unit(h, 0) - xf;
            let pz = dz as f32 + unit(h, 1) - zf;
            nearest = nearest.min(px * px + pz * pz);
        }
    }
    nearest.sqrt().min(1.0) * 2.0 - 1.0
}

/// 3D Voronoi noise in `[-1, 1]`.
pub fn noise3(seed: i32, x: f32, y: f32, z: f32) -> f32 {
    let xb = x.floor() as i32;
    let yb = y.floor() as i32;
    let zb = z.floor() as i32;
    let xf = x - xb as f32;
    let yf = y - yb as f32;
    let zf = z - zb as f32;
    let mut nearest = f32::INFINITY;
    for dz in -1i32..=1 {
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let h = hash(seed, xb.wrapping_add(dx), yb.wrapping_add(dy), zb.wrapping_add(dz));
                let px = dx as f32 + unit(h, 0) - xf;
                let py = dy as f32 + unit(h, 1) - yf;
                let pz = dz as f32 + unit(h, 2) - zf;
                nearest = nearest.min(px * px + py * py + pz * pz);
            }
        }
    }
    nearest.sqrt().min(1.0) * 2.0 - 1.0
}
