//! World biome map implementation

use bevy_math::IVec2;
use hashbrown::HashMap;
use itertools::iproduct;
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
    /// Map of block position to biome definition. `(elevation, temperature, moisture)`.
    pub noise_map: HashMap<[i32; 2], (f64, f64, f64)>,
    /// Map of block position to elevation.
    pub height_map: HashMap<[i32; 2], i32>,
    /// Generatable Biomes, with set seeds
    #[serde(skip)] // TODO fix serialization of `BiomeDefinition`
    pub generatable_biomes: Vec<(RegistryId, BiomeDefinition)>,
}

impl BiomeMap {
    /// Get the elevation at this 2D point.
    pub fn elevation_at(&self, pos: IVec2) -> f64 {
        self.noise_map
            .get(&pos.to_array())
            .unwrap_or(&(0.0, 0.0, 0.0))
            .0
    }
    /// Get the temperature at this 2D point.
    pub fn temperature_at(&self, pos: IVec2) -> f64 {
        self.noise_map
            .get(&pos.to_array())
            .unwrap_or(&(0.0, 0.0, 0.0))
            .1
    }
    /// Get the moisture at this 2D point.
    pub fn moisture_at(&self, pos: IVec2) -> f64 {
        self.noise_map
            .get(&pos.to_array())
            .unwrap_or(&(0.0, 0.0, 0.0))
            .2
    }

    /// Get the heightmap points within the given area (inclusive).
    pub fn heightmap_between(&self, min: IVec2, max: IVec2) -> HashMap<IVec2, i32> {
        let mut points = HashMap::new();
        for (x, y) in iproduct!(min.x..=max.x, min.y..=max.y) {
            points.insert(IVec2::new(x, y), *self.height_map.get(&[x, y]).unwrap_or(&0));
        }
        points
    }
}
