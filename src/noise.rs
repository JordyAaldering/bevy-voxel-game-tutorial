pub mod opensimplex2s;
pub mod voronoi;

use bevy::math::Vec2;

/// The underlying noise algorithm.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum NoiseType {
    #[default]
    OpenSimplex2S,
    /// Distance to the nearest of randomly placed points; gives cell-like patterns.
    Voronoi,
}

impl NoiseType {
    fn sample2(self, seed: i64, x: f64, z: f64) -> f32 {
        match self {
            Self::OpenSimplex2S => opensimplex2s::noise2(seed, x, z),
            Self::Voronoi => voronoi::noise2(seed, x, z),
        }
    }

    fn sample3(self, seed: i64, x: f64, y: f64, z: f64) -> f32 {
        match self {
            Self::OpenSimplex2S => opensimplex2s::noise3(seed, x, y, z),
            Self::Voronoi => voronoi::noise3(seed, x, y, z),
        }
    }
}

/// Configuration of fractal noise, independent of seed and size.
#[derive(Clone, Copy, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct NoiseSettings {
    /// The underlying noise algorithm.
    pub noise_type: NoiseType,
    /// Offset added to the sample position, in world units.
    /// Shifts the pattern without changing its shape.
    pub offset: Vec2,
    /// Number of noise layers summed together. One octave gives smooth, rolling
    /// shapes; each extra octave adds finer detail on top (hills, then bumps,
    /// then roughness). More octaves cost proportionally more to compute.
    pub octaves: usize,
    /// Scale of the output, which lies in `[bias - amplitude, bias + amplitude]`.
    /// For terrain this is the height variation in blocks.
    pub amplitude: f32,
    /// Constant added to the output, centering its range around `bias`.
    /// For terrain this is the base height that the noise varies around.
    pub bias: f32,
    /// Frequency of the first octave, in cycles per world unit.
    /// Higher values give features that are smaller and more frequent;
    /// lower values stretch the terrain out into broad, gentle features.
    pub frequency: f32,
    /// Amplitude multiplier per octave, typically in `(0, 1)`.
    /// Low values make the fine detail faint, so the terrain stays smooth;
    /// values near 1 give the fine octaves nearly equal weight, so it is rough.
    pub persistence: f32,
    /// Frequency multiplier per octave, typically around 2.
    /// Controls how much finer each successive layer of detail is.
    /// Higher values spread the detail scales further apart.
    pub lacunarity: f32,
}

impl Default for NoiseSettings {
    fn default() -> Self {
        Self {
            noise_type: NoiseType::default(),
            offset: Vec2::ZERO,
            octaves: 1,
            amplitude: 1.0,
            bias: 0.0,
            frequency: 0.01,
            persistence: 0.5,
            lacunarity: 2.0,
        }
    }
}

impl NoiseSettings {
    /// Samples fractal noise at the given world position.
    #[allow(unused)]
    pub fn sample2(&self, seed: i64, x: f32, z: f32) -> f32 {
        let x = x + self.offset.x;
        let z = z + self.offset.y;

        let mut frequency = self.frequency;
        let mut amplitude = 1.0;
        let mut total = 0.0;
        let mut max = 0.0;

        for octave in 0..self.octaves {
            // Decorrelate octaves by giving each its own seed
            let octave_seed = seed.wrapping_add(octave as i64);
            total += amplitude * self.noise_type.sample2(octave_seed, (x * frequency) as f64, (z * frequency) as f64);
            max += amplitude;
            amplitude *= self.persistence;
            frequency *= self.lacunarity;
        }

        if max == 0.0 {
            self.bias
        } else {
            (total / max) as f32 * self.amplitude + self.bias
        }
    }

    #[allow(unused)]
    pub fn sample3(&self, seed: i64, x: f32, y: f32, z: f32) -> f32 {
        let x = x + self.offset.x;
        let z = z + self.offset.y;

        let mut frequency = self.frequency;
        let mut amplitude = 1.0;
        let mut total = 0.0;
        let mut max = 0.0;

        for octave in 0..self.octaves {
            // Decorrelate octaves by giving each its own seed
            let octave_seed = seed.wrapping_add(octave as i64);
            total += amplitude * self.noise_type.sample3(octave_seed, (x * frequency) as f64, (y * frequency) as f64, (z * frequency) as f64);
            max += amplitude;
            amplitude *= self.persistence;
            frequency *= self.lacunarity;
        }

        if max == 0.0 {
            self.bias
        } else {
            (total / max) as f32 * self.amplitude + self.bias
        }
    }
}

/*
/// A seeded noise map covering a `width` x `height` grid, computed in one batch.
#[derive(Clone, Copy, Debug)]
pub struct NoiseMap2d {
    seed: i64,
    width: usize,
    height: usize,
}

impl NoiseMap2d {
    pub fn new(seed: i64, width: usize, height: usize) -> Self {
        Self { seed, width, height }
    }

    /// Computes the whole map, with the grid origin at world position `origin`.
    /// Values are stored row-major: index `z * width + x`.
    pub fn generate(&self, settings: &NoiseSettings, origin: Vec2) -> Vec<f32> {
        let mut values = Vec::with_capacity(self.width * self.height);
        for x in 0..self.width {
            for z in 0..self.height {
                values.push(settings.sample(self.seed, origin.x + x as f32, origin.y + z as f32));
            }
        }
        values
    }
}
*/
