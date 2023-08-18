//! World biome map implementation

use std::{iter::repeat, ops::{Deref, DerefMut}, cell::RefCell};

use hashbrown::HashMap;
use itertools::iproduct;
use serde::{Serialize, Deserialize};

use crate::{coordinates::{AbsChunkPos, RelChunkPos, AbsBlockPos, RelBlockPos}, registry::RegistryId};

use super::{BiomeEntry, biome_picker::BiomeGenerator, BiomeRegistry, Noises, BiomeDefinition};


pub const CACHE_MAX_ENTRIES: i32 = 12;
	
pub const REGION_SIZE_EXPONENT: i32 = 8; // SIZExSIZE, SIZE=2^EXPONENT; 2^7=128
pub const CHUNK_SIZE_EXPONENT: i32 = 5; // SIZExSIZE, SIZE=2^EXPONENT; 2^5=32
pub const BLEND_RADIUS: i32 = 16;

pub const REGION_SIZE: i32 = 1 << REGION_SIZE_EXPONENT;
pub const CHUNK_SIZE: i32 = 1 << CHUNK_SIZE_EXPONENT;
pub const PADDED_REGION_SIZE: i32 = REGION_SIZE + BLEND_RADIUS*2;
pub const PADDED_REGION_SIZE_SQZ: usize = (PADDED_REGION_SIZE * PADDED_REGION_SIZE) as usize;

/// The per-planet biome map.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct BiomeMap {
    /// Map of Chunk position to biome.
    map: HashMap<AbsBlockPos, BiomeEntry>,
    /// Map of Chunk position to biome definition.
    #[serde(skip)]
    pub base_map: HashMap<AbsBlockPos, (RegistryId, BiomeDefinition)>,
}

impl BiomeMap {
    /// Gets biomes near the supplied position, in all cardinal directions (with strides of X=1, Z=3, Y=3²).
    pub fn get_biomes_near(&self, pos: AbsBlockPos) -> [Option<&BiomeEntry>; 27] {
        let mut new_arr = Vec::from_iter(repeat(Option::None).take(27));
        for (o_x, o_z, o_y) in iproduct!(0..=2, 0..=2, 0..=2) {
            let obj = self.map.get(&(pos + RelBlockPos::new(o_x - 1, o_y - 1, o_z - 1)));
            if obj.is_some() {
                new_arr[(o_x + (o_z * 3) + (o_y * 3 * 3)) as usize] = Option::Some(obj.unwrap());
            }
        }
        <[Option<&BiomeEntry>; 27]>::try_from(new_arr).unwrap()
    }

    /// Gets a biome for a chunk, or if nonexistent, generates a new one.
    pub fn get_or_new<'a>(&'a mut self, pos: &AbsBlockPos, generator: &mut RefCell<BiomeGenerator>, registry: &BiomeRegistry, noises: &Noises, biome_map: &mut Option<[&'a BiomeDefinition; PADDED_REGION_SIZE_SQZ]>) -> Option<&(RegistryId, BiomeDefinition)> {
        if !self.contains_key(pos) {
            generator.borrow_mut().generate_biome(pos, self, registry, noises);
        }
        return ret_val;
    }
}

impl Deref for BiomeMap {
    type Target = HashMap<AbsBlockPos, BiomeEntry>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl DerefMut for BiomeMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}