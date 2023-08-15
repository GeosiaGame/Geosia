//! Random biome picker

use crate::{voxel::biome::{BiomeRegistry, biome_map::BiomeMap, BiomeEntry}, coordinates::{AbsChunkRange, AbsChunkPos, RelChunkPos}, registry::RegistryId};
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
        if fl < 0.0 {
            VPMoisture::Deadland
        } else if fl < 0.2 {
            VPMoisture::Desert
        } else if fl < 0.4 {
            VPMoisture::LowMoist
        } else if fl < 0.6 {
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

    fn pick_biome(&mut self, center: AbsChunkPos, pos: RelChunkPos, map: &BiomeMap, registry: &BiomeRegistry, noises: &Noises) -> BiomeEntry {
        let get_id = |id: RegistryId| registry.lookup_id_to_object(id);

        /*
        let nearby = map.get_biomes_near(center + pos);
        if nearby.iter().any(|e| e.is_some()) {
            let center_chunk = nearby.get(1 + 1 * 3 + 1 * 3 * 3).unwrap();
            if center_chunk.is_some() {
                let chunk_size = get_id(center_chunk.unwrap().id).unwrap().size_chunks;
                if (chunk_size * chunk_size) as i32 <= (center + pos).length_squared() {
                    return center_chunk.unwrap().clone();
                }
            }
        }
        */

        let pos_d = (center + pos).as_dvec3();
        let pos_d = [pos_d.x, pos_d.z];
        let height = noises.elevation_noise.get(pos_d);
        let wetness = noises.moisture_noise.get(pos_d);
        let temp = noises.temperature_noise.get(pos_d);

        let height = BiomeGenerator::get_elevation(height);
        let wetness = BiomeGenerator::get_moisture(wetness);
        let temp = BiomeGenerator::get_temperature(temp);

        let objects = registry.get_ids();
        for id in objects.iter() {
            let obj = get_id(**id);
            if obj.is_some() {
                let obj = obj.unwrap();
                if obj.elevation >= height &&/* obj.moisture >= wetness*/ && obj.temperature >= &&temp {
                    return BiomeEntry::new(**id);
                }
            }
        }
        BiomeEntry::new(**objects.get(self.rand.next_u32() as usize % (objects.len())).unwrap())
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
