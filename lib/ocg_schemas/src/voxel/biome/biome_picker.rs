//! Random biome picker

use crate::{voxel::biome::{BiomeRegistry, biome_map::BiomeMap}, coordinates::{AbsChunkRange, AbsChunkPos, RelChunkPos, CHUNK_DIM}, registry::RegistryId};
use noise::NoiseFn;
use serde::{Serialize, Deserialize};
use bevy_math::IVec3;

use super::{Noises, BiomeDefinition, PLAINS_BIOME_NAME};

/// The generic biome selector.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct BiomeGenerator {
    seed: u64,
}

impl Default for BiomeGenerator {
    fn default() -> Self {
        Self { 
            seed: 0,
        }
    }
}

impl BiomeGenerator {
    /// Helper function to create a new, seeded BiomeGenerator.
    pub fn new(seed: u64) -> Self {
        Self {
            seed: seed,
        }
    }

    fn map_range(from_range: (f64, f64), to_range: (f64, f64), s: f64) -> f64 {
        to_range.0 + (s - from_range.0) * (to_range.1 - to_range.0) / (from_range.1 - from_range.0)
    }

    fn pick_biome<'a>(center: AbsChunkPos, pos: RelChunkPos, _map: &BiomeMap, registry: &'a BiomeRegistry, noises: &Noises) -> (RegistryId, &'a BiomeDefinition) {
        let pos_d = (center + pos).as_dvec3();
        let pos_d = [pos_d.x, pos_d.z];
        let height = noises.elevation_noise.get(pos_d); // Self::map_range((-1.0, 1.0), (0.0, 1.0), noises.elevation_noise.get(pos_d));
        let wetness = noises.moisture_noise.get(pos_d); //Self::map_range((-1.0, 1.0), (0.0, 1.0), noises.moisture_noise.get(pos_d));
        let temp = noises.temperature_noise.get(pos_d); //Self::map_range((-1.0, 1.0), (0.0, 1.0), noises.temperature_noise.get(pos_d));

        let mut final_id = None;

        let objects = registry.get_objects_ids(); // TODO change from looping the registry to getting a list of generatable biomes from somewhere (BiomeMap?)
        for id in objects.iter() {
            if let Some(obj) = id {
                let id = obj.0;
                let obj = obj.1;
                if obj.elevation.contains(height) && obj.moisture.contains(wetness) && obj.temperature.contains(temp) {
                    final_id = Some((*id, obj));
                }
            }
        }
        final_id.unwrap_or_else(|| registry.lookup_name_to_object(PLAINS_BIOME_NAME.as_ref()).unwrap())
    }

    /// Generates biomes for a range of positions.
    pub fn generate_area_biomes(&mut self, area: AbsChunkRange, biome_map: &mut BiomeMap, registry: &BiomeRegistry, noises: &Noises) {
        let center = area.max() - RelChunkPos::from(area.min().into_ivec3() / 2);
        for pos in area.iter_xzy() {
            let biome_def = BiomeGenerator::pick_biome(AbsChunkPos::from_ivec3(center.0 * CHUNK_DIM), RelChunkPos::from_ivec3(pos.0), &biome_map, registry, noises);
            biome_map.base_map.insert(pos, (biome_def.0, biome_def.1.to_owned()));
        }
    }

    /// Generates a single biome at `pos`.
    pub fn generate_biome(&mut self, pos: &AbsChunkPos, biome_map: &mut BiomeMap, registry: &BiomeRegistry, noises: &Noises) -> (RegistryId, BiomeDefinition) {
        let biome_def: (RegistryId, &BiomeDefinition) = BiomeGenerator::pick_biome(*pos, RelChunkPos::splat(0), &biome_map, registry, noises);
        //biome_map.base_map.insert(*pos, (biome_def.0, biome_def.1.to_owned()));
        (biome_def.0, biome_def.1.to_owned())
    }

    /// Sets the seed of this biome generator.
    pub fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
    }
    
    /// Gets the seed of this biome generator.
    pub fn seed(&self) -> u64 {
        self.seed
    } 
}
