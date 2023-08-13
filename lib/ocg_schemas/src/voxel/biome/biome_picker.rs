//! Random biome picker

use crate::{voxel::biome::{BiomeRegistry, biome_map::BiomeMap, BiomeEntry}, coordinates::{AbsChunkRange, AbsChunkPos, RelChunkPos}, registry::RegistryId};
use rand::{SeedableRng, RngCore};
use rand_xoshiro::Xoshiro256StarStar;

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

    fn pick_biome<'a>(&'a mut self, center: AbsChunkPos, pos: RelChunkPos, map: &BiomeMap, registry: &BiomeRegistry) -> BiomeEntry {
        let get_id = |id: RegistryId| registry.lookup_id_to_object(id);

        let nearby = map.get_biomes_near(center + pos);
        if nearby.iter().all(|e| e.is_some()) {
            let center_chunk = nearby.get(1 + 1 * 3).unwrap().unwrap();
            let chunk_size = get_id(center_chunk.id).unwrap().size_chunks;
            if (chunk_size * chunk_size) as i32 > (center + pos).length_squared() {
                return center_chunk.clone();
            }
        }

        let objects = registry.get_ids();
        BiomeEntry::new(**objects.get(self.rand.next_u32() as usize % (objects.len() + 1)).unwrap())
    }

    /// Gets biomes from a range of positions.
    pub fn generate_area_biomes<'a>(&'a mut self, area: AbsChunkRange, biome_map: &mut BiomeMap, registry: &'a BiomeRegistry) {
        let center = area.max() - RelChunkPos::from(area.min().into_ivec3() / 2);
        for pos in area.iter_xzy() {
            let biome_entry = self.pick_biome(center, pos.into(), &biome_map, registry);
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
