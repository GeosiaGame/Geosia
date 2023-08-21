//! Biome blender.

use std::cell::RefCell;

use lazy_static::lazy_static;
use ocg_schemas::{voxel::biome::{BiomeEntry, BiomeRegistry, BiomeDefinition, biome_map::{BLEND_RADIUS, BLEND_CIRCUMFERENCE, SUPERGRID_DIM, BiomeMap, SUPERGRID_DIM_EXPONENT, CHUNK_SIZE_EXPONENT, PADDED_REGION_SIZE}, biome_picker::BiomeGenerator, Noises}, coordinates::{CHUNK_DIM, CHUNK_DIM2Z}, registry::RegistryId, dependencies::{smallvec::SmallVec, itertools::iproduct}};

pub const CACHE_MAX_ENTRIES: i32 = 24;

lazy_static! {

    static ref BLUR_KERNEL: [f64; ((BLEND_CIRCUMFERENCE) * (BLEND_CIRCUMFERENCE)) as usize] = {
		let mut weight_total = 0.0;
        let mut ret_val = [0.0; ((BLEND_CIRCUMFERENCE) * (BLEND_CIRCUMFERENCE)) as usize];
		for iz in 0..BLEND_CIRCUMFERENCE {
			let idz = iz - BLEND_RADIUS;
			for ix in 0..BLEND_CIRCUMFERENCE {
				let idx = ix - BLEND_RADIUS;
				let mut this_weight = BLEND_RADIUS * BLEND_RADIUS - idx * idx - idz * idz;
				if this_weight <= 0 { // We only compute for the circle of positive values of the blending function.
                    continue;
                }
				this_weight *= this_weight; // Make transitions smoother.
				weight_total += this_weight as f64;
				ret_val[(ix + iz * (BLEND_CIRCUMFERENCE)) as usize] = this_weight as f64;
			}
		}
		
		// Rescale the weights, so they all add up to 1.
		for i in 0..ret_val.len() {
            ret_val[i] /= weight_total;
        }
        ret_val
    };
}

pub struct SimpleBiomeBlender {
    pub biome_map_cache: Vec<BiomeCacheEntry>
}

impl SimpleBiomeBlender {
    pub fn new() -> Self {
        Self {
            biome_map_cache: vec![],
        }
    }

    pub fn get_blended_for_chunk(&mut self, chunk_x: i32, chunk_z: i32, biome_map: &mut BiomeMap, generator: &mut RefCell<BiomeGenerator>, registry: &BiomeRegistry, noises: &Noises) -> SmallVec<[SmallVec<[BiomeEntry; 3]>; CHUNK_DIM2Z]> {
        let region_x = chunk_x >> (SUPERGRID_DIM_EXPONENT - CHUNK_SIZE_EXPONENT);
        let region_z = chunk_z >> (SUPERGRID_DIM_EXPONENT - CHUNK_SIZE_EXPONENT);
        let biomes = self.get_biomes_for_region(region_x, region_z, biome_map, generator, registry, noises);

        let mut chunk_results: SmallVec<[SmallVec<[BiomeEntry; 3]>; CHUNK_DIM2Z]> = SmallVec::new();
        chunk_results.resize(CHUNK_DIM2Z, SmallVec::default());

        for (cx, cz) in iproduct!(0..CHUNK_DIM, 0..CHUNK_DIM) {
            SimpleBiomeBlender::get_blended_from_region((chunk_x * CHUNK_DIM) + cx, (chunk_z * CHUNK_DIM) + cz, biome_map, biomes, &mut chunk_results[(cx + cz * CHUNK_DIM) as usize]);
        }
        chunk_results
    }

    pub fn get_blended_from_region(x: i32, z: i32, biome_map: &mut BiomeMap, biomes: &Vec<(RegistryId, BiomeDefinition)>, results: &mut SmallVec<[BiomeEntry; 3]>) {
		// Mod the world coordinate by the region size.
		let x_masked = x & (SUPERGRID_DIM - 1);
		let z_masked = z & (SUPERGRID_DIM - 1);

        for (ix, iz) in iproduct!(0..BLEND_CIRCUMFERENCE, 0..BLEND_CIRCUMFERENCE) {
            let this_weight = BLUR_KERNEL[(ix + iz * BLEND_CIRCUMFERENCE) as usize];
            if this_weight <= 0.0 {
                continue;
            }
            
            let this_biome = &biomes[((x_masked + ix) + ((z_masked + iz) * PADDED_REGION_SIZE)) as usize];

            let mut found_entry = false;
            for entry in results.iter_mut() {
                if entry.id == this_biome.0 {
                    entry.weight += this_weight;
                    found_entry = true;
                    break;
                }
            }

            if !found_entry {
                let mut entry = BiomeEntry::new(this_biome.0);
                entry.weight = this_weight;
                results.push(entry);
            }
        }
        biome_map.final_map.insert([x, z], results.clone());
    }

    fn get_biomes_for_region(&mut self, region_x: i32, region_z: i32, biome_map: &mut BiomeMap, generator: &mut RefCell<BiomeGenerator>, registry: &BiomeRegistry, noises: &Noises) -> &Vec<(RegistryId, BiomeDefinition)> {
        let mut correct_cache_entry: Option<BiomeCacheEntry> = None;
        self.biome_map_cache.retain(|obj| {
            if obj.region_x == region_x && obj.region_z == region_z {
                correct_cache_entry = Some(obj.to_owned());
                return false;
            }
            return true;
        });

        if correct_cache_entry.is_none() {
            let mut entry = BiomeCacheEntry::new(region_x, region_z);
            entry.cache = Some(generator.borrow_mut().generate_region(region_x, region_z, biome_map, registry, noises));
            correct_cache_entry = Some(entry);
        }

        self.biome_map_cache.insert(0, correct_cache_entry.unwrap());

        if self.biome_map_cache.len() > CACHE_MAX_ENTRIES as usize {
            self.biome_map_cache.remove(self.biome_map_cache.len() - 1);
        }

        self.biome_map_cache.get(0).unwrap().cache.as_ref().unwrap()
    }
}

#[derive(Clone)]
pub struct BiomeCacheEntry {
    pub cache: Option<Vec<(RegistryId, BiomeDefinition)>>,
    region_x: i32,
    region_z: i32,
}

impl BiomeCacheEntry {
    pub fn new(region_x: i32, region_z: i32) -> Self {
        Self {
            region_x: region_x,
            region_z: region_z,
            cache: None,
        }
    }
}
