//!
//! K.jpg's OpenSimplex 2, smooth variant ("SuperSimplex")
//!

use std::{num::Wrapping, sync::LazyLock};

const PRIME_X: u32 = 0x9270C86F;
const PRIME_Y: u32 = 0x003817B5;
const PRIME_Z: u32 = 0x9FA0BACB;
const HASH_MULTIPLIER: u32 = 0xEEC546F5;
const SEED_FLIP_3D: u32 = 0x169129D7;

const SKEW_2D: f32 = 0.366025403784439;
const UNSKEW_2D: f32 = -0.21132486540518713;
const ROOT3OVER3: f32 = 0.577350269189626;

const N_GRADS_2D_EXPONENT: i32 = 7;
const N_GRADS_3D_EXPONENT: i32 = 8;
const N_GRADS_2D: i32 = 1 << N_GRADS_2D_EXPONENT;
const N_GRADS_3D: i32 = 1 << N_GRADS_3D_EXPONENT;

const NORMALIZER_2D: f32 = 0.05481866495625118;
const NORMALIZER_3D: f32 = 0.2781926117527186;

const RSQUARED_2D: f32 = 2.0 / 3.0;
const RSQUARED_3D: f32 = 3.0 / 4.0;

/// 2D OpenSimplex2S/SuperSimplex noise, standard lattice orientation.
#[allow(unused)]
pub fn noise2(seed: i32, x: f32, y: f32) -> f32 {
    // Get points for A2* lattice
    let s = SKEW_2D * (x + y);
    let xs = x + s;
    let ys = y + s;
    noise2_unskewed_base(seed, xs, ys)
}

