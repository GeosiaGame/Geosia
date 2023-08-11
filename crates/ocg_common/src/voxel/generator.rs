use bevy::math::{ivec2, IVec2, DVec2};
use lru::LruCache;
use noise::{NoiseFn, Seedable, SuperSimplex};
use ocg_schemas::{voxel::{chunk_storage::{PaletteStorage, ChunkStorage}, voxeltypes::{BlockEntry, BlockDefinition, EMPTY_BLOCK_NAME}, chunk::Chunk}, coordinates::{AbsChunkPos, InChunkPos, InChunkRange}, registry::Registry, dependencies::itertools::iproduct};
use rand::prelude::*;
use rand_xoshiro::Xoshiro256StarStar;
use std::cell::RefCell;
use std::mem::MaybeUninit;
use thread_local::ThreadLocal;

use ocg_schemas::coordinates::{CHUNK_DIM, CHUNK_DIMZ};

use super::blocks::*;
use super::noise::fbm_noise::Fbm;

const GLOBAL_SCALE_MOD: f64 = 4.0;
const GLOBAL_BIOME_SCALE: f64 = 256.0;
const SUPERGRID_SIZE: i32 = 4 * CHUNK_DIM as i32;
type InCellRng = Xoshiro256StarStar;
type CellPointsT = [CellPoint; 4];

fn distance2(a: IVec2, b: IVec2) -> i32 {
    (a - b).length_squared()
}

#[derive(Clone, Copy, Debug)]
struct CellPoint {
    pos: IVec2,
    /// 0..1
    elevation_class: f64,
}

impl CellPoint {
    pub fn calc(&mut self, cg: &mut CellGen) {
        self.elevation_class = cg.elevation_noise(self.pos);
    }
}

