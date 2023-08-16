//! Default world generator

mod biome_blender;

use bevy::math::{ivec2, IVec2, DVec2};
use bevy_math::IVec3;
use lru::LruCache;
use noise::{NoiseFn, SuperSimplex};
use ocg_schemas::{voxel::{chunk_storage::{PaletteStorage, ChunkStorage}, voxeltypes::{BlockEntry, BlockDefinition}, biome::{VPElevation, VPMoisture, VPTemperature, BiomeDefinition, biome_map::BiomeMap, biome_picker::BiomeGenerator, Noises, BiomeEntry, BiomeRegistry}, generation::{fbm_noise::Fbm, Context, positional_random::PositionalRandomFactory}}, coordinates::{AbsChunkPos, InChunkPos, AbsChunkRange, AbsBlockPos, CHUNK_DIM2Z, CHUNK_DIM2}, registry::Registry, dependencies::itertools::iproduct};
use rand::prelude::*;
use rand_xoshiro::{Xoshiro256StarStar, rand_core::le};
use std::{cell::RefCell, rc::Rc};
use std::mem::MaybeUninit;
use thread_local::ThreadLocal;
use lazy_static::lazy_static;

use ocg_schemas::coordinates::{CHUNK_DIM, CHUNK_DIMZ};

use self::biome_blender::ScatteredBiomeBlender;

use super::biomes::PLAINS_BIOME_NAME;

const GLOBAL_SCALE_MOD: f64 = 2.0;
const GLOBAL_BIOME_SCALE: f64 = 256.0;
const SUPERGRID_SIZE: i32 = 4 * CHUNK_DIM;
type InCellRng = Xoshiro256StarStar;
type CellPointsT = [CellPoint; 4];

const BLEND_RADIUS: f64 = 24.0;
const MIN_PADDING_FOR_SCATTERED: f64 = 4.0;
const GRID_INTERVAL_EXPONENT: i32 = 3;
const GRID_INTERVAL: i32 = 1 << GRID_INTERVAL_EXPONENT;
lazy_static! {
    static ref GRID_EQUIVALENT_FREQUENCY: f64 = (1.0 / GRID_INTERVAL as f64) * 0.7598356856515925;
    static ref BLEND_RADIUS_PADDING: f64 = get_effective_scattered_blend_radius(*GRID_EQUIVALENT_FREQUENCY, true);
}

fn get_effective_scattered_blend_radius(frequency: f64, just_padding: bool) -> f64 {
        
    // Since the scattered blender has a minimum blend circle size, and the provided parameter is padding onto that,
    // try to generate a padding to achieve the desired blend radius internally. If this results in too low of a
    // padding value, use the defined minimum padding value instead.
    // Note that, in a real use case, the padding value will probably be tuned by a developer,
    // rather than mathematically generated to meet certain requirements.
    let internal_min_blend_radius = ScatteredBiomeBlender::get_internal_min_blend_radius_for_frequency(frequency);
    let mut blend_radius_padding = BLEND_RADIUS - internal_min_blend_radius;
    if blend_radius_padding < MIN_PADDING_FOR_SCATTERED {
        blend_radius_padding = MIN_PADDING_FOR_SCATTERED;
    }
    
    if just_padding {
        blend_radius_padding
    } else {
        blend_radius_padding + internal_min_blend_radius
    }
}

fn distance2(a: IVec2, b: IVec2) -> i32 {
    (a - b).length_squared()
}

#[derive(Clone, Copy, Debug)]
struct CellPoint {
    pos: IVec2,
}

impl CellPoint {
    pub fn calc(&mut self, cg: &mut CellGen) {

    }
}

