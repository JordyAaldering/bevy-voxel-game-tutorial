use crate::{CHUNK_HEIGHT, CHUNK_WIDTH, VoxelType, coords::ChunkCoord, noise::NoiseSettings, ore::Ore, write_atomic};

use std::{fs, io, path::{Path, PathBuf}};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

const SURFACE_SALT: u64 = 0x5352_4641_4345;
const TREE_SALT: u64 = 0x5452_4545;
const TEMPERATURE_SEED_OFFSET: i32 = 0x7E57;
const HUMIDITY_SEED_OFFSET: i32 = 0x4801;
const CAVE_SEED_OFFSET: i32 = 0xCA7E;
const ORE_SEED_OFFSET: i32 = 0x0E5;

/// Terrain generation settings of a region of the world.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Biome {
    pub name: String,
    /// Where this biome sits in climate space; the closest biome to the local climate wins.
    pub temperature: f32,
    pub humidity: f32,
    /// Terrain height; in blocks.
    pub height: NoiseSettings,
    /// Top-most voxel.
    pub surface: VoxelType,
    /// Voxels directly below the surface.
    pub subsurface: VoxelType,
    /// Probability of a tree on each surface column, between 0.0 and 1.0.
    pub tree_density: f32,
}

/// Noise that decides which biome applies where.
/// Both maps should roughly span `[-1, 1]`, matching the biomes' `temperature` and `humidity`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Climate {
    pub temperature: NoiseSettings,
    pub humidity: NoiseSettings,
    /// Width of the transition between biomes, in climate units.
    /// A biome contributes to a column when its climate distance is at most this much
    /// further away than that of the closest biome.
    pub blend: f32,
}

/// Caves are carved out of the terrain wherever 3D noise exceeds a threshold.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Caves {
    /// Sampled in 3D; should roughly span `[-1, 1]`.
    pub noise: NoiseSettings,
    /// Noise value above which a voxel becomes air. Lower values give more and bigger caves.
    pub threshold: f32,
}

/// Settings of a single world, stored in `world.json` next to its chunks.
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct WorldConfig {
    pub seed: i32,
    pub climate: Climate,
    pub caves: Caves,
    pub ores: Vec<Ore>,
    pub biomes: Vec<Biome>,
}

/// Everything terrain generation needs to know about one (x, z) column.
pub struct Column {
    /// Height of the top-most (surface) voxel.
    pub height: usize,
    pub surface: VoxelType,
    pub subsurface: VoxelType,
    pub tree_density: f32,
}

impl WorldConfig {
    fn path() -> PathBuf {
        Path::new(crate::SAVE_DIR).join("world.json")
    }

    /// Writes the default configuration if there is none yet, so it can be edited afterwards.
    /// Terrain depends on this file, so an invalid one is an error instead of falling back to the defaults.
    pub fn load_or_create() -> Result<Self, String> {
        let path = Self::path();
        let config = match fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).map_err(|e| format!("{}: {e}", path.display()))?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let config = Self::default();
                let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
                write_atomic(&path, json.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
                config
            }
            Err(error) => return Err(format!("{}: {error}", path.display())),
        };

        if config.biomes.is_empty() {
            return Err(format!("{}: at least one biome is required", path.display()));
        }
        Ok(config)
    }

    pub fn column(&self, x: i32, z: i32) -> Column {
        let (fx, fz) = (x as f32, z as f32);
        let temperature = self.climate.temperature.sample2(self.seed.wrapping_add(TEMPERATURE_SEED_OFFSET), fx, fz);
        let humidity = self.climate.humidity.sample2(self.seed.wrapping_add(HUMIDITY_SEED_OFFSET), fx, fz);
        self.column_from_samples(x, z, temperature, humidity, |index| {
            self.biomes[index].height.sample2(self.seed, fx, fz)
        })
    }

    /// Generates columns in row-major order, with x varying fastest.
    pub fn columns(&self, coord: ChunkCoord) -> Vec<Column> {
        let temperature = self.climate.temperature.area2(
            self.seed.wrapping_add(TEMPERATURE_SEED_OFFSET), coord.x, coord.z,
        );
        let humidity = self.climate.humidity.area2(
            self.seed.wrapping_add(HUMIDITY_SEED_OFFSET), coord.x, coord.z,
        );
        let heights: Vec<_> = self.biomes.iter()
            .map(|biome| biome.height.area2(self.seed, coord.x, coord.z))
            .collect();

        (0..CHUNK_WIDTH).flat_map(|z| (0..CHUNK_WIDTH).map(move |x| (x, z)))
            .map(|(x, z)| {
                let index = x + z * CHUNK_WIDTH;
                let world_x = coord.x * CHUNK_WIDTH as i32 + x as i32;
                let world_z = coord.z * CHUNK_WIDTH as i32 + z as i32;
                self.column_from_samples(world_x, world_z, temperature[index], humidity[index], |biome| {
                    heights[biome][index]
                })
            })
            .collect()
    }

    fn column_from_samples(
        &self,
        x: i32,
        z: i32,
        temperature: f32,
        humidity: f32,
        mut height_sample: impl FnMut(usize) -> f32,
    ) -> Column {
        let distance = |biome: &Biome| {
            let dt = biome.temperature - temperature;
            let dh = biome.humidity - humidity;
            (dt * dt + dh * dh).sqrt()
        };
        let nearest = self.biomes.iter().map(distance).fold(f32::INFINITY, f32::min);
        // The nearest biome always has weight `blend`, so the total is positive.
        // Deep inside a biome it is the only one with any weight.
        let weight = |biome: &Biome| (nearest + self.climate.blend - distance(biome)).max(0.0);
        let total: f32 = self.biomes.iter().map(weight).sum();

        let mut height = 0.0;
        for (index, biome) in self.biomes.iter().enumerate() {
            let weight = weight(biome);
            if weight > 0.0 {
                height += weight / total * height_sample(index);
            }
        }
        let height = (height as i32).clamp(0, CHUNK_HEIGHT as i32 - 1) as usize;

        // Voxels cannot be blended, so each column picks one biome for its blocks, with
        // probability proportional to its weight. This dithers the border between biomes.
        let mut pick = hash01(self.seed, x, z, SURFACE_SALT) * total;
        let mut chosen = &self.biomes[0];
        for biome in &self.biomes {
            let weight = weight(biome);
            if weight <= 0.0 {
                continue;
            }
            chosen = biome;
            if pick < weight {
                break;
            }
            pick -= weight;
        }

        Column {
            height,
            surface: chosen.surface,
            subsurface: chosen.subsurface,
            tree_density: chosen.tree_density,
        }
    }

    /// Noise values for cave generation in voxel index order.
    pub fn cave_noise(&self, coord: ChunkCoord) -> Vec<f32> {
        let seed = self.seed.wrapping_add(CAVE_SEED_OFFSET);
        self.caves.noise.area3(seed, coord.x, coord.z)
    }

    /// The ore voxel at this position, if any. The first ore in `ores` wins where clusters overlap.
    pub fn ore_at(&self, x: i32, y: i32, z: i32) -> Option<VoxelType> {
        self.ores.iter().enumerate()
            .find(|(index, ore)| {
                // Every ore has its own seed, so their clusters do not line up
                let seed = self.seed.wrapping_add(ORE_SEED_OFFSET).wrapping_add((*index as i32 + 1).wrapping_mul(0x9E37_79B9u32 as i32));
                ore.contains(seed, x, y, z)
            })
            .map(|(_, ore)| ore.voxel)
    }

    /// Whether a tree grows in this column, given the column's biome.
    pub fn has_tree(&self, column: &Column, x: i32, z: i32) -> bool {
        hash01(self.seed, x, z, TREE_SALT) < column.tree_density
    }
}