/// 2D OpenSimplex2S/SuperSimplex noise base.
fn noise2_unskewed_base(seed: i32, xs: f32, ys: f32) -> f32 {
    let seed = Wrapping(seed as u32);

    // Get base points and offsets.
    let xsb = fast_floor(xs);
    let ysb = fast_floor(ys);
    let xi = (xs - xsb as f32) as f32;
    let yi = (ys - ysb as f32) as f32;

    // Prime pre-multiplication for hash.
    let xsbp = Wrapping(xsb as u32) * Wrapping(PRIME_X);
    let ysbp = Wrapping(ysb as u32) * Wrapping(PRIME_Y);

    // Unskew.
    let t = (xi + yi) * UNSKEW_2D as f32;
    let dx0 = xi + t;
    let dy0 = yi + t;

    // First vertex.
    let a0 = RSQUARED_2D - dx0 * dx0 - dy0 * dy0;
    let mut value = (a0 * a0) * (a0 * a0) * grad2(seed, xsbp, ysbp, dx0, dy0);

    // Second vertex.
    let a1 = (2.0 * (1.0 + 2.0 * UNSKEW_2D) * (1.0 / UNSKEW_2D + 2.0)) as f32 * t
        + ((-2.0 * (1.0 + 2.0 * UNSKEW_2D) * (1.0 + 2.0 * UNSKEW_2D)) as f32 + a0);
    let dx1 = dx0 - (1.0 + 2.0 * UNSKEW_2D) as f32;
    let dy1 = dy0 - (1.0 + 2.0 * UNSKEW_2D) as f32;
    value += (a1 * a1)
        * (a1 * a1)
        * grad2(
            seed,
            xsbp + Wrapping(PRIME_X),
            ysbp + Wrapping(PRIME_Y),
            dx1,
            dy1,
        );

    // Third and fourth vertices.
    // Nested conditionals were faster than compact bit logic/arithmetic.
    let xmyi = xi - yi;
    if t < UNSKEW_2D as f32 {
        if xi + xmyi > 1.0 {
            let dx2 = dx0 - (3.0 * UNSKEW_2D + 2.0) as f32;
            let dy2 = dy0 - (3.0 * UNSKEW_2D + 1.0) as f32;
            let a2 = RSQUARED_2D - dx2 * dx2 - dy2 * dy2;
            if a2 > 0.0 {
                value += (a2 * a2)
                    * (a2 * a2)
                    * grad2(
                        seed,
                        xsbp + Wrapping(PRIME_X << 1),
                        ysbp + Wrapping(PRIME_Y),
                        dx2,
                        dy2,
                    );
            }
        } else {
            let dx2 = dx0 - UNSKEW_2D as f32;
            let dy2 = dy0 - (UNSKEW_2D + 1.0) as f32;
            let a2 = RSQUARED_2D - dx2 * dx2 - dy2 * dy2;
            if a2 > 0.0 {
                value +=
                    (a2 * a2) * (a2 * a2) * grad2(seed, xsbp, ysbp + Wrapping(PRIME_Y), dx2, dy2);
            }
        }

        if yi - xmyi > 1.0 {
            let dx3 = dx0 - (3.0 * UNSKEW_2D + 1.0) as f32;
            let dy3 = dy0 - (3.0 * UNSKEW_2D + 2.0) as f32;
            let a3 = RSQUARED_2D - dx3 * dx3 - dy3 * dy3;
            if a3 > 0.0 {
                value += (a3 * a3)
                    * (a3 * a3)
                    * grad2(
                        seed,
                        xsbp + Wrapping(PRIME_X),
                        ysbp + Wrapping(PRIME_Y << 1),
                        dx3,
                        dy3,
                    );
            }
        } else {
            let dx3 = dx0 - (UNSKEW_2D + 1.0) as f32;
            let dy3 = dy0 - UNSKEW_2D as f32;
            let a3 = RSQUARED_2D - dx3 * dx3 - dy3 * dy3;
            if a3 > 0.0 {
                value +=
                    (a3 * a3) * (a3 * a3) * grad2(seed, xsbp + Wrapping(PRIME_X), ysbp, dx3, dy3);
            }
        }
    } else {
        if xi + xmyi < 0.0 {
            let dx2 = dx0 + (1.0 + UNSKEW_2D) as f32;
            let dy2 = dy0 + UNSKEW_2D as f32;
            let a2 = RSQUARED_2D - dx2 * dx2 - dy2 * dy2;
            if a2 > 0.0 {
                value +=
                    (a2 * a2) * (a2 * a2) * grad2(seed, xsbp - Wrapping(PRIME_X), ysbp, dx2, dy2);
            }
        } else {
            let dx2 = dx0 - (UNSKEW_2D + 1.0) as f32;
            let dy2 = dy0 - UNSKEW_2D as f32;
            let a2 = RSQUARED_2D - dx2 * dx2 - dy2 * dy2;
            if a2 > 0.0 {
                value +=
                    (a2 * a2) * (a2 * a2) * grad2(seed, xsbp + Wrapping(PRIME_X), ysbp, dx2, dy2);
            }
        }

        if yi < xmyi {
            let dx2 = dx0 + UNSKEW_2D as f32;
            let dy2 = dy0 + (UNSKEW_2D + 1.0) as f32;
            let a2 = RSQUARED_2D - dx2 * dx2 - dy2 * dy2;
            if a2 > 0.0 {
                value +=
                    (a2 * a2) * (a2 * a2) * grad2(seed, xsbp, ysbp - Wrapping(PRIME_Y), dx2, dy2);
            }
        } else {
            let dx2 = dx0 - UNSKEW_2D as f32;
            let dy2 = dy0 - (UNSKEW_2D + 1.0) as f32;
            let a2 = RSQUARED_2D - dx2 * dx2 - dy2 * dy2;
            if a2 > 0.0 {
                value +=
                    (a2 * a2) * (a2 * a2) * grad2(seed, xsbp, ysbp + Wrapping(PRIME_Y), dx2, dy2);
            }
        }
    }

    value
}

