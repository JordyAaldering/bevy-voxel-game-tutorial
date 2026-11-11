fn voronoi_mix(value: u32) -> u32 {
    var h = value;
    h = h ^ (h >> 16u);
    h = h * 0x7feb352du;
    h = h ^ (h >> 15u);
    h = h * 0x846ca68bu;
    return h ^ (h >> 16u);
}
fn voronoi_hash(seed: i32, cell: vec3<i32>) -> u32 {
    return voronoi_mix(bitcast<u32>(seed)
        ^ (bitcast<u32>(cell.x) * 0x9270c86fu)
        ^ (bitcast<u32>(cell.y) * 0x003817b5u)
        ^ (bitcast<u32>(cell.z) * 0x9fa0bacbu));
}

fn voronoi_unit(hash: u32, salt: u32) -> f32 {
    return f32(voronoi_mix(hash ^ salt) >> 8u) * (1.0 / 16777216.0);
}

/// Seeded 2D Voronoi noise in [-1, 1], matching the Rust noise2 function.
fn voronoi_noise2(seed: i32, position: vec2<f32>) -> f32 {
    let base = vec2<i32>(floor(position));
    let fraction = position - vec2<f32>(base);
    var nearest = 3.402823e38;
    for (var dz = -1; dz <= 1; dz++) {
        for (var dx = -1; dx <= 1; dx++) {
            let offset = vec2<i32>(dx, dz);
            let h = voronoi_hash(seed, vec3<i32>(base + offset, 0));
            let feature = vec2<f32>(voronoi_unit(h, 0u), voronoi_unit(h, 1u));
            let delta = vec2<f32>(offset) + feature - fraction;
            nearest = min(nearest, delta.x * delta.x + delta.y * delta.y);
        }
    }
    return min(sqrt(nearest), 1.0) * 2.0 - 1.0;
}

/// Seeded 3D Voronoi noise in [-1, 1], matching the Rust noise3 function.
fn voronoi_noise3(seed: i32, position: vec3<f32>) -> f32 {
    let base = vec3<i32>(floor(position));
    let fraction = position - vec3<f32>(base);
    var nearest = 3.402823e38;
    for (var dz = -1; dz <= 1; dz++) {
        for (var dy = -1; dy <= 1; dy++) {
            for (var dx = -1; dx <= 1; dx++) {
                let offset = vec3<i32>(dx, dy, dz);
                let h = voronoi_hash(seed, base + offset);
                let feature = vec3<f32>(
                    voronoi_unit(h, 0u),
                    voronoi_unit(h, 1u),
                    voronoi_unit(h, 2u),
                );
                let delta = vec3<f32>(offset) + feature - fraction;
                nearest = min(nearest, delta.x * delta.x + delta.y * delta.y + delta.z * delta.z);
            }
        }
    }
    return min(sqrt(nearest), 1.0) * 2.0 - 1.0;
}
