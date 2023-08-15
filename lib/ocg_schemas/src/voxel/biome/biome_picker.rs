//! Random biome picker

use crate::{voxel::biome::{BiomeRegistry, biome_map::BiomeMap, BiomeEntry}, coordinates::{AbsChunkRange, AbsChunkPos, RelChunkPos, InChunkRange, CHUNK_DIM, AbsBlockPos}, registry::RegistryId};
use bevy_math::IVec3;
use noise::NoiseFn;
use rand::{SeedableRng, RngCore};
use rand_xoshiro::Xoshiro256StarStar;

use super::{Noises, VPTemperature, VPMoisture, VPElevation};

/// The generic biome selector.
pub struct BiomeGenerator {
    seed: u64,
    rand: Xoshiro256StarStar,
}

impl Default for BiomeGenerator {
    fn default() -> Self {
        Self { 
            seed: 0,
            rand: Xoshiro256StarStar::seed_from_u64(0),
        }
    }
}

impl BiomeGenerator {
    /// Helper function to create a new, seeded BiomeGenerator.
    pub fn new(seed: u64) -> Self {
        Self {
            seed: seed,
            rand: Xoshiro256StarStar::seed_from_u64(seed),
        }
    }

    fn get_temperature(fl: f64) -> VPTemperature {
        if fl < 0.0 {
            VPTemperature::Freezing
        } else if fl < 0.2 {
            VPTemperature::LowTemp
        } else if fl < 0.4 {
            VPTemperature::MedTemp
        } else if fl < 0.6 {
            VPTemperature::HiTemp
        } else {
            VPTemperature::Desert
        }
    }

    fn get_moisture(fl: f64) -> VPMoisture {
        if fl < 0.05 {
            VPMoisture::Deadland
        } else if fl < 0.3 {
            VPMoisture::Desert
        } else if fl < 0.55 {
            VPMoisture::LowMoist
        } else if fl < 0.8 {
            VPMoisture::MedMoist
        } else {
            VPMoisture::HiMoist
        }
    }

    fn get_elevation(fl: f64) -> VPElevation {
        if fl < 0.35 {
            VPElevation::Ocean
        } else if fl < 0.5 {
            VPElevation::LowLand
        } else if fl < 0.75 {
            VPElevation::Hill
        } else {
            VPElevation::Mountain
        }
    }

    fn pick_biome(&mut self, center: AbsChunkPos, pos: RelChunkPos, _map: &BiomeMap, registry: &BiomeRegistry, noises: &Noises) -> BiomeEntry {
        let get_id = |id: RegistryId| registry.lookup_id_to_object(id);

        let pos_d = (center + pos).as_dvec3();
        let pos_d = [pos_d.x, pos_d.z];
        let height = noises.elevation_noise.get(pos_d);
        let wetness = noises.moisture_noise.get(pos_d);
        let temp = noises.temperature_noise.get(pos_d);

        let height = BiomeGenerator::get_elevation(height);
        let moisture = BiomeGenerator::get_moisture(wetness);
        let temp = BiomeGenerator::get_temperature(temp);

        let mut final_id: RegistryId = *registry.get_ids()[0];

        let objects = registry.get_ids();
        for id in objects.iter() {
            let obj = get_id(**id);
            if obj.is_some() {
                let obj = obj.unwrap();
                if obj.elevation >= height &&/* obj.moisture >= wetness*/ obj.temperature >= temp {
                    final_id = **id;
                    break;
                }
            }
        }
        BiomeEntry::new_base(final_id, 1.0)
    }

    /// Gets biomes from a range of positions.
    pub fn generate_area_biomes<'a>(&'a mut self, area: AbsChunkRange, biome_map: &mut BiomeMap, registry: &'a BiomeRegistry, noises: &Noises) {
        let center = area.max() - RelChunkPos::from(area.min().into_ivec3() / 2);
        for pos in area.iter_xzy() {
            let biome_entry = self.pick_biome(center, pos.into(), &biome_map, registry, noises);
            biome_map.insert(pos, biome_entry);
        }
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