impl Default for CellPoint {
    fn default() -> Self {
        Self {
            pos: ivec2(0, 0),
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
struct VoxelParams {
    height: i32,
}

struct CellGen {
    seed: u64,
    height_map_gen: [Fbm<SuperSimplex>; 5],
    elevation_map_gen: Fbm<SuperSimplex>,
    moisture_map_gen: Fbm<SuperSimplex>,
    temperature_map_gen: Fbm<SuperSimplex>,
    cell_points: LruCache<IVec2, CellPointsT>,
    nearest_buf: Vec<(i32, CellPoint)>,
}

impl CellGen {
    fn new(seed: u64) -> Self {
        let mut simplex = Fbm::<SuperSimplex>::new(0);
        let mut octaves = Vec::new();
        octaves.push(1.0);
        octaves.push(0.0);
        octaves.push(0.0);
        octaves.push(1.0);
        simplex = simplex.set_octaves(octaves);
        let mut s = Self {
            seed: 0,
            height_map_gen: [simplex.clone(), simplex.clone(), simplex.clone(), simplex.clone(), simplex.clone()], // init like this because Fbm can't implement copy
            elevation_map_gen: simplex.clone(),
            moisture_map_gen: simplex.clone(),
            temperature_map_gen: simplex.clone(),
            cell_points: LruCache::new(64.try_into().unwrap()),
            nearest_buf: Vec::with_capacity(16),
        };
        s.set_seed(seed);
        s
    }

    fn set_seed(&mut self, seed: u64) {
        use rand::prelude::*;
        use rand_xoshiro::SplitMix64;
        let mut sdgen = SplitMix64::seed_from_u64(seed);
        self.seed = seed;
        for hmg in self.height_map_gen.iter_mut() {
            hmg.set_seed(sdgen.next_u32());
        }
        Fbm::set_seed(&mut self.elevation_map_gen, sdgen.next_u32());
        Fbm::set_seed(&mut self.moisture_map_gen, sdgen.next_u32());
        Fbm::set_seed(&mut self.temperature_map_gen, sdgen.next_u32());
        self.cell_points.clear();
    }

    #[inline(always)]
    fn get_seed(&self, cell: IVec2) -> u64 {
        self.seed ^ (((cell.x as u64) << 32) | (cell.y as u64 & 0xFFFF_FFFF))
    }

    fn get_cell_points(&mut self, cell: IVec2) -> CellPointsT {
        if let Some(cp) = self.cell_points.get(&cell) {
            return *cp;
        }
        let mut pts: CellPointsT = Default::default();
        let mut r = InCellRng::seed_from_u64(self.get_seed(cell));
        for (i, (x, y)) in [
            (SUPERGRID_SIZE / 4, SUPERGRID_SIZE / 4),
            (3 * SUPERGRID_SIZE / 4, SUPERGRID_SIZE / 4),
            (0, 3 * SUPERGRID_SIZE / 4),
            (SUPERGRID_SIZE / 2, 3 * SUPERGRID_SIZE / 4),
        ]
        .iter()
        .enumerate()
        {
            const MOD: i32 = SUPERGRID_SIZE / 4;
            let xoff = (r.next_u32() % MOD as u32) as i32 - MOD / 2;
            let yoff = (r.next_u32() % MOD as u32) as i32 - MOD / 2;
            pts[i].pos = IVec2::new(
                cell.x * SUPERGRID_SIZE + *x + xoff,
                cell.y * SUPERGRID_SIZE + *y + yoff,
            );
            pts[i].calc(self);
        }
        self.cell_points.put(cell, pts);
        pts
    }

    fn find_nearest_cell_points(&mut self, pos: IVec2, num: usize) {
        let cell = pos / SUPERGRID_SIZE;

        type CP = (i32, CellPoint);
        self.nearest_buf.clear();
        for cdx in -1..=1 {
            for cdy in -1..=1 {
                for p in self.get_cell_points(cell + IVec2::new(cdx, cdy)).iter() {
                    let dist = distance2(p.pos, pos);
                    self.nearest_buf.push((dist, *p));
                }
            }
        }
        let cmp = |a: &CP, b: &CP| a.0.cmp(&b.0);
        if num == 1 {
            self.nearest_buf[0] = self.nearest_buf.iter().copied().min_by(cmp).unwrap();
        } else {
            self.nearest_buf.sort_by(cmp);
        }
        self.nearest_buf.resize(num, (0, CellPoint::default()));
    }

    fn elevation_noise(&self, pos: IVec2, c_pos: IVec2, biome_registry: &BiomeRegistry, mut biome_blender: impl FnMut(IVec2) -> BiomeEntry) -> f64 {
        let nf = |p: DVec2, b: &BiomeDefinition| (b.surface_noise.get([p.x, p.y]) + 1.0) / 2.0;
        let scale_factor = GLOBAL_BIOME_SCALE * GLOBAL_SCALE_MOD;
        let mut height: f64 = 0.0;
        let mut entry = Some(biome_blender(pos));
        while let Some(e) = entry {
            let mut weights = 1.0;
            if let Some(w) = e.get_weights() {
                let pos = pos - (c_pos * CHUNK_DIM);
                weights = w[(pos.x + pos.y * CHUNK_DIM) as usize];
            }
            height += nf(pos.as_dvec2() / scale_factor, e.lookup(biome_registry).unwrap()) * weights;
            entry = Rc::unwrap_or_clone(e.next);
        }
        height
    }

    fn calc_voxel_params(&mut self, pos: IVec2, c_pos: IVec2, biome_registry: &BiomeRegistry, biome_blender: impl FnMut(IVec2) -> BiomeEntry) -> i32 {
        self.find_nearest_cell_points(pos, 1);

        let height = self.elevation_noise(pos, c_pos, biome_registry, biome_blender);

        height.round() as i32
    }
}

impl Default for CellGen {
    fn default() -> Self {
        Self::new(0)
    }
}

pub struct StdGenerator {
    seed: u64,
    biome_gen: BiomeGenerator,
    biome_map: BiomeMap,
    biome_blender: RefCell<ScatteredBiomeBlender>,
    noises: Noises,
    cell_gen: ThreadLocal<RefCell<CellGen>>,
}

impl Default for StdGenerator {
    fn default() -> Self {
        Self::new(0, BiomeMap::default(), BiomeGenerator::new(0))
    }
}

impl StdGenerator {
    pub fn new(seed: u64, biome_map: BiomeMap, biome_generator: BiomeGenerator) -> Self {
        Self {
            seed,
            biome_gen: biome_generator,
            biome_map: biome_map,
            biome_blender: RefCell::new(ScatteredBiomeBlender::new(*GRID_EQUIVALENT_FREQUENCY, *BLEND_RADIUS_PADDING)),
            noises: Noises {
                elevation_noise: Box::new(Fbm::<SuperSimplex>::new(1).set_octaves(vec![1.0, 1.0, 1.0, 1.0])),
                temperature_noise: Box::new(Fbm::<SuperSimplex>::new(2).set_octaves(vec![1.0, 1.0, 1.0, 1.0])),
                moisture_noise: Box::new(Fbm::<SuperSimplex>::new(3).set_octaves(vec![1.0, 1.0, 1.0, 1.0])),
            },
            cell_gen: ThreadLocal::default(),
        }
    }

    pub fn generate_area_biome_map(&mut self, area: AbsChunkRange, biome_registry: &Registry<BiomeDefinition>) {
        self.biome_gen.generate_area_biomes(area, &mut self.biome_map, biome_registry, &self.noises);
    }

    pub fn generate_chunk(&mut self, c_pos: AbsChunkPos, chunk: &mut PaletteStorage<BlockEntry>, block_registry: &Registry<BlockDefinition>, biome_registry: &Registry<BiomeDefinition>) {
        let mut cellgen = self
            .cell_gen
            .get_or(|| RefCell::new(CellGen::new(self.seed)))
            .borrow_mut();

        let vparams: [i32; CHUNK_DIMZ * CHUNK_DIMZ] = {
            let mut vparams: [MaybeUninit<i32>; CHUNK_DIMZ * CHUNK_DIMZ] =
                unsafe { std::mem::MaybeUninit::uninit().assume_init() };
            for (i, v) in vparams[..].iter_mut().enumerate() {
                let x = (i % CHUNK_DIMZ) as i32 + (c_pos.x * CHUNK_DIM);
                let z = ((i / CHUNK_DIMZ) % CHUNK_DIMZ) as i32 + (c_pos.z * CHUNK_DIM);
                let p = cellgen.calc_voxel_params(IVec2::new(x, z), IVec2::new(c_pos.x, c_pos.z), biome_registry, |pos| self.biome_blender.borrow_mut().get_blend_for_block(self.seed, pos.x, pos.y, biome_registry, |x, z| self.biome_map.get_or_new(&AbsChunkPos::new(x as i32, 0, z as i32), &mut self.biome_gen, biome_registry, &self.noises).map(|x| x.to_owned()).unwrap_or_else(|| biome_registry.lookup_name_to_object(PLAINS_BIOME_NAME.as_ref()).map(|f| (f.0, f.1.to_owned())).unwrap())));
                //|b_pos: IVec2| self.biome_blender.get_blend_for_block(self.seed, (b_pos.x + (c_pos.x * CHUNK_DIM)) as f64, (b_pos.y + (c_pos.z * CHUNK_DIM)) as f64, biome_registry, |x, z| {
                //    let key = &AbsBlockPos::new(x as i32, 0, z as i32);
                //    if biomemap.contains_key(key) {
                //        Some(biomemap.get(key).unwrap().id)
                //    } else {
                //        None
                //    }
                //}));
                unsafe {
                    std::ptr::write(v.as_mut_ptr(), p);
                }
            }
            unsafe { std::mem::transmute(vparams) }
        };

        let mut biomegen_cloned = self.biome_gen;
        let current_biome = self.biome_map.get_or_new(&c_pos, &mut biomegen_cloned, biome_registry, &self.noises).expect("Invalid biome at pos!");
        for (pos_x, pos_y, pos_z) in iproduct!(0..CHUNK_DIM, 0..CHUNK_DIM, 0..CHUNK_DIM) {
            let b_pos = InChunkPos::try_new(pos_x, pos_y, pos_z).unwrap();
            let g_pos = <IVec3>::from(b_pos) + (<IVec3>::from(c_pos) * CHUNK_DIM);
            let height = vparams[(pos_x + pos_z * (CHUNK_DIM)) as usize];

            let ctx = Context { biome_generator: &self.biome_gen, chunk: chunk, random: PositionalRandomFactory::default(), ground_y: height, sea_level: 0 /* hardcoded for now... */ };
            let result = current_biome.1.rule_source.place(&g_pos, &ctx, block_registry);
            if result.is_some() {
                chunk.put(b_pos, result.unwrap());
            }
        }
    }

}
