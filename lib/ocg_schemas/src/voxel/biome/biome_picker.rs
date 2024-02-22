//! Random biome picker

use crate::{voxel::biome::{BiomeRegistry, biome_map::BiomeMap}, registry::RegistryId};
use itertools::iproduct;
use serde::{Serialize, Deserialize};

use super::{Noises, BiomeDefinition, PLAINS_BIOME_NAME, biome_map::{BLEND_RADIUS, SUPERGRID_DIM, PADDED_REGION_SIZE, PADDED_REGION_SIZE_SQZ, GLOBAL_BIOME_SCALE}};

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

    fn pick_biome<'a>(pos: [i32; 2], map: &'a BiomeMap, registry: &'a BiomeRegistry, noises: &mut Noises) -> (RegistryId, &'a BiomeDefinition) {
        let pos_d = [pos[0] as f64 / GLOBAL_BIOME_SCALE, pos[1] as f64 / GLOBAL_BIOME_SCALE];
        let height = Self::map_range((-0.9, 0.9), (0.0, 5.0), (noises.elevation_noise)(&mut noises.base_noise, pos_d));
        let wetness = Self::map_range((-0.9, 0.9), (0.0, 5.0), (noises.moisture_noise)(&mut noises.base_noise, pos_d));
        let temp = Self::map_range((-0.9, 0.9), (0.0, 5.0), (noises.temperature_noise)(&mut noises.base_noise, pos_d));

        let mut final_id = None;

        for obj in map.gen_biomes.iter() {
            if obj.1.elevation.contains(height) && obj.1.moisture.contains(wetness) && obj.1.temperature.contains(temp) {
                final_id = Some((obj.0, &obj.1));
                break;
            }
        }
        final_id.unwrap_or_else(|| registry.lookup_name_to_object(PLAINS_BIOME_NAME.as_ref()).unwrap())
    }

    /// Generates a single biome at `pos`.
    pub fn generate_biome(pos: [i32; 2], biome_map: &mut BiomeMap, registry: &BiomeRegistry, noises: &mut Noises) -> (RegistryId, BiomeDefinition) {
        let biome_def = BiomeGenerator::pick_biome(pos, &biome_map, registry, noises);
        //biome_map.base_map.insert(*pos, (biome_def.0, biome_def.1.to_owned()));
        (biome_def.0, biome_def.1.to_owned())
    }

    /// Generates a region of biomes.
    pub fn generate_region(&mut self, region_x: i32, region_z: i32, biome_map: &mut BiomeMap, registry: &BiomeRegistry, noises: &mut Noises) -> Vec<(RegistryId, BiomeDefinition)> {
        //let mut lock = stdout().lock();

        let mut biomes = vec![registry.lookup_name_to_object(PLAINS_BIOME_NAME.as_ref()).map(|x| (x.0, x.1.to_owned())).unwrap(); PADDED_REGION_SIZE_SQZ];
        for (rx, rz) in iproduct!(0..PADDED_REGION_SIZE, 0..PADDED_REGION_SIZE) {
            let x = (rx - BLEND_RADIUS) + (region_x * SUPERGRID_DIM);
            let z = (rz - BLEND_RADIUS) + (region_z * SUPERGRID_DIM);

            let biome;
            let pos = [x, z];
            if biome_map.base_map.contains_key(&pos) {
                biome = biome_map.base_map.get(&pos).unwrap().to_owned();
            } else {
                biome = BiomeGenerator::generate_biome(pos, biome_map, registry, noises);
                biome_map.base_map.insert(pos, biome.clone());
            }

            //writeln!(lock, "picked biome {0} for chunk [{x}, {z}]", biome.1).expect("Lock failed");
            biomes[(rx + rz * PADDED_REGION_SIZE) as usize] = biome;
        }
        biomes
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
