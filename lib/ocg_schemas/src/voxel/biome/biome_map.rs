//! World biome map implementation

use std::{ops::{Deref, DerefMut}, cell::RefCell};

use hashbrown::HashMap;
use serde::{Serialize, Deserialize};
use smallvec::{SmallVec, smallvec};

use crate::{coordinates::AbsChunkPos, registry::RegistryId};

use super::{BiomeEntry, biome_picker::BiomeGenerator, BiomeRegistry, Noises, BiomeDefinition, VOID_BIOME_NAME, VOID_BIOME};


pub const CACHE_MAX_ENTRIES: i32 = 12;
	
pub const REGION_SIZE_EXPONENT: i32 = 8; // SIZExSIZE, SIZE=2^EXPONENT; 2^7=128
pub const CHUNK_SIZE_EXPONENT: i32 = 5; // SIZExSIZE, SIZE=2^EXPONENT; 2^5=32
pub const BLEND_RADIUS: i32 = 16;
pub const BLEND_CIRCUMFERENCE: i32 = BLEND_RADIUS * 2 + 1;

pub const REGION_SIZE: i32 = 1 << REGION_SIZE_EXPONENT;
pub const CHUNK_SIZE: i32 = 1 << CHUNK_SIZE_EXPONENT;
pub const PADDED_REGION_SIZE: i32 = REGION_SIZE + BLEND_RADIUS*2;
pub const PADDED_REGION_SIZE_SQZ: usize = (PADDED_REGION_SIZE * PADDED_REGION_SIZE) as usize;

/// The per-planet biome map.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct BiomeMap {
    /// Map of Chunk position to biome.
    map: HashMap<AbsChunkPos, BiomeEntry>,
    /// Map of Chunk position to biome definition.
    #[serde(skip)]
    pub base_map: HashMap<AbsChunkPos, (RegistryId, BiomeDefinition)>,
}

impl BiomeMap {

    /// Gets a biome for a chunk, or if nonexistent, generates a new one.
    pub fn get_or_new<'a>(&'a mut self, pos: &AbsChunkPos, generator: &mut RefCell<BiomeGenerator>, registry: &BiomeRegistry, noises: &Noises) -> Option<&(RegistryId, BiomeDefinition)> {
        if !self.contains_key(pos) {
            let mut gen = generator.borrow_mut();
            let gen = gen.generate_biome(pos, self, registry, noises);
            self.base_map.insert(*pos, (gen.0, gen.1.to_owned()));
        }
        return self.base_map.get(pos);
    }

    pub fn generate_region(&mut self, region_x: i32, region_z: i32, generator: &mut RefCell<BiomeGenerator>, registry: &BiomeRegistry, noises: &Noises) -> SmallVec<[(RegistryId, BiomeDefinition); PADDED_REGION_SIZE_SQZ]> {
        let mut biome_map = smallvec![registry.lookup_name_to_object(VOID_BIOME_NAME.as_ref()).map(|x| (x.0, x.1.to_owned())).unwrap(); PADDED_REGION_SIZE_SQZ];
        for rx in 0..PADDED_REGION_SIZE {
            let x = (rx - BLEND_RADIUS) + (region_x << REGION_SIZE_EXPONENT);
            for rz in 0..PADDED_REGION_SIZE {
                let z = (rz - BLEND_RADIUS) + (region_z << REGION_SIZE_EXPONENT);

                let biome = generator.borrow_mut().generate_biome(&AbsChunkPos::new(x, 0, z), self, registry, noises);
                self.base_map.insert(AbsChunkPos::new(x, 0, z), biome.clone());

                biome_map[(rx + rz * PADDED_REGION_SIZE) as usize] = biome;
            }
        }
        biome_map
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