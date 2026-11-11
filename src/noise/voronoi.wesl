struct VoronoiResult {
    // Distance to the nearest feature point (squared for `voronoi_3d`, to the nearest border for `voronoi_precise_3d`)
    value: f32,
    // Random value of the nearest cell
    cell: f32,
}

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

fn voronoi_random_vector(uv: vec3<f32>, offset: f32) -> vec3<f32> {
    // WGSL matrices are column-major, so this has the same columns as the HLSL rows,
    // and `m * uv` equals the HLSL `mul(uv, m)`.
    let m = mat3x3<f32>(
        15.27, 47.63, 99.41,
        89.98, 95.07, 38.39,
        33.83, 51.06, 60.77,
    );
    let hashed = fract(sin(m * uv) * 46839.32);
    return vec3<f32>(
        sin(hashed.y * offset) * 0.5 + 0.5,
        cos(hashed.x * offset) * 0.5 + 0.5,
        sin(hashed.z * offset) * 0.5 + 0.5,
    );
}

// Distance to the cell borders, instead of to the feature points, by also checking the cells around the nearest one.
fn voronoi_precise_3d(uv: vec3<f32>, angle_offset: f32, cell_density: f32) -> VoronoiResult {
    let g = floor(uv * cell_density);
    let f = fract(uv * cell_density);
    var res = vec2<f32>(8.0, 8.0);
    var ml = vec3<f32>(0.0);
    var mv = vec3<f32>(0.0);

    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            for (var z = -1; z <= 1; z++) {
                let lattice = vec3<f32>(f32(x), f32(y), f32(z));
                let offset = voronoi_random_vector(g + lattice, angle_offset);
                let v = lattice + offset - f;
                let d = dot(v, v);

                if (d < res.x) {
                    res.x = d;
                    res.y = offset.x;
                    mv = v;
                    ml = lattice;
                }
            }
        }
    }

    let cell = res.y;

    res = vec2<f32>(8.0, 8.0);
    for (var y = -2; y <= 2; y++) {
        for (var x = -2; x <= 2; x++) {
            for (var z = -2; z <= 2; z++) {
                let lattice = ml + vec3<f32>(f32(x), f32(y), f32(z));
                let offset = voronoi_random_vector(g + lattice, angle_offset);
                let v = lattice + offset - f;

                let cell_difference = abs(ml - lattice);
                if (cell_difference.x + cell_difference.y + cell_difference.z > 0.1) {
                    let d = dot(0.5 * (mv + v), normalize(v - mv));
                    res.x = min(res.x, d);
                }
            }
        }
    }

    return VoronoiResult(res.x, cell);
}

fn voronoi_3d(uv: vec3<f32>, angle_offset: f32, cell_density: f32) -> VoronoiResult {
    let g = floor(uv * cell_density);
    let f = fract(uv * cell_density);
    var res = vec3<f32>(8.0, 8.0, 8.0);

    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            for (var z = -1; z <= 1; z++) {
                let lattice = vec3<f32>(f32(x), f32(y), f32(z));
                let offset = voronoi_random_vector(g + lattice, angle_offset);
                let v = lattice + offset - f;
                let d = dot(v, v);

                if (d < res.x) {
                    res.y = res.x;
                    res.x = d;
                    res.z = offset.x;
                } else if (d < res.y) {
                    res.y = d;
                }
            }
        }
    }

    return VoronoiResult(res.x, res.z);
}