/// 3D OpenSimplex2S/SuperSimplex noise, with better visual isotropy in (X, Z).
/// Recommended for 3D terrain and time-varied animations.
/// The Y coordinate should always be the "different" coordinate in whatever your use case is.
/// If Y is vertical in world coordinates, call noise3_ImproveXZ(x, Y, z).
/// If Z is vertical in world coordinates, call noise3_ImproveXZ(x, Z, y) or use noise3_ImproveXY.
/// For a time varied animation, call noise3_ImproveXZ(x, T, y) or use noise3_ImproveXY.
#[allow(unused)]
pub fn noise3(seed: i32, x: f32, y: f32, z: f32) -> f32 {
    // Re-orient the cubic lattices without skewing, so Y points up the main lattice diagonal,
    // and the planes formed by XZ are moved far out of alignment with the cube faces.
    // Orthonormal rotation. Not a skew transform.
    let xz = x + z;
    let s2 = xz * -0.211324865405187;
    let yy = y * ROOT3OVER3;
    let xr = x + s2 + yy;
    let zr = z + s2 + yy;
    let yr = xz * -ROOT3OVER3 + yy;

    // Evaluate both lattices to form a BCC lattice.
    noise3_unrotated_base(seed, xr, yr, zr)
}

/// Generate overlapping cubic lattices for 3D Re-oriented BCC noise.
/// Lookup table implementation inspired by DigitalShadow.
/// It was actually faster to narrow down the points in the loop itself,
/// than to build up the index with enough info to isolate 8 points.
fn noise3_unrotated_base(seed: i32, xr: f32, yr: f32, zr: f32) -> f32 {
    let seed = Wrapping(seed as u32);

    // Get base points and offsets.
    let xrb = fast_floor(xr);
    let yrb = fast_floor(yr);
    let zrb = fast_floor(zr);
    let xi = (xr - xrb as f32) as f32;
    let yi = (yr - yrb as f32) as f32;
    let zi = (zr - zrb as f32) as f32;

    // Prime pre-multiplication for hash. Also flip seed for second lattice copy.
    let xrbp = Wrapping(xrb as u32) * Wrapping(PRIME_X);
    let yrbp = Wrapping(yrb as u32) * Wrapping(PRIME_Y);
    let zrbp = Wrapping(zrb as u32) * Wrapping(PRIME_Z);
    let seed2 = seed ^ Wrapping(SEED_FLIP_3D);

    // -1 if positive, 0 if negative.
    let xnmask = (-0.5 - xi) as i32;
    let ynmask = (-0.5 - yi) as i32;
    let znmask = (-0.5 - zi) as i32;

    // First vertex.
    let x0 = xi + xnmask as f32;
    let y0 = yi + ynmask as f32;
    let z0 = zi + znmask as f32;
    let a0 = RSQUARED_3D - x0 * x0 - y0 * y0 - z0 * z0;
    let mut value = (a0 * a0)
        * (a0 * a0)
        * grad3(
            seed,
            xrbp + (Wrapping(xnmask as u32) & Wrapping(PRIME_X)),
            yrbp + (Wrapping(ynmask as u32) & Wrapping(PRIME_Y)),
            zrbp + (Wrapping(znmask as u32) & Wrapping(PRIME_Z)),
            x0,
            y0,
            z0,
        );

    // Second vertex.
    let x1 = xi - 0.5;
    let y1 = yi - 0.5;
    let z1 = zi - 0.5;
    let a1 = RSQUARED_3D - x1 * x1 - y1 * y1 - z1 * z1;
    value += (a1 * a1)
        * (a1 * a1)
        * grad3(
            seed2,
            xrbp + Wrapping(PRIME_X),
            yrbp + Wrapping(PRIME_Y),
            zrbp + Wrapping(PRIME_Z),
            x1,
            y1,
            z1,
        );

    // Shortcuts for building the remaining falloffs.
    // Derived by subtracting the polynomials with the offsets plugged in.
    let xa_flip_mask0 = ((xnmask | 1) << 1) as f32 * x1;
    let ya_flip_mask0 = ((ynmask | 1) << 1) as f32 * y1;
    let za_flip_mask0 = ((znmask | 1) << 1) as f32 * z1;
    let xa_flip_mask1 = (-2 - (xnmask << 2)) as f32 * x1 - 1.0;
    let ya_flip_mask1 = (-2 - (ynmask << 2)) as f32 * y1 - 1.0;
    let za_flip_mask1 = (-2 - (znmask << 2)) as f32 * z1 - 1.0;

    let mut skip5 = false;
    let a2 = xa_flip_mask0 + a0;
    if a2 > 0.0 {
        let x2 = x0 - (xnmask | 1) as f32;
        let y2 = y0;
        let z2 = z0;
        value += (a2 * a2)
            * (a2 * a2)
            * grad3(
                seed,
                xrbp + (Wrapping(!xnmask as u32) & Wrapping(PRIME_X)),
                yrbp + (Wrapping(ynmask as u32) & Wrapping(PRIME_Y)),
                zrbp + (Wrapping(znmask as u32) & Wrapping(PRIME_Z)),
                x2,
                y2,
                z2,
            );
    } else {
        let a3 = ya_flip_mask0 + za_flip_mask0 + a0;
        if a3 > 0.0 {
            let x3 = x0;
            let y3 = y0 - (ynmask | 1) as f32;
            let z3 = z0 - (znmask | 1) as f32;
            value += (a3 * a3)
                * (a3 * a3)
                * grad3(
                    seed,
                    xrbp + (Wrapping(xnmask as u32) & Wrapping(PRIME_X)),
                    yrbp + (Wrapping(!ynmask as u32) & Wrapping(PRIME_Y)),
                    zrbp + (Wrapping(!znmask as u32) & Wrapping(PRIME_Z)),
                    x3,
                    y3,
                    z3,
                );
        }

        let a4 = xa_flip_mask1 + a1;
        if a4 > 0.0 {
            let x4 = (xnmask | 1) as f32 + x1;
            let y4 = y1;
            let z4 = z1;
            value += (a4 * a4)
                * (a4 * a4)
                * grad3(
                    seed2,
                    xrbp + (Wrapping(xnmask as u32) & (Wrapping(PRIME_X) << 1)),
                    yrbp + Wrapping(PRIME_Y),
                    zrbp + Wrapping(PRIME_Z),
                    x4,
                    y4,
                    z4,
                );
            skip5 = true;
        }
    }

    let mut skip9 = false;
    let a6 = ya_flip_mask0 + a0;
    if a6 > 0.0 {
        let x6 = x0;
        let y6 = y0 - (ynmask | 1) as f32;
        let z6 = z0;
        value += (a6 * a6)
            * (a6 * a6)
            * grad3(
                seed,
                xrbp + (Wrapping(xnmask as u32) & Wrapping(PRIME_X)),
                yrbp + (Wrapping(!ynmask as u32) & Wrapping(PRIME_Y)),
                zrbp + (Wrapping(znmask as u32) & Wrapping(PRIME_Z)),
                x6,
                y6,
                z6,
            );
    } else {
        let a7 = xa_flip_mask0 + za_flip_mask0 + a0;
        if a7 > 0.0 {
            let x7 = x0 - (xnmask | 1) as f32;
            let y7 = y0;
            let z7 = z0 - (znmask | 1) as f32;
            value += (a7 * a7)
                * (a7 * a7)
                * grad3(
                    seed,
                    xrbp + (Wrapping(!xnmask as u32) & Wrapping(PRIME_X)),
                    yrbp + (Wrapping(ynmask as u32) & Wrapping(PRIME_Y)),
                    zrbp + (Wrapping(!znmask as u32) & Wrapping(PRIME_Z)),
                    x7,
                    y7,
                    z7,
                );
        }

        let a8 = ya_flip_mask1 + a1;
        if a8 > 0.0 {
            let x8 = x1;
            let y8 = (ynmask | 1) as f32 + y1;
            let z8 = z1;
            value += (a8 * a8)
                * (a8 * a8)
                * grad3(
                    seed2,
                    xrbp + Wrapping(PRIME_X),
                    yrbp + (Wrapping(ynmask as u32) & (Wrapping(PRIME_Y) << 1)),
                    zrbp + Wrapping(PRIME_Z),
                    x8,
                    y8,
                    z8,
                );
            skip9 = true;
        }
    }

    let mut skipd = false;
    let aa = za_flip_mask0 + a0;
    if aa > 0.0 {
        let xa = x0;
        let ya = y0;
        let za = z0 - (znmask | 1) as f32;
        value += (aa * aa)
            * (aa * aa)
            * grad3(
                seed,
                xrbp + (Wrapping(xnmask as u32) & Wrapping(PRIME_X)),
                yrbp + (Wrapping(ynmask as u32) & Wrapping(PRIME_Y)),
                zrbp + (Wrapping(!znmask as u32) & Wrapping(PRIME_Z)),
                xa,
                ya,
                za,
            );
    } else {
        let ab = xa_flip_mask0 + ya_flip_mask0 + a0;
        if ab > 0.0 {
            let xb = x0 - (xnmask | 1) as f32;
            let yb = y0 - (ynmask | 1) as f32;
            let zb = z0;
            value += (ab * ab)
                * (ab * ab)
                * grad3(
                    seed,
                    xrbp + (Wrapping(!xnmask as u32) & Wrapping(PRIME_X)),
                    yrbp + (Wrapping(!ynmask as u32) & Wrapping(PRIME_Y)),
                    zrbp + (Wrapping(znmask as u32) & Wrapping(PRIME_Z)),
                    xb,
                    yb,
                    zb,
                );
        }

        let ac = za_flip_mask1 + a1;
        if ac > 0.0 {
            let xc = x1;
            let yc = y1;
            let zc = (znmask | 1) as f32 + z1;
            value += (ac * ac)
                * (ac * ac)
                * grad3(
                    seed2,
                    xrbp + Wrapping(PRIME_X),
                    yrbp + Wrapping(PRIME_Y),
                    zrbp + (Wrapping(znmask as u32) & (Wrapping(PRIME_Z) << 1)),
                    xc,
                    yc,
                    zc,
                );
            skipd = true;
        }
    }

    if !skip5 {
        let a5 = ya_flip_mask1 + za_flip_mask1 + a1;
        if a5 > 0.0 {
            let x5 = x1;
            let y5 = (ynmask | 1) as f32 + y1;
            let z5 = (znmask | 1) as f32 + z1;
            value += (a5 * a5)
                * (a5 * a5)
                * grad3(
                    seed2,
                    xrbp + Wrapping(PRIME_X),
                    yrbp + (Wrapping(ynmask as u32) & (Wrapping(PRIME_Y) << 1)),
                    zrbp + (Wrapping(znmask as u32) & (Wrapping(PRIME_Z) << 1)),
                    x5,
                    y5,
                    z5,
                );
        }
    }

    if !skip9 {
        let a9 = xa_flip_mask1 + za_flip_mask1 + a1;
        if a9 > 0.0 {
            let x9 = (xnmask | 1) as f32 + x1;
            let y9 = y1;
            let z9 = (znmask | 1) as f32 + z1;
            value += (a9 * a9)
                * (a9 * a9)
                * grad3(
                    seed2,
                    xrbp + (Wrapping(xnmask as u32) & (Wrapping(PRIME_X) << 1)),
                    yrbp + Wrapping(PRIME_Y),
                    zrbp + (Wrapping(znmask as u32) & (Wrapping(PRIME_Z) << 1)),
                    x9,
                    y9,
                    z9,
                );
        }
    }

    if !skipd {
        let ad = xa_flip_mask1 + ya_flip_mask1 + a1;
        if ad > 0.0 {
            let xd = (xnmask | 1) as f32 + x1;
            let yd = (ynmask | 1) as f32 + y1;
            let zd = z1;
            value += (ad * ad)
                * (ad * ad)
                * grad3(
                    seed2,
                    xrbp + (Wrapping(xnmask as u32) & (Wrapping(PRIME_X) << 1)),
                    yrbp + (Wrapping(ynmask as u32) & (Wrapping(PRIME_Y) << 1)),
                    zrbp + Wrapping(PRIME_Z),
                    xd,
                    yd,
                    zd,
                );
        }
    }

    value
}