impl Default for CellPoint {
    fn default() -> Self {
        Self {
            pos: ivec2(0, 0),
            elevation_class: 0.0,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VPElevation {
    LowLand,
    Hill,
    Mountain,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum VPMoisture {
    Deadland,
    Desert,
    LowMoist,
    MedMoist,
    HiMoist,
}

impl Default for VPElevation {
    fn default() -> Self {
        VPElevation::LowLand
    }
}

impl Default for VPMoisture {
    fn default() -> Self {
        VPMoisture::MedMoist
    }
}

#[derive(Copy, Clone, Debug, Default)]
struct VoxelParams {
    height: i32,
    elevation: VPElevation,
    moisture: VPMoisture,
}

struct CellGen {
    seed: u64,
    height_map_gen: [Fbm<SuperSimplex>; 5],
    elevation_map_gen: Fbm<SuperSimplex>,
    moisture_map_gen: Fbm<SuperSimplex>,
    cell_points: LruCache<IVec2, CellPointsT>,
    nearest_buf: Vec<(i32, CellPoint)>,
}

impl CellGen {
    fn new(seed: u64) -> Self {
        let mut simplex = Fbm::<SuperSimplex>::new(0);
        let mut octaves = Vec::new();
        octaves.push(0.0);
        octaves.push(1.0);
        octaves.push(1.0);
        octaves.push(2.0);
        simplex = simplex.set_octaves(octaves);
        simplex = simplex.set_persistence(0.6);
        simplex = simplex.set_frequency(2.0);
        let mut s = Self {
            seed: 0,
            height_map_gen: [simplex.clone(), simplex.clone(), simplex.clone(), simplex.clone(), simplex.clone()], // init like this because Fbm can't implement copy
            elevation_map_gen: simplex.clone(),
            moisture_map_gen: simplex.clone(),
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
        Fbm::<SuperSimplex>::set_seed(&mut self.elevation_map_gen, sdgen.next_u32());
        Fbm::<SuperSimplex>::set_seed(&mut self.moisture_map_gen, sdgen.next_u32());
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

    fn elevation_noise(&self, pos: IVec2) -> f64 {
        let nf = |p: DVec2| (self.elevation_map_gen.get([p.x, p.y]) + 1.0) / 2.0;
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
        let scale_factor = GLOBAL_SCALE_MOD * 120.0;
        let p = pos.as_dvec2() / scale_factor;

        (0.75 * self.h_n(0, p) + 0.25 * self.h_n(1, p * 2.0)) * 5.0 + 10.0
    }

    fn hills_height_noise(&self, pos: IVec2) -> f64 {
        let scale_factor = GLOBAL_SCALE_MOD * 160.0;
        let p = pos.as_dvec2() / scale_factor;

        (0.60 * self.h_n(0, p) + 0.25 * self.h_n(1, p * 1.5) + 0.15 * self.h_n(2, p * 3.0)) * 30.0
            + 15.0
    }

    fn mountains_height_noise(&self, pos: IVec2) -> f64 {
        let scale_factor = GLOBAL_SCALE_MOD * 200.0;
        let p = pos.as_dvec2() / scale_factor;

        let h0 = 0.50 * self.h_rn(0, p);
        let h01 = 0.25 * self.h_rn(1, p * 2.0) + h0;

        (h01 + (h01 / 0.75) * 0.15 * self.h_n(2, p * 5.0)
            + (h01 / 0.75) * 0.05 * self.h_rn(3, p * 9.0))
            * 750.0
            + 40.0
    }

    fn calc_voxel_params(&mut self, pos: IVec2) -> VoxelParams {
        self.find_nearest_cell_points(pos, 1);
        let cp = self.nearest_buf[0].1;
        let cec = cp.elevation_class;
        let ec = self.elevation_noise(pos);

        let height: f64;
        if ec < 0.4 {
            let ph = self.plains_height_noise(pos);
            height = ph;
        } else if ec < 0.5 {
            let nlin = (ec - 0.4) / 0.1;
            let olin = 1.0 - nlin;
            let ph = self.plains_height_noise(pos);
            let hh = self.hills_height_noise(pos);
            height = olin * ph + nlin * hh;
        } else if ec < 0.7 {
            let hh = self.hills_height_noise(pos);
            height = hh;
        } else if ec < 0.8 {
            let nlin = (ec - 0.7) / 0.75;
            let olin = 1.0 - nlin;
            let hh = self.hills_height_noise(pos);
            let mh = self.mountains_height_noise(pos);
            height = olin * hh + nlin * mh;
        } else {
            let mh = self.mountains_height_noise(pos);
            height = mh;
        }
        let elevation: VPElevation;
        if cec < 0.4 {
            elevation = VPElevation::LowLand;
        } else if cec < 0.5 {
            let p = pos.as_dvec2();
            let bnoise = self.height_map_gen.last().unwrap().get([p.x, p.y]);
            if bnoise < 0.0 {
                elevation = VPElevation::LowLand;
            } else {
                elevation = VPElevation::Hill;
            }
        } else if cec < 0.7 {
            elevation = VPElevation::Hill;
        } else if cec < 0.8 {
            let p = pos.as_dvec2();
            let bnoise = self.height_map_gen.last().unwrap().get([p.x, p.y]);
            if bnoise < 0.0 {
                elevation = VPElevation::Hill;
            } else {
                elevation = VPElevation::Mountain;
            }
        } else {
            elevation = VPElevation::Mountain;
        }

        let moisture = VPMoisture::MedMoist;

        VoxelParams {
            height: height.round() as i32,
            elevation,
            moisture,
        }
    }
}

impl Default for CellGen {
    fn default() -> Self {
        Self::new(0)
    }
}

pub struct StdGenerator<ECD> {
    seed: u64,
    cell_gen: ThreadLocal<RefCell<CellGen>>,
    extra_data: ECD
}

impl<ECD> Default for StdGenerator<ECD> where ECD: Clone + Default {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<ECD> StdGenerator<ECD> where ECD: Clone + Default {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            cell_gen: ThreadLocal::default(),
            extra_data: ECD::default()
        }
    }

    pub fn generate_chunk(&self, pos: AbsChunkPos, chunk: &mut Chunk<ECD>, block_registry: &Registry<BlockDefinition>) {
        let chunk_blocks = &mut chunk.blocks;
        let (i_air, _) = block_registry.lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref()).unwrap();
        let (i_grass, _) = block_registry.lookup_name_to_object(GRASS_BLOCK_NAME.as_ref()).unwrap();
        let (i_dirt, _) = block_registry.lookup_name_to_object(DIRT_BLOCK_NAME.as_ref()).unwrap();
        let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
        let (i_snow_grass, _) = block_registry.lookup_name_to_object(SNOWY_GRASS_BLOCK_NAME.as_ref()).unwrap();

        let mut cellgen = self
            .cell_gen
            .get_or(|| RefCell::new(CellGen::new(self.seed)))
            .borrow_mut();

        let vparams: [VoxelParams; CHUNK_DIMZ * CHUNK_DIMZ] = {
            let mut vparams: [MaybeUninit<VoxelParams>; CHUNK_DIMZ * CHUNK_DIMZ] =
                unsafe { std::mem::MaybeUninit::uninit().assume_init() };
            for (i, v) in vparams[..].iter_mut().enumerate() {
                let x = (i % CHUNK_DIMZ) as i32 + (pos.x * CHUNK_DIM);
                let z = ((i / CHUNK_DIMZ) % CHUNK_DIMZ) as i32 + (pos.z * CHUNK_DIM);
                let p = cellgen.calc_voxel_params(IVec2::new(x, z));
                unsafe {
                    std::ptr::write(v.as_mut_ptr(), p);
                }
            }
            unsafe { std::mem::transmute(vparams) }
        };

        for (pos_x, pos_y, pos_z) in iproduct!(0..CHUNK_DIM, 0..CHUNK_DIM, 0..CHUNK_DIM) {
            let b_pos = InChunkPos::try_new(pos_x, pos_y, pos_z).unwrap();
            //let xc = (vidx % CHUNK_DIM) as i32;
            //let zc = ((vidx / CHUNK_DIM) % CHUNK_DIM) as i32;
            //let yc = ((vidx / CHUNK_DIM / CHUNK_DIM) % CHUNK_DIM) as i32;
            ////let x = (chunk.position.0.x * VCD) as i32 + xc;
            let y = (pos.y * CHUNK_DIM) + pos_y;
            ////let z = (chunk.position.0.z * VCD) as i32 + zc;
            //let vp = vparams[(xc + zc * (CHUNK_DIM as i32)) as usize];
            let vp = vparams[(pos_x + pos_z * (CHUNK_DIM as i32)) as usize];

            let h = vp.height - (pos.y * CHUNK_DIM);
                        
            //if h > 0 {
            //    chunk_blocks.put(InChunkPos::try_new(pos_x, h, pos_x).unwrap(), BlockEntry::new(i_dirt, 0));

            //    let max_x = if pos_x >= CHUNK_DIM - 1 { 0 } else { pos_x + 1 };
            //    let max_z = if pos_z >= CHUNK_DIM - 1 { 0 } else { pos_z + 1 };
            //    
            //    let bottom = InChunkPos::try_new(pos_x, 0, pos_z).unwrap();
            //    let surface_minus_5 = InChunkPos::try_new(max_x, h - 5, max_z).unwrap();
            //    let surface_minus_1 = InChunkPos::try_new(pos_x, h - 1, pos_z).unwrap();
            //    let surface = InChunkPos::try_new(max_x, h, max_z).unwrap();

            //    chunk_blocks.fill(InChunkRange::from_corners(bottom, surface_minus_5), BlockEntry::new(i_stone, 0));
            //    chunk_blocks.fill(InChunkRange::from_corners(surface_minus_5, surface_minus_1), BlockEntry::new(i_dirt, 0));
            //    chunk_blocks.fill(InChunkRange::from_corners(surface_minus_1, surface), if vp.elevation == VPElevation::Mountain { BlockEntry::new(i_snow_grass, 0) } else { BlockEntry::new(i_grass, 0) });
            //    //println!("Amount of generated blocks at chunk=[{0},{1},{2}] x={pos_x},z={pos_z}: {h}", pos.x, pos.y, pos.z)
            //}
//            chunk_blocks.put(b_pos, BlockEntry::new(i_dirt, 0));
            chunk_blocks.put(b_pos, BlockEntry::new(
            if pos_y == h {
                    if vp.elevation == VPElevation::Mountain && y > 80 {
                        i_snow_grass
                    } else {
                        i_grass
                    }
                } else if pos_y < h - 5 {
                    i_stone
                } else if pos_y < h {
                    i_dirt
                } else {
                    i_air
                },
            0));
            //if pos_y - h - 16 < 0 {
            //    chunk_blocks.put(b_pos, BlockEntry::new(i_stone, 0));
            //}
        }
    }
}