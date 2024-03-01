//! Random biome picker

use crate::{coordinates::{REGION_SIZE, REGION_SIZE2Z}, voxel::{biome::biome_map::BiomeMap, generation::Noise4DTo2D}};
use itertools::iproduct;
use serde::{Serialize, Deserialize};
use smallvec::{smallvec, SmallVec};

use super::{biome_map::{EXPECTED_BIOME_COUNT, GLOBAL_BIOME_SCALE}, BiomeEntry, Noises};

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

    fn pick_biome<'a>(pos: [i32; 2], map: &'a BiomeMap, noises: &mut Noises) -> SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]> {
        let pos_d = [pos[0] as f64 / GLOBAL_BIOME_SCALE, pos[1] as f64 / GLOBAL_BIOME_SCALE];
        let height = Self::map_range((0.0, 1.0), (0.0, 5.0), noises.elevation_noise.get_2d(pos_d));
        let moisture: f64 = Self::map_range((0.0, 1.0), (0.0, 5.0), noises.moisture_noise.get_2d(pos_d));
        let temperature = Self::map_range((0.0, 1.0), (0.0, 5.0), noises.temperature_noise.get_2d(pos_d));

        let mut biomes = smallvec![];

        for obj in map.generatable_biomes.iter() {
            let mut distance_elevation = (height + obj.1.elevation.min()) - obj.1.elevation.max();
            if distance_elevation < 0.0 {
                distance_elevation = distance_elevation.abs();
            }
            let mut distance_moisture = (moisture + obj.1.moisture.min()) - obj.1.moisture.max();
            if distance_moisture < 0.0 {
                distance_moisture = distance_moisture.abs();
            }
            let mut distance_temperature = (temperature + obj.1.temperature.min()) - obj.1.temperature.max();
            if distance_temperature < 0.0 {
                distance_temperature = distance_temperature.abs();
            }

            let average_distance = 1.0 / ((distance_elevation + distance_moisture + distance_temperature) / 3.0);//Self::map_range((0.0, 5.0), (0.0, 1.0), (distance_elevation + distance_moisture + distance_temperature) / 3.0);
            biomes.push(BiomeEntry { id: obj.0, weight: average_distance });
        }
        biomes //biomes.unwrap_or_else(|| BiomeEntry::new(registry.lookup_name_to_object(PLAINS_BIOME_NAME.as_ref()).unwrap().0))
    }

    /// Generates a region of biomes.
    pub fn generate_region(&mut self, region_x: i32, region_z: i32, biome_map: &mut BiomeMap, noises: &mut Noises) -> Vec<SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>> {
        let mut biomes = vec![smallvec![]; REGION_SIZE2Z];
        for (rx, rz) in iproduct!(0..REGION_SIZE, 0..REGION_SIZE) {
            let x = rx + (region_x * REGION_SIZE);
            let z = rz + (region_z * REGION_SIZE);

            let biome;
            let pos = [x, z];
            if biome_map.map.contains_key(&pos) {
                biome = biome_map.map.get(&pos).unwrap().to_owned();
            } else {
                biome = BiomeGenerator::pick_biome(pos, biome_map, noises);
                biome_map.map.insert(pos, biome.clone());
            }

            biomes[(rx + rz * REGION_SIZE) as usize] = biome;
        }
        biomes
    }
}
