fn simplex_gradient(seed: u32, lattice: u32, base: vec3<i32>, corner: vec3<i32>, delta: vec3<f32>) -> f32 {
    let c = bitcast<vec3<u32>>(base + corner);
    var hash = seed ^ select(0u, 0x169129d7u, lattice == 1u)
        ^ (c.x * 0x9270c86fu) ^ (c.y * 0x003817b5u) ^ (c.z * 0x9fa0bacbu);
    hash *= 0xeec546f5u;
    hash ^= hash >> 26u;
    let index = 11u + (hash & 1020u);
    return bitcast<f32>(params[index]) * delta.x
        + bitcast<f32>(params[index + 1u]) * delta.y
        + bitcast<f32>(params[index + 2u]) * delta.z;
}
fn simplex_noise3(seed: i32, p: vec3<f32>) -> f32 {
    let xz = p.x + p.z;
    let s2 = xz * -0.211324865405187;
    let yy = p.y * 0.577350269189626;
    let rotated = vec3<f32>(p.x + s2 + yy, xz * -0.577350269189626 + yy, p.z + s2 + yy);
    let base = vec3<i32>(floor(rotated));
    let fraction = rotated - vec3<f32>(base);
    let nearest = select(vec3<i32>(0), vec3<i32>(1), fraction >= vec3<f32>(0.5));
    let alternate = select(vec3<i32>(0), vec3<i32>(2), fraction >= vec3<f32>(0.5));
    let delta0 = fraction - vec3<f32>(nearest);
    let a0 = 0.75 - delta0.x * delta0.x - delta0.y * delta0.y - delta0.z * delta0.z;
    let delta1 = fraction - vec3<f32>(0.5);
    let a1 = 0.75 - delta1.x * delta1.x - delta1.y * delta1.y - delta1.z * delta1.z;
    let direction = select(vec3<f32>(1.0), vec3<f32>(-1.0), nearest == vec3<i32>(1));
    let flip0 = direction * 2.0 * delta1;
    let flip1 = -2.0 * direction * delta1 - vec3<f32>(1.0);
    let first = vec3<bool>(a0 + flip0.x > 0.0, a0 + flip0.y > 0.0, a0 + flip0.z > 0.0);
    let second = vec3<bool>(
        !first.x && a1 + flip1.x > 0.0,
        !first.y && a1 + flip1.y > 0.0,
        !first.z && a1 + flip1.z > 0.0,
    );
    var value = 0.0;
    for (var lattice = 0u; lattice < 2u; lattice++) {
        for (var z = 0; z <= 2; z++) {
            for (var y = 0; y <= 2; y++) {
                for (var x = 0; x <= 2; x++) {
                    if lattice == 0u && (x == 2 || y == 2 || z == 2) { continue; }
                    let corner = vec3<i32>(x, y, z);
                    if lattice == 0u {
                        let flip = corner != nearest;
                        let n = select(0u, 1u, flip.x) + select(0u, 1u, flip.y) + select(0u, 1u, flip.z);
                        if n == 3u || (n == 2u && ((!flip.x && first.x) || (!flip.y && first.y) || (!flip.z && first.z))) {
                            continue;
                        }
                    } else {
                        if (x != 1 && x != alternate.x) || (y != 1 && y != alternate.y) || (z != 1 && z != alternate.z) { continue; }
                        let flip = corner != vec3<i32>(1);
                        let n = select(0u, 1u, flip.x) + select(0u, 1u, flip.y) + select(0u, 1u, flip.z);
                        if n == 3u || (n == 1u && ((flip.x && !second.x) || (flip.y && !second.y) || (flip.z && !second.z)))
                            || (n == 2u && ((!flip.x && second.x) || (!flip.y && second.y) || (!flip.z && second.z))) {
                            continue;
                        }
                    }
                    let delta = fraction - vec3<f32>(corner) + f32(lattice) * 0.5;
                    let a = 0.75 - delta.x * delta.x - delta.y * delta.y - delta.z * delta.z;
                    if a > 0.0 {
                        let gradient = simplex_gradient(bitcast<u32>(seed), lattice, base, corner, delta);
                        value += (a * a) * (a * a) * gradient;
                    }
                }
            }
        }
    }
    return value;
}