fn grad2(seed: Wrapping<u32>, xsvp: Wrapping<u32>, ysvp: Wrapping<u32>, dx: f32, dy: f32) -> f32 {
    let mut hash = seed ^ xsvp ^ ysvp;
    hash *= HASH_MULTIPLIER;
    hash ^= hash.0 >> (32 - N_GRADS_2D_EXPONENT + 1);
    let gi = (hash.0 as i32 & ((N_GRADS_2D - 1) << 1)) as usize;
    GRADIENTS_2D[gi | 0] * dx + GRADIENTS_2D[gi | 1] * dy
}

fn grad3(
    seed: Wrapping<u32>,
    xrvp: Wrapping<u32>,
    yrvp: Wrapping<u32>,
    zrvp: Wrapping<u32>,
    dx: f32,
    dy: f32,
    dz: f32,
) -> f32 {
    let mut hash = (seed ^ xrvp) ^ (yrvp ^ zrvp);
    hash *= HASH_MULTIPLIER;
    hash ^= hash.0 >> (32 - N_GRADS_3D_EXPONENT + 2);
    let gi = (hash.0 as i32 & ((N_GRADS_3D - 1) << 2)) as usize;
    GRADIENTS_3D[gi | 0] * dx + GRADIENTS_3D[gi | 1] * dy + GRADIENTS_3D[gi | 2] * dz
}

