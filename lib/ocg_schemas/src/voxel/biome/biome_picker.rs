//! Random biome picker

use crate::{voxel::biome::{BiomeRegistry, biome_map::BiomeMap}, coordinates::{AbsChunkRange, AbsChunkPos, RelChunkPos, InChunkRange, CHUNK_DIM, RelBlockPos, AbsBlockPos}, registry::RegistryId};
use noise::NoiseFn;
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256StarStar;
use serde::{Serialize, Deserialize};

use super::{Noises, VPTemperature, VPMoisture, VPElevation, BiomeDefinition, VOID_BIOME_NAME};

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

    fn get_temperature(fl: f64) -> VPTemperature {
        if fl < 0.0 {
            VPTemperature::Freezing(fl)
        } else if fl < 0.2 {
            VPTemperature::LowTemp(fl)
        } else if fl < 0.4 {
            VPTemperature::MedTemp(fl)
        } else if fl < 0.6 {
            VPTemperature::HiTemp(fl)
        } else {
            VPTemperature::Desert(fl)
        }
    }

    fn get_moisture(fl: f64) -> VPMoisture {
        if fl < 0.05 {
            VPMoisture::Deadland(fl)
        } else if fl < 0.3 {
            VPMoisture::Desert(fl)
        } else if fl < 0.55 {
            VPMoisture::LowMoist(fl)
        } else if fl < 0.8 {
            VPMoisture::MedMoist(fl)
        } else {
            VPMoisture::HiMoist(fl)
        }
    }

    fn get_elevation(fl: f64) -> VPElevation {
        if fl < 0.15 {
            VPElevation::Ocean(fl)
        } else if fl < 0.4 {
            VPElevation::LowLand(fl)
        } else if fl < 0.65 {
            VPElevation::Hill(fl)
        } else {
            VPElevation::Mountain(fl)
        }
    }

    fn pick_biome<'a>(center: AbsBlockPos, pos: RelBlockPos, _map: &BiomeMap, registry: &'a BiomeRegistry, noises: &Noises) -> (RegistryId, &'a BiomeDefinition) {
        let pos_d = (center + pos).as_dvec3();
        let pos_d = [pos_d.x, pos_d.z];
        let height = noises.elevation_noise.get(pos_d);
        let wetness = noises.moisture_noise.get(pos_d);
        let temp = noises.temperature_noise.get(pos_d);

        let mut final_id: Option<(RegistryId, &BiomeDefinition)> = None;

        let objects = registry.get_objects_ids();
        for id in objects.iter() {
            if let Some(obj) = id {
                let id = obj.0;
                let obj = obj.1;
                if obj.elevation >= height &&/* obj.moisture >= wetness*/ obj.temperature >= temp {
                    final_id = Some((*id, obj));
                    break;
                }
            }
        }
        final_id.unwrap_or_else(|| registry.lookup_name_to_object(VOID_BIOME_NAME.as_ref()).unwrap())
    }

    /// Gets biomes from a range of positions.
    pub fn generate_area_biomes(&mut self, area: AbsChunkRange, biome_map: &mut BiomeMap, registry: &BiomeRegistry, noises: &Noises, ) {
        let center = area.max() - RelChunkPos::from(area.min().into_ivec3() / 2);
        for pos in area.iter_xzy() {
            for b_pos in InChunkRange::WHOLE_CHUNK.iter_xzy() {
                let biome_def = BiomeGenerator::pick_biome(AbsBlockPos::from_ivec3(center.0 * CHUNK_DIM), RelBlockPos::from_ivec3(b_pos.0 + pos.0), &biome_map, registry, noises);
                biome_map.base_map.insert(AbsBlockPos::from_ivec3(b_pos.0 + pos.into_ivec3() * CHUNK_DIM), (biome_def.0, biome_def.1.to_owned()));
                
                let ret_val = self.base_map.get(pos);
                if let Some(map) = biome_map {
                    map[(pos.x + pos.z * PADDED_REGION_SIZE) as usize] = &ret_val.unwrap().1;
                }
            }
        }
    }

    pub fn generate_biome(&mut self, pos: &AbsBlockPos, biome_map: &mut BiomeMap, registry: &BiomeRegistry, noises: &Noises) {
        let biome_def: (RegistryId, &BiomeDefinition) = BiomeGenerator::pick_biome(*pos, RelBlockPos::splat(0), &biome_map, registry, noises);
        biome_map.base_map.insert(*pos, (biome_def.0, biome_def.1.to_owned()));
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
