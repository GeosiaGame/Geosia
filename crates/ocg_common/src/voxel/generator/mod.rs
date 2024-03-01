//! Default world generator

use bevy::{math::{IVec2, DVec2}, prelude::ResMut};
use bevy_math::IVec3;
use noise::OpenSimplex;
use ocg_schemas::{coordinates::{AbsChunkPos, InChunkPos, CHUNK_DIM2Z}, dependencies::{itertools::iproduct, smallvec::SmallVec}, registry::RegistryId, voxel::{biome::{biome_map::{BiomeMap, CHUNK_SIZE_EXPONENT, EXPECTED_BIOME_COUNT, GLOBAL_BIOME_SCALE, GLOBAL_SCALE_MOD, REGION_SIZE_EXPONENT}, biome_picker::BiomeGenerator, BiomeDefinition, BiomeEntry, BiomeRegistry, Noises}, chunk_storage::{ChunkStorage, PaletteStorage}, generation::{fbm_noise::Fbm, positional_random::PositionalRandomFactory, Context}, voxeltypes::{BlockEntry, BlockRegistry}}};
use std::cell::RefCell;
use std::mem::MaybeUninit;
use thread_local::ThreadLocal;

use ocg_schemas::coordinates::{CHUNK_DIM, CHUNK_DIMZ};

pub mod newgen;

pub const WORLD_SIZE_XZ: i32 = 8;
pub const WORLD_SIZE_Y: i32 = 8;

struct CellGen {
    seed: u64,
}

impl CellGen {
    fn new(seed: u64, biome_map: &mut BiomeMap, biome_registry: &BiomeRegistry) -> Self {
        let mut s = Self {
            seed: 0,
        };
        s.set_seed(seed, biome_map, biome_registry);
        s
    }
    
    fn set_seed(&mut self, seed: u64, biome_map: &mut BiomeMap, biome_registry: &BiomeRegistry) {
        self.seed = seed;
        let mut biomes: Vec<(RegistryId, BiomeDefinition)> = Vec::new();
        for def in biome_registry.get_objects_ids().iter() {
            if def.1.can_generate {
                biomes.push((*def.0, def.1.to_owned()));
            }
        }
        biome_map.generatable_biomes = biomes;
    }

    fn elevation_noise(&self, in_chunk_pos: IVec2, chunk_pos: IVec2, biome_registry: &BiomeRegistry, blended: &Vec<SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>>, noises: &mut Noises) -> f64 {
        let mut nf = |p: DVec2, b: &BiomeDefinition| ((b.surface_noise)(p, &mut noises.base_terrain_noise) + 1.0) / 2.0;
        let scale_factor = GLOBAL_BIOME_SCALE * GLOBAL_SCALE_MOD;
        let blend = &blended[(in_chunk_pos.x + in_chunk_pos.y * CHUNK_DIM) as usize];
        let global_pos = DVec2::new((in_chunk_pos.x + (chunk_pos.x * CHUNK_DIM)) as f64, (in_chunk_pos.y + (chunk_pos.y * CHUNK_DIM)) as f64);

        let mut heights = 0.0;
        let mut weights = 0.0;
        for entry in blend.iter() {
            let biome = entry.lookup(biome_registry).unwrap();
            let noise = nf(global_pos / scale_factor, biome);
            let strength = (biome.blend_influence - entry.weight) / biome.blend_influence;
            heights += noise * strength;
            weights += strength;
        }
        heights / weights
    }
}

pub struct StdGenerator<'a> {
    seed: u64,
    biome_gen: RefCell<BiomeGenerator>,
    biome_map: ResMut<'a, BiomeMap>,
    //biome_blender: SimpleBiomeBlender,
    noises: Noises,
    cell_gen: ThreadLocal<RefCell<CellGen>>
}

