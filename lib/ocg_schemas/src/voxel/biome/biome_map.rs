//! World biome map implementation

use hashbrown::HashMap;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use super::{BiomeDefinition, BiomeEntry};
use crate::registry::RegistryId;

/// Global scale modification, every other value is multiplied with this.
pub const GLOBAL_SCALE_MOD: f64 = 1.0;
/// Biome scale.
pub const GLOBAL_BIOME_SCALE: f64 = 64.0;

/// Expected amount of biomes per chunk
pub const EXPECTED_BIOME_COUNT: usize = 4;

/// The per-planet biome map.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct BiomeMap {
    /// Map of block position to biome definition.
    pub biome_map: HashMap<[i32; 2], SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>>,
    /// Map of block position to biome definition.
    pub noise_map: HashMap<[i32; 2], (f64, f64, f64)>,
    /// Map of block position to elevation.
    pub height_map: HashMap<[i32; 2], i32>,
    /// Generatable Biomes, with set seeds
    #[serde(skip)] // TODO fix serialization of `BiomeDefinition`
    pub generatable_biomes: Vec<(RegistryId, BiomeDefinition)>,
}
