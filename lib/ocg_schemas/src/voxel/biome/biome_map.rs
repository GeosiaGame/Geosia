//! World biome map implementation

use bevy::prelude::Resource;
use hashbrown::HashMap;
use serde::{Serialize, Deserialize};
use smallvec::SmallVec;

use crate::{coordinates::CHUNK_DIM, registry::RegistryId};

use super::{BiomeEntry, BiomeDefinition};


/// SIZExSIZE, SIZE=2^EXPONENT; 2^8=256
pub const SUPERGRID_DIM_EXPONENT: i32 = 8;
/// SIZExSIZE, SIZE=2^EXPONENT; 2^5=32
pub const CHUNK_SIZE_EXPONENT: i32 = 5;
/// Blend radius in blocks.
pub const BLEND_RADIUS: i32 = 32;
/// Blend circumference in blocks.
pub const BLEND_CIRCUMFERENCE: i32 = BLEND_RADIUS * 2 + 1;

/// Size of a single region.
pub const SUPERGRID_DIM: i32 = 4 * CHUNK_DIM;
/// Padded region size.
pub const PADDED_REGION_SIZE: i32 = SUPERGRID_DIM + BLEND_RADIUS * 2;
/// Square of the padded region size, as `usize`.
pub const PADDED_REGION_SIZE_SQZ: usize = (PADDED_REGION_SIZE * PADDED_REGION_SIZE) as usize;

/// Global scale modification, every other value is multiplied with this.
pub const GLOBAL_SCALE_MOD: f64 = 1.0;
/// Biome scale.
pub const GLOBAL_BIOME_SCALE: f64 = 256.0;

/// The per-planet biome map.
#[derive(Clone, Default, Serialize, Deserialize, Resource)]
pub struct BiomeMap {
    /// Map of Chunk position to biome definition.
    #[serde(skip)] // TODO fix serialization of `BiomeDefinition`
    pub base_map: HashMap<[i32; 2], (RegistryId, BiomeDefinition)>,
    /// The final map of Block column -> Weighted biome entry.
    pub final_map: HashMap<[i32; 2], SmallVec<[BiomeEntry; 3]>>,
    /// Generatable Biomes, with set seeds
    #[serde(skip)] // TODO fix serialization of `BiomeDefinition`
    pub gen_biomes: Vec<(RegistryId, BiomeDefinition)>,
}
