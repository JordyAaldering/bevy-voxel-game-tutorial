use crate::{ChunkData, ChunkCoord, Compass};

use std::{ops::Index, sync::Arc};

use bevy::{math::CompassOctant, platform::collections::HashMap};

pub struct ChunkNeighbors {
    pub north: Option<Arc<ChunkData>>,
    pub north_east: Option<Arc<ChunkData>>,
    pub east: Option<Arc<ChunkData>>,
    pub south_east: Option<Arc<ChunkData>>,
    pub south: Option<Arc<ChunkData>>,
    pub south_west: Option<Arc<ChunkData>>,
    pub west: Option<Arc<ChunkData>>,
    pub north_west: Option<Arc<ChunkData>>,
}

impl ChunkNeighbors {
    pub fn from(pos: ChunkCoord, chunks: &HashMap<ChunkCoord, Arc<ChunkData>>) -> Self {
        Self {
            north: chunks.get(&ChunkCoord::new(pos.x, pos.z + 1)).cloned(),
            north_east: chunks.get(&ChunkCoord::new(pos.x + 1, pos.z + 1)).cloned(),
            east: chunks.get(&ChunkCoord::new(pos.x + 1, pos.z)).cloned(),
            south_east: chunks.get(&ChunkCoord::new(pos.x + 1, pos.z - 1)).cloned(),
            south: chunks.get(&ChunkCoord::new(pos.x, pos.z - 1)).cloned(),
            south_west: chunks.get(&ChunkCoord::new(pos.x - 1, pos.z - 1)).cloned(),
            west: chunks.get(&ChunkCoord::new(pos.x - 1, pos.z)).cloned(),
            north_west: chunks.get(&ChunkCoord::new(pos.x - 1, pos.z + 1)).cloned(),
        }
    }
}

impl Index<Compass> for ChunkNeighbors {
    type Output = Option<Arc<ChunkData>>;

    fn index(&self, index: Compass) -> &Self::Output {
        match index {
            Compass::Front => &self.north,
            Compass::Right => &self.east,
            Compass::Back => &self.south,
            Compass::Left => &self.west,
        }
    }
}

impl Index<CompassOctant> for ChunkNeighbors {
    type Output = Option<Arc<ChunkData>>;

    fn index(&self, index: CompassOctant) -> &Self::Output {
        match index {
            CompassOctant::North => &self.north,
            CompassOctant::NorthEast => &self.north_east,
            CompassOctant::East => &self.east,
            CompassOctant::SouthEast => &self.south_east,
            CompassOctant::South => &self.south,
            CompassOctant::SouthWest => &self.south_west,
            CompassOctant::West => &self.west,
            CompassOctant::NorthWest => &self.north_west,
        }
    }
}
