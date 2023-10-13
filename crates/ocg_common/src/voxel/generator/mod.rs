//! Default world generator

mod biome_blender;

use bevy::{math::{IVec2, DVec2}, prelude::ResMut};
use bevy_math::IVec3;
use noise::SuperSimplex;
use ocg_schemas::{voxel::{chunk_storage::{PaletteStorage, ChunkStorage}, voxeltypes::{BlockEntry, BlockRegistry}, biome::{BiomeDefinition, biome_map::{BiomeMap, GLOBAL_BIOME_SCALE, GLOBAL_SCALE_MOD}, biome_picker::BiomeGenerator, Noises, BiomeRegistry, BiomeEntry}, generation::{fbm_noise::Fbm, Context, positional_random::PositionalRandomFactory}}, coordinates::{AbsChunkPos, InChunkPos, CHUNK_DIM2Z}, dependencies::{itertools::iproduct, smallvec::SmallVec}, registry::RegistryId};
use std::cell::RefCell;
use std::mem::MaybeUninit;
use thread_local::ThreadLocal;

use ocg_schemas::coordinates::{CHUNK_DIM, CHUNK_DIMZ};

use self::biome_blender::SimpleBiomeBlender;

struct CellGen {
    seed: u64,
}

impl CellGen {
    fn new(seed: u64, biome_map: &mut BiomeMap, biome_registry: &BiomeRegistry, noises: &mut Noises) -> Self {
        let mut s = Self {
            seed: 0,
        };
        s.set_seed(seed, biome_map, biome_registry, noises);
        s
    }
    
    fn set_seed(&mut self, seed: u64, biome_map: &mut BiomeMap, biome_registry: &BiomeRegistry, noises: &mut Noises) {
        use rand::prelude::*;
        use rand_xoshiro::SplitMix64;
        self.seed = seed;
        let mut sdgen = SplitMix64::seed_from_u64(seed);
        let mut biomes: Vec<(RegistryId, BiomeDefinition)> = Vec::new();
        /*for def in biome_registry.get_objects_ids().iter() {
            if def.1.can_generate {
                if let Some(seedable) = def.1.surface_noise.get_seedable().as_mut() {
                    seedable.set_seed(sdgen.next_u32());
                }
                biomes.push((*def.0, def.1.to_owned()));
            }
        }*/
        biome_map.gen_biomes = biomes;
        if let Some(seedable) = noises.elevation_noise.get_seedable().as_mut() {
            seedable.set_seed(sdgen.next_u32());
        }
        if let Some(seedable) = noises.temperature_noise.get_seedable().as_mut() {
            seedable.set_seed(sdgen.next_u32());
        }
        if let Some(seedable) = noises.moisture_noise.get_seedable().as_mut() {
            seedable.set_seed(sdgen.next_u32());
        }
    }

//    #[inline(always)]
//    fn get_seed(&self, cell: IVec2) -> u64 {
//        self.seed ^ (((cell.x as u64) << 32) | (cell.y as u64 & 0xFFFF_FFFF))
//    }

    fn elevation_noise(&self, pos: IVec2, c_pos: IVec2, biome_registry: &BiomeRegistry, blended: &SmallVec<[SmallVec<[BiomeEntry; 3]>; CHUNK_DIM2Z]>) -> f64 {
        let nf = |p: DVec2, b: &BiomeDefinition| (b.surface_noise.call([p.x, p.y], self.seed as u32) + 1.0) / 2.0;
        let scale_factor = GLOBAL_BIOME_SCALE * GLOBAL_SCALE_MOD;
        let in_c_pos = pos - (c_pos * CHUNK_DIM);
        let blend = &blended[(in_c_pos.x + in_c_pos.y * CHUNK_DIM) as usize];

        let mut h: f64 = 0.0;
        for entry in blend.iter() {
            h += nf((pos).as_dvec2() / scale_factor, entry.lookup(biome_registry).unwrap()) * entry.weight * entry.lookup(biome_registry).unwrap().blend_influence;
        }
        //println!("Height at pos {pos}: {h}");
        h
    }
}

pub struct StdGenerator<'a> {
    seed: u64,
    biome_gen: RefCell<BiomeGenerator>,
    biome_map: ResMut<'a, BiomeMap>,
    biome_blender: SimpleBiomeBlender,
    noises: Noises,
    cell_gen: ThreadLocal<RefCell<CellGen>>
}

impl<'a> StdGenerator<'a> {
    pub fn new(seed: u64, biome_map: ResMut<'a, BiomeMap>, biome_generator: BiomeGenerator) -> Self {
        Self {
            seed,
            biome_gen: RefCell::new(biome_generator),
            biome_map: biome_map,
            biome_blender: SimpleBiomeBlender::new(),
            noises: Noises {
                elevation_noise: Box::new(Fbm::<SuperSimplex>::new(1).set_octaves(vec![-3.0, 1.0, 1.0, 0.0])),
                temperature_noise: Box::new(Fbm::<SuperSimplex>::new(2).set_octaves(vec![-3.0, 1.0, 1.0, 0.0])),
                moisture_noise: Box::new(Fbm::<SuperSimplex>::new(3).set_octaves(vec![-3.0, 1.0, 1.0, 0.0])),
            },
            cell_gen: ThreadLocal::new(),
        }
    }

    pub fn generate_chunk(&mut self, c_pos: AbsChunkPos, chunk: &mut PaletteStorage<BlockEntry>, block_registry: &BlockRegistry, biome_registry: &BiomeRegistry) {
        let cellgen = self
            .cell_gen
            .get_or(|| RefCell::new(CellGen::new(self.seed, &mut self.biome_map, biome_registry, &mut self.noises)))
            .borrow_mut();

        let biomegen = self
            .biome_gen.clone();

        let blended = self.biome_blender.get_blended_for_chunk(c_pos.x, c_pos.z, &mut self.biome_map, &mut self.biome_gen, biome_registry, &self.noises);

        let vparams: [i32; CHUNK_DIM2Z] = {
            let mut vparams: [MaybeUninit<i32>; CHUNK_DIM2Z] =
                unsafe { std::mem::MaybeUninit::uninit().assume_init() };
            for (i, v) in vparams[..].iter_mut().enumerate() {
                let x = (i % CHUNK_DIMZ) as i32 + (c_pos.x * CHUNK_DIM);
                let z = ((i / CHUNK_DIMZ) % CHUNK_DIMZ) as i32 + (c_pos.z * CHUNK_DIM);
                let p = cellgen.elevation_noise(IVec2::new(x, z), IVec2::new(c_pos.x, c_pos.z), biome_registry, &blended).round() as i32;
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

            //if g_pos.y - height < 0 {
            //    continue;
            //}

            let mut biomes: SmallVec<[(&BiomeDefinition, f64); 3]> = SmallVec::new();
            for b in blended[(pos_x + pos_z * CHUNK_DIM) as usize].iter() {
                let e = b.lookup(biome_registry).unwrap();
                let w = b.weight * e.block_influence;
                biomes.push((e, w));
            }
            biomes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            for (biome, _) in biomes.iter() {
                let ctx = Context { biome_generator: &biomegen.borrow_mut(), chunk: chunk, random: PositionalRandomFactory::default(), ground_y: height, sea_level: 0 /* hardcoded for now... */ };
                let result = biome.rule_source.call(&g_pos, &ctx, &block_registry);
                if result.is_some() {
                    chunk.put(b_pos, result.unwrap());
                }
            }
        }
    }

}