impl<'a> StdGenerator<'a> {
    pub fn new(seed: u64, biome_map: ResMut<'a, BiomeMap>, biome_generator: BiomeGenerator) -> Self {
        let seed_int = seed as u32;
        Self {
            seed,
            biome_gen: RefCell::new(biome_generator),
            biome_map: biome_map,
            //biome_blender: SimpleBiomeBlender::new(),
            noises: Noises {
                base_terrain_noise: Box::new(Fbm::<OpenSimplex>::new(seed_int).set_octaves(vec![-4.0, 1.0, 1.0, 0.0])),
                elevation_noise: Box::new(Fbm::<OpenSimplex>::new(seed_int + 1).set_octaves(vec![1.0, 2.0, 2.0, 1.0])),
                temperature_noise: Box::new(Fbm::<OpenSimplex>::new(seed_int + 2).set_octaves(vec![1.0, 2.0, 2.0, 1.0])),
                moisture_noise: Box::new(Fbm::<OpenSimplex>::new(seed_int + 3).set_octaves(vec![1.0, 2.0, 2.0, 1.0])),
            },
            cell_gen: ThreadLocal::new(),
        }
    }

    pub fn generate_chunk(&mut self, c_pos: AbsChunkPos, chunk: &mut PaletteStorage<BlockEntry>, block_registry: &BlockRegistry, biome_registry: &BiomeRegistry) {
        let cellgen = self
            .cell_gen
            .get_or(|| RefCell::new(CellGen::new(self.seed, &mut self.biome_map, biome_registry)))
            .borrow_mut();
        
        //let blended = self.biome_blender.get_blended_for_chunk(c_pos.x, c_pos.z, &mut self.biome_map, &mut self.biome_gen, biome_registry, &mut self.noises);
        let region_x = c_pos.x >> (REGION_SIZE_EXPONENT - CHUNK_SIZE_EXPONENT);
        let region_z = c_pos.z >> (REGION_SIZE_EXPONENT - CHUNK_SIZE_EXPONENT);
        let blended = self.biome_gen.borrow_mut().generate_region(region_x, region_z, &mut self.biome_map, &mut self.noises);

        let vparams: [i32; CHUNK_DIM2Z] = {
            let mut vparams: [MaybeUninit<i32>; CHUNK_DIM2Z] =
                unsafe { std::mem::MaybeUninit::uninit().assume_init() };
            for (i, v) in vparams[..].iter_mut().enumerate() {
                let in_chunk_x = (i % CHUNK_DIMZ) as i32;
                let in_chunk_z = ((i / CHUNK_DIMZ) % CHUNK_DIMZ) as i32;
                let p = cellgen.elevation_noise(IVec2::new(in_chunk_x, in_chunk_z), IVec2::new(c_pos.x, c_pos.z), biome_registry, &blended, &mut self.noises).round() as i32;
                unsafe {
                    std::ptr::write(v.as_mut_ptr(), p);
                }
            }
            unsafe { std::mem::transmute(vparams) }
        };

        for (pos_x, pos_y, pos_z) in iproduct!(0..CHUNK_DIM, 0..CHUNK_DIM, 0..CHUNK_DIM) {
            let b_pos = InChunkPos::try_new(pos_x, pos_y, pos_z).unwrap();

            let g_pos = <IVec3>::from(b_pos) + (<IVec3>::from(c_pos) * CHUNK_DIM);
            let height = vparams[(pos_x + pos_z * CHUNK_DIM) as usize];

            let mut biomes: SmallVec<[(&BiomeDefinition, f64); 3]> = SmallVec::new();
            for b in blended[(pos_x + pos_z * CHUNK_DIM) as usize].iter() {
                let e = b.lookup(biome_registry).unwrap();
                let w = b.weight * e.block_influence;
                biomes.push((e, w));
            }
            // sort by block influence, then registry id if influence is same
            biomes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or_else(|| biome_registry.lookup_object_to_id(a.0).cmp(&biome_registry.lookup_object_to_id(b.0))));

            for (biome, _) in biomes.iter() {
                let ctx = Context { chunk: chunk, random: PositionalRandomFactory::default(), ground_y: height, sea_level: 0 /* hardcoded for now... */ };
                let result = (biome.rule_source)(&g_pos, &ctx, &block_registry);
                if result.is_some() {
                    chunk.put(b_pos, result.unwrap());
                }
            }
        }
    }

}
