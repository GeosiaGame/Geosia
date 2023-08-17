//! World biome map implementation

use std::{iter::repeat, ops::{Deref, DerefMut}, cell::RefCell};

use hashbrown::HashMap;
use itertools::iproduct;
use serde::{Serialize, Deserialize};

use crate::{coordinates::{AbsChunkPos, RelChunkPos, AbsChunkRange}, registry::RegistryId};

use super::{BiomeEntry, biome_picker::BiomeGenerator, BiomeRegistry, Noises, BiomeDefinition};

/// The per-planet biome map.
#[derive(Clone, Default, Serialize, Deserialize)]
#[repr(C)]
pub struct BiomeMap {
    /// Map of Chunk position to biome.
    map: HashMap<AbsChunkPos, BiomeEntry>,
    /// Map of Chunk position to biome definition.
    #[serde(skip)]
    pub base_map: HashMap<AbsChunkPos, (RegistryId, BiomeDefinition)>,
}

impl BiomeMap {
    /// Gets biomes near the supplied position, in all cardinal directions (with strides of X=1, Z=3, Y=3²).
    pub fn get_biomes_near(&self, pos: AbsChunkPos) -> [Option<&BiomeEntry>; 27] {
        let mut new_arr = Vec::from_iter(repeat(Option::None).take(27));
        for (o_x, o_z, o_y) in iproduct!(0..=2, 0..=2, 0..=2) {
            let obj = self.map.get(&(pos + RelChunkPos::new(o_x - 1, o_y - 1, o_z - 1)));
            if obj.is_some() {
                new_arr[(o_x + (o_z * 3) + (o_y * 3 * 3)) as usize] = Option::Some(obj.unwrap());
            }
        }
        <[Option<&BiomeEntry>; 27]>::try_from(new_arr).unwrap()
    }

    /// Gets a biome for a chunk, or if nonexistent, generates a new one.
    pub fn get_or_new(&mut self, pos: &AbsChunkPos, generator: &mut RefCell<BiomeGenerator>, registry: &BiomeRegistry, noises: &Noises) -> Option<&(RegistryId, BiomeDefinition)> {
        if !self.contains_key(pos) {
            generator.borrow_mut().generate_biome(pos, self, registry, noises);
        }
        return self.base_map.get(pos);
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