fn fast_floor(x: f32) -> i32 {
    let xi = x as i32;
    if x < xi as f32 {
        xi - 1
    } else {
        xi
    }
}

const GRAD2_SRC: &[f32] = &[
     0.382683432365090,  0.923879532511287,
     0.923879532511287,  0.382683432365090,
     0.923879532511287, -0.382683432365090,
     0.382683432365090, -0.923879532511287,
    -0.382683432365090, -0.923879532511287,
    -0.923879532511287, -0.382683432365090,
    -0.923879532511287,  0.382683432365090,
    -0.382683432365090,  0.923879532511287,
    //-----------------------------------//
     0.130526192220052,  0.991444861373810,
     0.608761429008721,  0.793353340291235,
     0.793353340291235,  0.608761429008721,
     0.991444861373810,  0.130526192220051,
     0.991444861373810, -0.130526192220051,
     0.793353340291235, -0.608761429008720,
     0.608761429008721, -0.793353340291235,
     0.130526192220052, -0.991444861373810,
    -0.130526192220052, -0.991444861373810,
    -0.608761429008721, -0.793353340291235,
    -0.793353340291235, -0.608761429008721,
    -0.991444861373810, -0.130526192220052,
    -0.991444861373810,  0.130526192220051,
    -0.793353340291235,  0.608761429008721,
    -0.608761429008721,  0.793353340291235,
    -0.130526192220052,  0.991444861373810,
];