impl Default for WorldConfig {
    fn default() -> Self {
        let climate_noise = NoiseSettings {
            octaves: 2,
            frequency: 0.003,
            ..default()
        };

        Self {
            seed: 0,
            climate: Climate {
                temperature: climate_noise,
                humidity: climate_noise,
                blend: 0.1,
            },
            caves: Caves {
                noise: NoiseSettings {
                    octaves: 2,
                    frequency: 0.04,
                    ..default()
                },
                threshold: 0.45,
            },
            ores: vec![
                Ore { name: "coal".into(), voxel: VoxelType::StoneCoal, min_height: 5, max_height: 60, rarity: 0.2, cluster_size: 3.0 },
                Ore { name: "iron".into(), voxel: VoxelType::StoneIron, min_height: 5, max_height: 45, rarity: 0.1, cluster_size: 3.0 },
                Ore { name: "gold".into(), voxel: VoxelType::StoneGold, min_height: 3, max_height: 25, rarity: 0.03, cluster_size: 2.0 },
                Ore { name: "diamond".into(), voxel: VoxelType::StoneDiamond, min_height: 1, max_height: 14, rarity: 0.01, cluster_size: 1.5 },
            ],
            biomes: vec![
                Biome {
                    name: "plains".into(),
                    temperature: 0.1,
                    humidity: 0.4,
                    height: NoiseSettings {
                        octaves: 2,
                        amplitude: 6.0,
                        bias: 20.0,
                        frequency: 0.01,
                        ..default()
                    },
                    surface: VoxelType::Grass,
                    subsurface: VoxelType::Dirt,
                    tree_density: 0.004,
                },
                Biome {
                    name: "desert".into(),
                    temperature: 0.5,
                    humidity: -0.5,
                    height: NoiseSettings {
                        amplitude: 4.0,
                        bias: 19.0,
                        frequency: 0.015,
                        ..default()
                    },
                    surface: VoxelType::Sand,
                    subsurface: VoxelType::Sand,
                    tree_density: 0.0,
                },
                Biome {
                    name: "mountains".into(),
                    temperature: -0.5,
                    humidity: -0.1,
                    height: NoiseSettings {
                        octaves: 4,
                        amplitude: 22.0,
                        bias: 36.0,
                        frequency: 0.01,
                        ..default()
                    },
                    surface: VoxelType::Stone,
                    subsurface: VoxelType::Stone,
                    tree_density: 0.0005,
                },
            ],
        }
    }
}

/// Deterministic pseudo-random value in `[0, 1)` for a column, independent of generation order.
fn hash01(seed: i32, x: i32, z: i32, salt: u64) -> f32 {
    let position = (x as u32 as u64) << 32 | z as u32 as u64;
    let mut h = (seed as i64 as u64) ^ salt ^ position.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 30;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h >> 40) as f32 / (1u32 << 24) as f32
}
