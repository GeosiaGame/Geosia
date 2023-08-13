//! World biome map implementation

use std::{iter::repeat, ops::{Deref, DerefMut}};

use hashbrown::HashMap;
use itertools::iproduct;
use serde::{Serialize, Deserialize};

use crate::coordinates::{AbsChunkPos, RelChunkPos};

use super::BiomeEntry;

/// The per-planet biome map.
#[derive(Clone, Default, Serialize, Deserialize)]
#[repr(C)]
pub struct BiomeMap {
    /// Map of Chunk position to biome.
    map: HashMap<AbsChunkPos, BiomeEntry>
}

impl BiomeMap {
    /// Gets biomes near the supplied position, in all cardinal directions (with strides of X=1, Z=3, Y=3²).
    pub fn get_biomes_near(&self, pos: AbsChunkPos) -> [Option<&BiomeEntry>; 27] {
        let mut new_arr = Vec::from_iter(repeat(Option::None).take(27));
        for (o_x, o_z, o_y) in iproduct!(-1..=1, -1..=1, -1..=1) {
            let obj = self.map.get(&(pos + RelChunkPos::new(o_x, o_y, o_z)));
            if obj.is_some() {
                new_arr.insert((o_x + (o_z * 3) + (o_y * 3 * 3)) as usize, Option::Some(obj.unwrap()));
            }
        }
        <[Option<&BiomeEntry>; 27]>::try_from(new_arr).unwrap()
    }
}

impl Deref for BiomeMap {
    type Target = HashMap<AbsChunkPos, BiomeEntry>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl DerefMut for BiomeMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}