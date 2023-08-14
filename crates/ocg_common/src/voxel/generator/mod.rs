//! Default world generator

use bevy::math::{ivec2, IVec2, DVec2};
use bevy_math::IVec3;
use lru::LruCache;
use noise::{NoiseFn, SuperSimplex};
use ocg_schemas::{voxel::{chunk_storage::{PaletteStorage, ChunkStorage}, voxeltypes::{BlockEntry, BlockDefinition, EMPTY_BLOCK_NAME}, biome::{VPElevation, VPMoisture, VPTemperature, BiomeDefinition, biome_map::BiomeMap, biome_picker::BiomeGenerator}, generation::{fbm_noise::Fbm, Context, positional_random::PositionalRandomFactory}}, coordinates::{AbsChunkPos, InChunkPos, AbsChunkRange}, registry::Registry, dependencies::itertools::iproduct};
use rand::prelude::*;
use rand_xoshiro::Xoshiro256StarStar;
use std::cell::RefCell;
use std::mem::MaybeUninit;
use thread_local::ThreadLocal;

use ocg_schemas::coordinates::{CHUNK_DIM, CHUNK_DIMZ};

use super::blocks::*;

const GLOBAL_SCALE_MOD: f64 = 2.0;
const GLOBAL_BIOME_SCALE: f64 = 256.0;
const SUPERGRID_SIZE: i32 = 4 * CHUNK_DIM;
type InCellRng = Xoshiro256StarStar;
type CellPointsT = [CellPoint; 4];

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
    elevation: VPElevation,
    moisture: VPMoisture,
    temperature: VPTemperature,
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

    fn elevation_noise(&self, pos: IVec2, biome: &BiomeDefinition) -> f64 {
        let nf = |p: DVec2| (biome.surface_noise.get([p.x, p.y]) + 1.0) / 2.0;
        let scale_factor = GLOBAL_BIOME_SCALE * GLOBAL_SCALE_MOD;
        nf(pos.as_dvec2() / scale_factor)
    }

    fn moisture_noise(&self, pos: IVec2) -> f64 {
        let nf = |p: DVec2| (self.moisture_map_gen.get([p.x, p.y]) + 1.0) / 2.0;
        let scale_factor = GLOBAL_BIOME_SCALE * GLOBAL_SCALE_MOD;
        nf(pos.as_dvec2() / scale_factor)
    }

    fn temperature_noise(&self, pos: IVec2) -> f64 {
        let nf = |p: DVec2| (self.temperature_map_gen.get([p.x, p.y]) + 1.0) / 2.0;
        let scale_factor = GLOBAL_BIOME_SCALE * GLOBAL_SCALE_MOD;
        nf(pos.as_dvec2() / scale_factor)
    }

    /// 0..=1 height noise
    fn h_n(&self, i: usize, p: DVec2) -> f64 {
        let point: [f64; 2] = [p.x as f64, p.y as f64];
        (self.height_map_gen[i].get(point) + 1.0) / 2.0
    }

    /// 0..=1 ridge noise
    fn h_rn(&self, i: usize, p: DVec2) -> f64 {
        2.0 * (0.5 - ((0.5 - self.h_n(i, p)) as f64).abs())
    }

    fn plains_height_noise(&self, pos: IVec2) -> f64 {
        let scale_factor = GLOBAL_SCALE_MOD * 20.0;
        let p = pos.as_dvec2() / scale_factor;

        (0.75 * self.h_n(0, p) + 0.25 * self.h_n(1, p * 2.0)) * 5.0 + 10.0
    }

    fn hills_height_noise(&self, pos: IVec2) -> f64 {
        let scale_factor = GLOBAL_SCALE_MOD * 40.0;
        let p = pos.as_dvec2() / scale_factor;

        (0.60 * self.h_n(0, p) + 0.25 * self.h_n(1, p * 1.5) + 0.15 * self.h_n(2, p * 3.0)) * 30.0
            + 15.0
    }

    fn mountains_height_noise(&self, pos: IVec2) -> f64 {
        let scale_factor = GLOBAL_SCALE_MOD * 80.0;
        let p = pos.as_dvec2() / scale_factor;

        let h0 = 0.50 * self.h_rn(0, p);
        let h01 = 0.25 * self.h_rn(1, p * 2.0) + h0;

        (h01 + (h01 / 0.75) * 0.15 * self.h_n(2, p * 5.0)
            + (h01 / 0.75) * 0.05 * self.h_rn(3, p * 9.0))
            * 150.0
            + 40.0
    }

    fn calc_voxel_params(&mut self, biome: &BiomeDefinition, pos: IVec2) -> VoxelParams {
        self.find_nearest_cell_points(pos, 1);

        let height = self.elevation_noise(pos, biome);
        let elevation = biome.elevation;
        let moisture = biome.moisture;
        let temperature = biome.temperature;

        VoxelParams {
            height: height.round() as i32,
            elevation,
            moisture,
            temperature,
        }
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
    biome_map: RefCell<BiomeMap>,
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
            biome_map: RefCell::new(biome_map),
            cell_gen: ThreadLocal::default(),
        }
    }

    pub fn generate_area_biome_map(&mut self, area: AbsChunkRange, biome_registry: &Registry<BiomeDefinition>) {
        let mut biomemap = self
            .biome_map
            .borrow_mut();
        self.biome_gen.generate_area_biomes(area, &mut biomemap, biome_registry);
    }

    pub fn generate_chunk(&mut self, pos: AbsChunkPos, chunk: &mut PaletteStorage<BlockEntry>, block_registry: &Registry<BlockDefinition>, biome_registry: &Registry<BiomeDefinition>) {
        let (i_air, _) = block_registry.lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref()).unwrap();
        let (i_grass, _) = block_registry.lookup_name_to_object(GRASS_BLOCK_NAME.as_ref()).unwrap();
        let (i_dirt, _) = block_registry.lookup_name_to_object(DIRT_BLOCK_NAME.as_ref()).unwrap();
        let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
        let (i_snow_grass, _) = block_registry.lookup_name_to_object(SNOWY_GRASS_BLOCK_NAME.as_ref()).unwrap();

        let mut cellgen = self
            .cell_gen
            .get_or(|| RefCell::new(CellGen::new(self.seed)))
            .borrow_mut();
        
        let biomemap = self
            .biome_map
            .borrow_mut();

        let current_biome = biomemap.get_biomes_near(pos)[1 + 1 * 3 + 0 * 3 * 3].unwrap().lookup(biome_registry).unwrap();

        let vparams: [VoxelParams; CHUNK_DIMZ * CHUNK_DIMZ] = {
            let mut vparams: [MaybeUninit<VoxelParams>; CHUNK_DIMZ * CHUNK_DIMZ] =
                unsafe { std::mem::MaybeUninit::uninit().assume_init() };
            for (i, v) in vparams[..].iter_mut().enumerate() {
                let x = (i % CHUNK_DIMZ) as i32 + (pos.x * CHUNK_DIM);
                let z = ((i / CHUNK_DIMZ) % CHUNK_DIMZ) as i32 + (pos.z * CHUNK_DIM);
                let p = cellgen.calc_voxel_params(current_biome, IVec2::new(x, z));
                unsafe {
                    std::ptr::write(v.as_mut_ptr(), p);
                }
            }
            unsafe { std::mem::transmute(vparams) }
        };

        for (pos_x, pos_y, pos_z) in iproduct!(0..CHUNK_DIM, 0..CHUNK_DIM, 0..CHUNK_DIM) {
            let b_pos = InChunkPos::try_new(pos_x, pos_y, pos_z).unwrap();
            let g_pos = <IVec3>::from(b_pos) + (<IVec3>::from(pos) * CHUNK_DIM);
            let y = (pos.y * CHUNK_DIM) + pos_y;
            let vp = vparams[(pos_x + pos_z * (CHUNK_DIM)) as usize];

            let h = vp.height - (pos.y * CHUNK_DIM);

            if h >= 0 {
                let ctx = Context { biome_generator: &self.biome_gen, chunk: chunk, random: PositionalRandomFactory::default(), ground_y: h };
                chunk.put(b_pos, current_biome.rule_source.place(&g_pos, &ctx, block_registry).unwrap());
            }
        }
    }
}