const GRAD3_SRC: &[f32] = &[
     2.22474487139,       2.22474487139,      -1.0,                 0.0,
     2.22474487139,       2.22474487139,       1.0,                 0.0,
     3.0862664687972017,  1.1721513422464978,  0.0,                 0.0,
     1.1721513422464978,  3.0862664687972017,  0.0,                 0.0,
    -2.22474487139,       2.22474487139,      -1.0,                 0.0,
    -2.22474487139,       2.22474487139,       1.0,                 0.0,
    -1.1721513422464978,  3.0862664687972017,  0.0,                 0.0,
    -3.0862664687972017,  1.1721513422464978,  0.0,                 0.0,
    -1.0,                -2.22474487139,      -2.22474487139,       0.0,
     1.0,                -2.22474487139,      -2.22474487139,       0.0,
     0.0,                -3.0862664687972017, -1.1721513422464978,  0.0,
     0.0,                -1.1721513422464978, -3.0862664687972017,  0.0,
    -1.0,                -2.22474487139,       2.22474487139,       0.0,
     1.0,                -2.22474487139,       2.22474487139,       0.0,
     0.0,                -1.1721513422464978,  3.0862664687972017,  0.0,
     0.0,                -3.0862664687972017,  1.1721513422464978,  0.0,
    //----------------------------------------------------------------//
    -2.22474487139,      -2.22474487139,      -1.0,                 0.0,
    -2.22474487139,      -2.22474487139,       1.0,                 0.0,
    -3.0862664687972017, -1.1721513422464978,  0.0,                 0.0,
    -1.1721513422464978, -3.0862664687972017,  0.0,                 0.0,
    -2.22474487139,      -1.0,                -2.22474487139,       0.0,
    -2.22474487139,       1.0,                -2.22474487139,       0.0,
    -1.1721513422464978,  0.0,                -3.0862664687972017,  0.0,
    -3.0862664687972017,  0.0,                -1.1721513422464978,  0.0,
    -2.22474487139,      -1.0,                 2.22474487139,       0.0,
    -2.22474487139,       1.0,                 2.22474487139,       0.0,
    -3.0862664687972017,  0.0,                 1.1721513422464978,  0.0,
    -1.1721513422464978,  0.0,                 3.0862664687972017,  0.0,
    -1.0,                 2.22474487139,      -2.22474487139,       0.0,
     1.0,                 2.22474487139,      -2.22474487139,       0.0,
     0.0,                 1.1721513422464978, -3.0862664687972017,  0.0,
     0.0,                 3.0862664687972017, -1.1721513422464978,  0.0,
    -1.0,                 2.22474487139,       2.22474487139,       0.0,
     1.0,                 2.22474487139,       2.22474487139,       0.0,
     0.0,                 3.0862664687972017,  1.1721513422464978,  0.0,
     0.0,                 1.1721513422464978,  3.0862664687972017,  0.0,
     2.22474487139,      -2.22474487139,      -1.0,                 0.0,
     2.22474487139,      -2.22474487139,       1.0,                 0.0,
     1.1721513422464978, -3.0862664687972017,  0.0,                 0.0,
     3.0862664687972017, -1.1721513422464978,  0.0,                 0.0,
     2.22474487139,      -1.0,                -2.22474487139,       0.0,
     2.22474487139,       1.0,                -2.22474487139,       0.0,
     3.0862664687972017,  0.0,                -1.1721513422464978,  0.0,
     1.1721513422464978,  0.0,                -3.0862664687972017,  0.0,
     2.22474487139,      -1.0,                 2.22474487139,       0.0,
     2.22474487139,       1.0,                 2.22474487139,       0.0,
     1.1721513422464978,  0.0,                 3.0862664687972017,  0.0,
     3.0862664687972017,  0.0,                 1.1721513422464978,  0.0,
];

static GRADIENTS_2D: LazyLock<Vec<f32>> = LazyLock::new(|| {
    GRAD2_SRC
        .into_iter()
        .map(|v| (v / NORMALIZER_2D) as f32)
        .collect::<Vec<_>>() // cache divisions
        .into_iter()
        .cycle()
        .take((N_GRADS_2D * 2) as usize)
        .collect()
});

static GRADIENTS_3D: LazyLock<Vec<f32>> = LazyLock::new(|| {
    GRAD3_SRC
        .into_iter()
        .map(|v| (v / NORMALIZER_3D) as f32)
        .collect::<Vec<_>>() // cache divisions
        .into_iter()
        .cycle()
        .take((N_GRADS_3D * 4) as usize)
        .collect()
});
