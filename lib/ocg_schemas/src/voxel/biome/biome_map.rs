//! World biome map implementation

use bevy::prelude::Resource;
use hashbrown::HashMap;
use serde::{Serialize, Deserialize};
use smallvec::SmallVec;

use crate::{coordinates::REGION_SIZE, registry::RegistryId};

use super::{BiomeEntry, BiomeDefinition};


/// SIZExSIZE, SIZE=2^EXPONENT; 2^8=256
pub const REGION_SIZE_EXPONENT: i32 = 8;
/// SIZExSIZE, SIZE=2^EXPONENT; 2^5=32
pub const CHUNK_SIZE_EXPONENT: i32 = 5;
/// Blend radius in blocks.
pub const BLEND_RADIUS: i32 = 16;
/// Blend circumference in blocks.
pub const BLEND_CIRCUMFERENCE: i32 = BLEND_RADIUS * 2 + 1;
/// Padded region size.
pub const PADDED_REGION_SIZE: i32 = REGION_SIZE + BLEND_RADIUS * 2;
/// Square of the padded region size, as `usize`.
pub const PADDED_REGION_SIZE2Z: usize = (PADDED_REGION_SIZE * PADDED_REGION_SIZE) as usize;

/// Global scale modification, every other value is multiplied with this.
pub const GLOBAL_SCALE_MOD: f64 = 1.0;
/// Biome scale.
pub const GLOBAL_BIOME_SCALE: f64 = 64.0;

/// Expected amount of biomes per chunk
pub const EXPECTED_BIOME_COUNT: usize = 4;

/// The per-planet biome map.
#[derive(Clone, Default, Serialize, Deserialize, Resource)]
pub struct BiomeMap {
    /// Map of Chunk position to biome definition.
    pub map: HashMap<[i32; 2], SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>>,
    /// Generatable Biomes, with set seeds
    #[serde(skip)] // TODO fix serialization of `BiomeDefinition`
    pub generatable_biomes: Vec<(RegistryId, BiomeDefinition)>,
}
