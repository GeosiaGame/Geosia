//! World biome map implementation

use hashbrown::HashMap;
use serde::{Serialize, Deserialize};
use smallvec::SmallVec;

use crate::registry::RegistryId;

use super::{BiomeEntry, BiomeDefinition};


/// Global scale modification, every other value is multiplied with this.
pub const GLOBAL_SCALE_MOD: f64 = 1.0;
/// Biome scale.
pub const GLOBAL_BIOME_SCALE: f64 = 64.0;

/// Expected amount of biomes per chunk
pub const EXPECTED_BIOME_COUNT: usize = 4;

/// The per-planet biome map.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct BiomeMap {
    /// Map of Chunk position to biome definition.
    pub biome_map: HashMap<[i32; 2], SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>>,
    /// Map of Chunk position to biome definition.
    pub noise_map: HashMap<[i32; 2], (f64, f64, f64)>,
    /// Generatable Biomes, with set seeds
    #[serde(skip)] // TODO fix serialization of `BiomeDefinition`
    pub generatable_biomes: Vec<(RegistryId, BiomeDefinition)>,
}
