use crate::{ChunkData, ChunkCoord, Compass};

use std::{ops::Index, sync::Arc};

use bevy::platform::collections::HashMap;

pub struct ChunkNeighbors {
    pub north: Option<Arc<ChunkData>>,
    pub south: Option<Arc<ChunkData>>,
    pub east: Option<Arc<ChunkData>>,
    pub west: Option<Arc<ChunkData>>,
}

impl ChunkNeighbors {
    pub fn from(pos: ChunkCoord, chunks: &HashMap<ChunkCoord, Arc<ChunkData>>) -> Self {
        let north = chunks.get(&ChunkCoord::new(pos.x, pos.z + 1)).cloned();
        let south = chunks.get(&ChunkCoord::new(pos.x, pos.z - 1)).cloned();
        let east  = chunks.get(&ChunkCoord::new(pos.x + 1, pos.z)).cloned();
        let west  = chunks.get(&ChunkCoord::new(pos.x - 1, pos.z)).cloned();
        Self { north, east, south, west }
    }
}

impl Index<Compass> for ChunkNeighbors {
    type Output = Option<Arc<ChunkData>>;

    fn index(&self, index: Compass) -> &Self::Output {
        match index {
            Compass::Front => &self.north,
            Compass::Right  => &self.east,
            Compass::Back => &self.south,
            Compass::Left  => &self.west,
        }
    }
}
