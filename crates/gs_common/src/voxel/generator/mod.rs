//! Standard world generator.

pub mod flat;

use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::{cell::RefCell, cmp::Ordering, collections::VecDeque, mem::MaybeUninit, ops::Deref, rc::Rc};

use bevy::utils::hashbrown::HashMap;
use bevy_math::{DVec2, IVec2, IVec3};
use gs_schemas::{
    coordinates::{AbsChunkPos, InChunkPos, CHUNK_DIM, CHUNK_DIM2Z, CHUNK_DIMZ},
    dependencies::{
        itertools::{iproduct, Itertools},
        smallvec::SmallVec,
    },
    registry::RegistryId,
    voxel::{
        biome::{
            biome_map::{BiomeMap, EXPECTED_BIOME_COUNT, GLOBAL_BIOME_SCALE, GLOBAL_SCALE_MOD},
            BiomeDefinition, BiomeEntry, BiomeRegistry, Noises, VOID_BIOME_NAME,
        },
        chunk::Chunk,
        chunk_storage::{ChunkStorage, PaletteStorage},
        generation::{fbm_noise::Fbm, positional_random::PositionalRandomFactory, Context, Noise4DTo2D},
        voxeltypes::{BlockEntry, BlockRegistry},
    },
    GsExtraData,
};
use noise::OpenSimplex;
use rand::{distributions::Uniform, Rng, SeedableRng};
use rand_xoshiro::Xoshiro128StarStar;
use serde::{Deserialize, Serialize};
use spade::handles::FixedVertexHandle;
use spade::{DelaunayTriangulation, FloatTriangulation, HasPosition, Point2, Triangulation};
use tracing::warn;

use crate::voxel::biomes::{BEACH_BIOME_NAME, OCEAN_BIOME_NAME, RIVER_BIOME_NAME};

/// World size of the +X & +Z axis, in chunks.
pub const WORLD_SIZE_XZ: i32 = 16;
/// World size of the +Y axis, in chunks.
pub const WORLD_SIZE_Y: i32 = 1;
/// Biome size in blocks.
pub const BIOME_SIZE: i32 = 64;
/// Biome size in blocks, as a float.
pub const BIOME_SIZEF: f64 = BIOME_SIZE as f64;
/// Square biome size as a float.
pub const BIOME_SIZEF2: f64 = BIOME_SIZEF * BIOME_SIZEF;

const POINT_OFFSET: f64 = 64.0;
const POINT_OFFSET_VEC: DVec2Wrapper = DVec2Wrapper(DVec2::splat(POINT_OFFSET));

const LAKE_TRESHOLD: f64 = 0.3;

const BIOME_BLEND_RADIUS: f64 = 16.0;

/// A chunk generator
pub trait VoxelGenerator<ExtraData: GsExtraData>: Send + Sync {
    /// Generates a single chunk at the given coordinates, with the given pre-filled extra data.
    fn generate_chunk(&self, position: AbsChunkPos, extra_data: ExtraData::ChunkData) -> Chunk<ExtraData>;
}

// TODO: move to a separate module
/// Standard world generator implementation.
pub struct StdGenerator<'a> {
    biome_registry: &'a BiomeRegistry,
    block_registry: &'a BlockRegistry,

    seed: u64,
    size_chunks_xz: i32,

    random: Xoshiro128StarStar,
    biome_map: BiomeMap,
    noises: Noises,

    delaunay: DelaunayTriangulation<DVec2Wrapper>,
    centers: Vec<Rc<RefCell<Center>>>,
    corners: Vec<Rc<RefCell<Corner>>>,
    edges: Vec<Rc<RefCell<Edge>>>,

    center_lookup: HashMap<[i32; 2], Rc<RefCell<Center>>>,
    corner_map: Vec<Vec<Rc<RefCell<Corner>>>>,
}

impl<'a> StdGenerator<'a> {
    /// create a new StdGenerator.
    pub fn new(
        seed: u64,
        size_chunks_xz: i32,
        biome_registry: &'a BiomeRegistry,
        block_registry: &'a BlockRegistry,
    ) -> Self {
        let seed_int = seed as u32;
        let mut value = Self {
            biome_registry,
            block_registry,

            seed,
            size_chunks_xz,

            random: Xoshiro128StarStar::seed_from_u64(seed),
            biome_map: BiomeMap::default(),
            noises: Noises {
                base_terrain_noise: Box::new(Fbm::<OpenSimplex>::new(seed_int).set_octaves(vec![-4.0, 1.0, 1.0, 0.0])),
                elevation_noise: Box::new(
                    Fbm::<OpenSimplex>::new(seed_int.wrapping_pow(1347)).set_octaves(vec![1.0, 2.0, 2.0, 1.0]),
                ),
                temperature_noise: Box::new(
                    Fbm::<OpenSimplex>::new(seed_int.wrapping_pow(2349)).set_octaves(vec![1.0, 2.0, 2.0, 1.0]),
                ),
                moisture_noise: Box::new(
                    Fbm::<OpenSimplex>::new(seed_int.wrapping_pow(3243)).set_octaves(vec![1.0, 2.0, 2.0, 1.0]),
                ),
            },

            delaunay: DelaunayTriangulation::new(),
            centers: Vec::new(),
            corners: Vec::new(),
            edges: Vec::new(),

            center_lookup: HashMap::new(),
            corner_map: Vec::new(),
        };

        // initialize generatable biomes
        let mut biomes: Vec<(RegistryId, BiomeDefinition)> = Vec::new();
        for (id, _name, def) in biome_registry.iter() {
            if def.can_generate {
                biomes.push((id, def.to_owned()));
            }
        }
        value.biome_map.generatable_biomes = biomes;
        value
    }

    /// Generate the biome map for the world.
    pub fn generate_world_biomes(&mut self) {
        /*
        let total = Instant::now();
        info!("starting biome generation");

        let start = Instant::now();
        // TODO adapt into new system
        self.calculate_downslopes();
        self.calculate_watersheds(biome_registry);
        self.create_rivers(biome_registry); // stack overflow???
        let duration = start.elapsed();
        info!("moisture calculations took {:?}", duration);

        let start = Instant::now();
        let duration = start.elapsed();
        info!("biome map lookup took {:?}", duration);

        let duration = total.elapsed();
        info!("biome generation took {:?} total", duration);
        */
    }

    /// Generate a single chunk's blocks for the world.
    pub fn generate_chunk(&mut self, c_pos: AbsChunkPos, chunk: &mut PaletteStorage<BlockEntry>) {
        let range = Uniform::new_inclusive(-BIOME_SIZEF, BIOME_SIZEF);
        let void_id = self
            .biome_registry
            .lookup_name_to_object(VOID_BIOME_NAME.as_ref())
            .unwrap()
            .0;

        let point: IVec3 = <IVec3>::from(c_pos) * IVec3::splat(CHUNK_DIM);
        let point = DVec2Wrapper::new((point.x + CHUNK_DIM / 2) as f64, (point.z + CHUNK_DIM / 2) as f64);

        let seed_bytes_be = self.seed.to_be_bytes();
        let seed_bytes_le = self.seed.to_le_bytes();
        let x = point.x.to_le_bytes();
        let y = point.y.to_be_bytes();
        let mut seed = [0_u8; 16];
        for i in 0..8 {
            seed[i] = x[i].wrapping_mul(seed_bytes_be[i]);
            seed[i + 8] = y[i].wrapping_mul(seed_bytes_le[i]);
        }
        let mut rand = Xoshiro128StarStar::from_seed(seed);
        let offset_point = point + DVec2Wrapper::new(rand.sample(range), rand.sample(range)) + POINT_OFFSET_VEC;

        let mut nearby = self
            .delaunay
            .get_vertices_in_circle(offset_point.into(), BIOME_SIZEF2)
            .sorted_by(|a, b| {
                let dist_a = a.position().distance_2(offset_point.into()).sqrt();
                let dist_b = b.position().distance_2(offset_point.into()).sqrt();
                if dist_a < dist_b {
                    Ordering::Less
                } else if dist_a == dist_b {
                    Ordering::Equal
                } else {
                    Ordering::Greater
                }
            });
        let vertex_point = if let Some(closest_vertex) = nearby.next() {
            closest_vertex.fix()
        } else {
            self
                .delaunay
                .insert(offset_point)
                .expect(format!("failed to insert point {:?} into delaunay triangulation", offset_point).as_str())
        };

        let center = self.make_edge_center_corner(vertex_point);
        self.assign_biome(center, self.biome_registry);

        let mut blended = vec![SmallVec::new(); CHUNK_DIM2Z];

        let vparams: [i32; CHUNK_DIM2Z] = {
            let mut vparams: [MaybeUninit<i32>; CHUNK_DIM2Z] = unsafe { MaybeUninit::uninit().assume_init() };
            for (i, v) in vparams[..].iter_mut().enumerate() {
                let ix = (i % CHUNK_DIMZ) as i32;
                let iz = ((i / CHUNK_DIMZ) % CHUNK_DIMZ) as i32;
                self.find_biomes_at_point(
                    DVec2::new((ix + c_pos.x * CHUNK_DIM) as f64, (iz + c_pos.z * CHUNK_DIM) as f64),
                    void_id,
                );

                self.get_biomes_at_point(&[ix + c_pos.x * CHUNK_DIM, iz + c_pos.z * CHUNK_DIM])
                    .unwrap_or(&SmallVec::<[BiomeEntry; EXPECTED_BIOME_COUNT]>::new())
                    .clone_into(&mut blended[(ix + iz * CHUNK_DIM) as usize]);
                let p = Self::elevation_noise(
                    IVec2::new(ix, iz),
                    IVec2::new(c_pos.x, c_pos.z),
                    self.biome_registry,
                    &blended,
                    &mut self.noises,
                )
                .round() as i32;
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
                let e = b.lookup(self.biome_registry).unwrap();
                let w = b.weight * e.block_influence;
                biomes.push((e, w));
            }
            // sort by block influence, then registry id if influence is same
            biomes.sort_by(|a, b| {
                a.1.partial_cmp(&b.1).unwrap_or_else(|| {
                    self.biome_registry
                        .search_object_to_id(a.0)
                        .cmp(&self.biome_registry.search_object_to_id(b.0))
                })
            });

            for (biome, _) in biomes.iter() {
                let ctx = Context {
                    seed: self.seed,
                    chunk,
                    random: PositionalRandomFactory::default(),
                    ground_y: height,
                    sea_level: 0, /* hardcoded for now... */
                };
                let result = (biome.rule_source)(&g_pos, &ctx, self.block_registry);
                if let Some(result) = result {
                    chunk.put(b_pos, result);
                }
            }
        }
    }

    fn elevation_noise(
        in_chunk_pos: IVec2,
        chunk_pos: IVec2,
        biome_registry: &BiomeRegistry,
        blended: &[SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>],
        noises: &mut Noises,
    ) -> f64 {
        let mut nf = |p: DVec2, b: &BiomeDefinition| ((b.surface_noise)(p, &mut noises.base_terrain_noise) + 1.0) / 2.0;
        let scale_factor = GLOBAL_BIOME_SCALE * GLOBAL_SCALE_MOD;
        let blend = &blended[(in_chunk_pos.x + in_chunk_pos.y * CHUNK_DIM) as usize];
        let global_pos = DVec2::new(
            (in_chunk_pos.x + (chunk_pos.x * CHUNK_DIM)) as f64,
            (in_chunk_pos.y + (chunk_pos.y * CHUNK_DIM)) as f64,
        );

        let mut heights = 0.0;
        let mut weights = 0.0;
        for entry in blend {
            let biome = entry.lookup(biome_registry).unwrap();
            let noise = nf(global_pos / scale_factor, biome);
            let strength = entry.weight * biome.blend_influence;
            heights += noise * strength;
            weights += strength;
        }
        heights / weights
    }

    fn add_to_corner_list(v: &mut Vec<Rc<RefCell<Corner>>>, x: &Option<Rc<RefCell<Corner>>>) {
        if x.is_some() && !v.iter().any(|y| Rc::ptr_eq(y, x.as_ref().unwrap())) {
            v.push(x.clone().unwrap());
        }
    }
    fn add_to_center_list(v: &mut Vec<Rc<RefCell<Center>>>, x: &Option<Rc<RefCell<Center>>>) {
        if x.is_some() && !v.iter().any(|y| Rc::ptr_eq(y, x.as_ref().unwrap())) {
            v.push(x.clone().unwrap());
        }
    }
    fn make_corner(&mut self, point: DVec2) -> Rc<RefCell<Corner>> {
        let mut bucket = point.x.abs() as usize;
        while bucket <= point.x.abs() as usize + 2 {
            if self.corner_map.get(bucket).is_none() {
                break;
            }
            for q in &self.corner_map[bucket] {
                if point.distance(q.borrow().point) < 1e-6 {
                    return q.clone();
                }
            }
            bucket += 1;
        }

        let bucket = point.x.abs() as usize + 1;
        while self.corner_map.get(bucket).is_none() {
            self.corner_map.push(Vec::new());
        }
        let q = Corner::new(point);
        //q.border = q.point.x == -x_size/2.0 || q.point.x == x_size/2.0
        //            || q.point.y == -y_size/2.0 || q.point.y == y_size/2.0;
        let q = Rc::new(RefCell::new(q));
        self.corners.push(q.clone());
        self.corner_map[bucket].push(q.clone());
        q
    }
    fn make_centers_corners_for_edge(edge: Rc<RefCell<Edge>>) {
        // Centers point to edges. Corners point to edges.
        if let Some(d0) = &edge.borrow().d0 {
            d0.borrow_mut().borders.push(edge.clone());
        }
        if let Some(d1) = &edge.borrow().d1 {
            d1.borrow_mut().borders.push(edge.clone());
        }
        if let Some(v0) = &edge.borrow().v0 {
            v0.borrow_mut().protrudes.push(edge.clone());
        }
        if let Some(v1) = &edge.borrow().v1 {
            v1.borrow_mut().protrudes.push(edge.clone());
        }

        // Centers point to centers.
        if let (Some(d0), Some(d1)) = (&edge.borrow().d0, &edge.borrow().d1) {
            Self::add_to_center_list(&mut d0.borrow_mut().neighbors, &Some(d1.clone()));
            Self::add_to_center_list(&mut d1.borrow_mut().neighbors, &Some(d0.clone()));
        }

        // Corners point to corners
        if let (Some(v0), Some(v1)) = (&edge.borrow().v0, &edge.borrow().v1) {
            Self::add_to_corner_list(&mut v0.borrow_mut().adjacent, &Some(v1.clone()));
            Self::add_to_corner_list(&mut v1.borrow_mut().adjacent, &Some(v0.clone()));
        }

        // Centers point to corners
        if let Some(d0) = &edge.borrow().d0 {
            Self::add_to_corner_list(&mut d0.borrow_mut().corners, &edge.borrow().v0);
            Self::add_to_corner_list(&mut d0.borrow_mut().corners, &edge.borrow().v1);
        }

        // Centers point to corners
        if let Some(d1) = &edge.borrow().d1 {
            Self::add_to_corner_list(&mut d1.borrow_mut().corners, &edge.borrow().v0);
            Self::add_to_corner_list(&mut d1.borrow_mut().corners, &edge.borrow().v1);
        }

        // Corners point to centers
        if let Some(v0) = &edge.borrow().v0 {
            Self::add_to_center_list(&mut v0.borrow_mut().touches, &edge.borrow().d0);
            Self::add_to_center_list(&mut v0.borrow_mut().touches, &edge.borrow().d1);
        }
        if let Some(v1) = &edge.borrow().v1 {
            Self::add_to_center_list(&mut v1.borrow_mut().touches, &edge.borrow().d0);
            Self::add_to_center_list(&mut v1.borrow_mut().touches, &edge.borrow().d1);
        }
    }

    fn make_edge_center_corner(&mut self, handle: FixedVertexHandle) -> Rc<RefCell<Center>> {
        let point = self.delaunay.vertex(handle);
        let point: DVec2 = *<DVec2Wrapper>::from(point.position());
        let center_lookup_pos = [point.x.round() as i32, point.y.round() as i32];
        let mut center = if self.center_lookup.contains_key(&center_lookup_pos) {
            return self.center_lookup.get(&center_lookup_pos).unwrap().clone();
        } else {
            let center = Rc::new(RefCell::new(Center::new(point)));
            self.centers.push(center.clone());
            self.center_lookup.insert(center_lookup_pos, center.clone());
            center
        };

        let edges = Self::make_edges(&self.delaunay, handle);
        for (delaunay_edge, voronoi_edge) in edges {
            let midpoint = voronoi_edge.0.lerp(voronoi_edge.1, 0.5);
            for edge in &self.edges {
                if (midpoint - edge.borrow().midpoint).length() < 1e-3 {
                    continue;
                }
            }

            let mut edge = Edge::new();
            edge.midpoint = midpoint;

            // Edges point to corners. Edges point to centers.
            edge.v0 = Some(self.make_corner(voronoi_edge.0));
            edge.v1 = Some(self.make_corner(voronoi_edge.1));
            let d0_pos = [delaunay_edge.0.x.round() as i32, delaunay_edge.0.y.round() as i32];
            edge.d0 = self.center_lookup.get(&d0_pos).cloned().or_else(|| {
                let center = Rc::new(RefCell::new(Center::new(delaunay_edge.0)));
                self.centers.push(center.clone());
                self.center_lookup.insert(d0_pos, center.clone());
                Some(center)
            });
            let d1_pos = [delaunay_edge.1.x.round() as i32, delaunay_edge.1.y.round() as i32];
            edge.d1 = self.center_lookup.get(&d1_pos).cloned().or_else(|| {
                let center = Rc::new(RefCell::new(Center::new(delaunay_edge.1)));
                self.centers.push(center.clone());
                self.center_lookup.insert(d1_pos, center.clone());
                Some(center)
            });

            let rc = Rc::new(RefCell::new(edge));
            Self::make_centers_corners_for_edge(rc.clone());
            self.edges.push(rc);
        }
        self.assign_noise_and_ocean(&mut center);

        return center;
    }

    /// returns: \[(delaunay edges, voronoi edges)\]
    fn make_edges(
        delaunay_triangulation: &DelaunayTriangulation<DVec2Wrapper>,
        handle: FixedVertexHandle,
    ) -> Vec<(PointEdge, PointEdge)> {
        let mut list_of_delaunay_edges = Vec::new();
        let vertex = delaunay_triangulation.vertex(handle);
        let edges = vertex.out_edges().collect_vec();
        for edge in edges.iter() {
            let vertex_1 = *edge.from().data();
            let vertex_2 = *edge.to().data();
            list_of_delaunay_edges.push(PointEdge(*(vertex_1), *(vertex_2)));
        }

        let mut list_of_voronoi_edges = Vec::new();
        for edge in vertex.as_voronoi_face().adjacent_edges() {
            if let (Some(from), Some(to)) = (edge.from().position(), edge.to().position()) {
                list_of_voronoi_edges.push(PointEdge(*(<DVec2Wrapper>::from(from)), *(<DVec2Wrapper>::from(to))));
            }
        }

        list_of_delaunay_edges
            .into_iter()
            .zip(list_of_voronoi_edges)
            .collect_vec()
    }

    fn make_noise(noises: &Noises, point: DVec2) -> NoiseValues {
        let scale_factor = GLOBAL_BIOME_SCALE * GLOBAL_SCALE_MOD;
        let point = [point.x / scale_factor, point.y / scale_factor];
        let elevation = Self::map_range((-1.5, 1.5), (0.0, 5.0), noises.elevation_noise.get_2d(point));
        let temperature = Self::map_range((-1.5, 1.5), (0.0, 5.0), noises.temperature_noise.get_2d(point));
        let moisture: f64 = Self::map_range((-1.5, 1.5), (0.0, 5.0), noises.moisture_noise.get_2d(point));

        NoiseValues {
            elevation,
            temperature,
            moisture,
        }
    }

    /// Compute polygon attributes 'ocean' and 'water' based on the
    /// corner attributes. Count the water corners per
    /// polygon. Oceans are all polygons connected to the edge of the
    /// map. In the first pass, mark the edges of the map as ocean;
    /// in the second pass, mark any water-containing polygon
    /// connected to an ocean as ocean.
    fn assign_noise_and_ocean(&mut self, center: &mut Rc<RefCell<Center>>) {
        let mut queue = VecDeque::new();

        {
            let mut p_b = center.borrow_mut();
            // assign noise parameters based on node position
            p_b.noise = Self::make_noise(&self.noises, p_b.point);
            let mut num_water = 0;

            for q in p_b.corners.clone() {
                let q = q.borrow();
                if q.border {
                    p_b.ocean = true;
                    p_b.water = true;
                    queue.push_back(center.clone());
                }
                if q.water {
                    num_water += 1;
                }
            }
            p_b.water = p_b.ocean || num_water as f64 >= p_b.corners.len() as f64 * LAKE_TRESHOLD;
        }
        while !queue.is_empty() {
            let p = queue.pop_back();
            if p.is_none() {
                break;
            }
            for r in &p.unwrap().borrow().neighbors {
                let mut r_b = r.borrow_mut();
                if r_b.water && !r_b.ocean {
                    r_b.ocean = true;
                    queue.push_back(r.clone());
                }
            }
        }

        // Set the polygon attribute 'coast' based on its neighbors. If
        // it has at least one ocean and at least one land neighbor,
        // then this is a coastal polygon.
        {
            let mut num_ocean = 0;
            let mut num_land = 0;
            for r in &center.borrow().neighbors {
                if r.borrow().ocean {
                    num_ocean += 1;
                }
                if !r.borrow().water {
                    num_land += 1;
                }
            }
            center.borrow_mut().coast = num_land > 0 && num_ocean > 0;
        }

        // Set the corner attributes based on the computed polygon
        // attributes. If all polygons connected to this corner are
        // ocean, then it's ocean; if all are land, then it's land;
        // otherwise it's coast.
        for q in &center.borrow().corners {
            let mut q_b = q.borrow_mut();
            q_b.noise = Self::make_noise(&self.noises, q_b.point);
            let mut num_ocean = 0;
            let mut num_land = 0;
            for p in &q_b.touches {
                if p.borrow().ocean {
                    num_ocean += 1;
                }
                if !p.borrow().water {
                    num_land += 1;
                }
            }
            q_b.ocean = num_ocean == q_b.touches.len();
            q_b.coast = num_land > 0 && num_ocean > 0;
            q_b.water = q_b.border || (num_land != q_b.touches.len() && !q_b.coast);
        }
    }

    fn calculate_downslopes(&mut self) {
        let mut r;
        for q in &self.corners {
            r = q.clone();
            for s in &q.borrow().adjacent {
                if s.borrow().noise.elevation <= r.borrow().noise.elevation {
                    r = s.clone();
                }
            }
            q.borrow_mut().downslope = Some(r);
        }
    }

    /// Calculate the watershed of every land point. The watershed is
    /// the last downstream land point in the downslope graph. TODO:
    /// watersheds are currently calculated on corners, but it'd be
    /// more useful to compute them on polygon centers so that every
    /// polygon can be marked as being in one watershed.
    #[allow(clippy::assigning_clones)] // false positive, "fixing" this causes a borrow checker error
    fn calculate_watersheds(&mut self, biome_registry: &BiomeRegistry) {
        let (ocean_id, _) = biome_registry.lookup_name_to_object(OCEAN_BIOME_NAME.as_ref()).unwrap();
        let (beach_id, _) = biome_registry.lookup_name_to_object(BEACH_BIOME_NAME.as_ref()).unwrap();

        // Initially the watershed pointer points downslope one step.
        for q in &self.corners {
            let mut q_b = q.borrow_mut();
            q_b.watershed = Some(q.clone());
            if q_b.biome != Some(ocean_id) && q_b.biome != Some(beach_id) {
                q_b.watershed = q_b.downslope.clone();
            }
        }

        // Follow the downslope pointers to the coast. Limit to 100
        // iterations although most of the time with numPoints==2000 it
        // only takes 20 iterations because most points are not far from
        // a coast.  TODO: can run faster by looking at
        // p.watershed.watershed instead of p.downslope.watershed.
        for _ in 0..100 {
            let mut changed = false;
            for q in &self.corners {
                let mut q_b = q.borrow_mut();
                // why does this stack overflow???
                if Rc::ptr_eq(q_b.watershed.as_ref().unwrap(), q) {
                    continue;
                }
                if !q_b.ocean && !q_b.coast && !q_b.watershed.as_ref().unwrap().borrow().coast && {
                    let downslope = q_b.downslope.as_ref().unwrap().borrow();
                    let r = downslope.watershed.as_ref().unwrap().borrow();
                    !r.ocean
                } {
                    let downslope_watershed = q_b.downslope.as_ref().unwrap().borrow().watershed.clone().unwrap();
                    q_b.watershed = Some(downslope_watershed);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        // How big is each watershed?
        for q in &self.corners {
            let mut q_b = q.borrow_mut();
            let r = q_b.watershed.as_ref().unwrap();
            if Rc::ptr_eq(q_b.watershed.as_ref().unwrap(), r) {
                q_b.watershed_size += 1;
            } else {
                r.borrow_mut().watershed_size += 1;
            }
        }
    }

    fn create_rivers(&mut self, biome_registry: &BiomeRegistry) {
        let (river_id, _) = biome_registry.lookup_name_to_object(RIVER_BIOME_NAME.as_ref()).unwrap();

        for _ in 0..(self.size_chunks_xz / 2) {
            let mut q = self.corners[self.random.gen_range(0..self.corners.len())].clone();
            if q.borrow_mut().ocean || q.borrow_mut().noise.elevation < 1.0 || q.borrow_mut().noise.elevation > 3.5 {
                continue;
            }
            while !q.borrow().coast {
                if Rc::ptr_eq(&q, q.borrow().downslope.as_ref().unwrap()) {
                    break;
                }
                let edge = Self::lookup_edge_from_corner(&q, q.borrow().downslope.as_ref().unwrap()).unwrap();
                let mut edge = edge.borrow_mut();
                edge.river += 1;
                q.borrow_mut().river += 1;
                q.borrow_mut().downslope.as_mut().unwrap().borrow_mut().river += 1;
                edge.biome = Some(river_id);

                q = q.clone().borrow_mut().downslope.as_ref().unwrap().clone();
            }
        }
    }

    fn assign_biome(&mut self, center: Rc<RefCell<Center>>, biome_registry: &BiomeRegistry) {
        // go over all centers and assign biomes to them based on noise & other parameters.
        let mut center = center.borrow_mut();

        // first assign the corners' biomes
        for corner in &center.corners {
            let mut corner = corner.borrow_mut();
            if corner.biome.is_some() {
                continue;
            }
            if corner.ocean {
                corner.biome = Some(
                    biome_registry
                        .lookup_name_to_object(OCEAN_BIOME_NAME.as_ref())
                        .unwrap()
                        .0,
                );
                continue;
            } else if corner.water {
                // TODO make lake biome(s)
                corner.biome = Some(
                    biome_registry
                        .lookup_name_to_object(OCEAN_BIOME_NAME.as_ref())
                        .unwrap()
                        .0,
                );
                continue;
            } else if corner.coast {
                corner.biome = Some(
                    biome_registry
                        .lookup_name_to_object(BEACH_BIOME_NAME.as_ref())
                        .unwrap()
                        .0,
                );
                continue;
            }
            for (id, biome) in &self.biome_map.generatable_biomes {
                if biome.elevation.contains(corner.noise.elevation)
                    && biome.temperature.contains(corner.noise.temperature)
                    && biome.moisture.contains(corner.noise.moisture)
                {
                    corner.biome = Some(*id);
                    break;
                }
            }
        }
        // then the edges' biomes
        for edge in &center.borders {
            let mut edge = edge.borrow_mut();
            if edge.biome.is_some() {
                continue;
            }
            for (id, biome) in &self.biome_map.generatable_biomes {
                if biome.elevation.contains(edge.noise.elevation)
                    && biome.temperature.contains(edge.noise.temperature)
                    && biome.moisture.contains(edge.noise.moisture)
                {
                    edge.biome = Some(*id);
                    break;
                }
            }
        }

        if center.biome.is_some() {
            return;
        }
        if center.ocean {
            center.biome = Some(
                biome_registry
                    .lookup_name_to_object(OCEAN_BIOME_NAME.as_ref())
                    .unwrap()
                    .0,
            );
            return;
        } else if center.water {
            // TODO make lake biome(s)
            center.biome = Some(
                biome_registry
                    .lookup_name_to_object(OCEAN_BIOME_NAME.as_ref())
                    .unwrap()
                    .0,
            );
            return;
        } else if center.coast {
            center.biome = Some(
                biome_registry
                    .lookup_name_to_object(BEACH_BIOME_NAME.as_ref())
                    .unwrap()
                    .0,
            );
            return;
        }
        let mut found = false;
        for (id, biome) in &self.biome_map.generatable_biomes {
            if biome.elevation.contains(center.noise.elevation)
                && biome.temperature.contains(center.noise.temperature)
                && biome.moisture.contains(center.noise.moisture)
            {
                center.biome = Some(*id);
                found = true;
                break;
            }
        }
        if !found {
            warn!(
                "found no biome for point {:?}, noise values: {:?}. Picking randomly.",
                center.point, center.noise
            );
            let index = self.random.gen_range(0..self.biome_map.generatable_biomes.len());
            center.biome = Some(self.biome_map.generatable_biomes[index].0);
            warn!(
                "picked {}",
                biome_registry.lookup_id_to_object(center.biome.unwrap()).unwrap()
            );
        }
    }

    fn lookup_edge_from_corner(q: &Rc<RefCell<Corner>>, s: &Rc<RefCell<Corner>>) -> Option<Rc<RefCell<Edge>>> {
        for edge in &q.borrow().protrudes {
            if edge.borrow().v0.is_some() && Rc::ptr_eq(edge.borrow().v0.as_ref().unwrap(), s) {
                return Some(edge.clone());
            }
            if edge.borrow().v1.is_some() && Rc::ptr_eq(edge.borrow().v1.as_ref().unwrap(), s) {
                return Some(edge.clone());
            }
        }
        None
    }

    fn find_biomes_at_point(&mut self, point: DVec2, default: RegistryId) {
        let p = [point.x.round() as i32, point.y.round() as i32];
        if self.biome_map.biome_map.contains_key(&p) {
            return;
        }

        let distance_ordering = |a: &Rc<RefCell<Center>>, b: &Rc<RefCell<Center>>| -> Ordering {
            let dist_a = point.distance(a.borrow().point);
            let dist_b = point.distance(b.borrow().point);
            if dist_a < dist_b {
                Ordering::Less
            } else if dist_a > dist_b {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        };
        let fade = |t: f64| -> f64 { t * t * (3.0 - 2.0 * t) };

        let mut sorted = self.centers.clone();
        sorted.sort_by(distance_ordering);

        let closest = sorted[0].borrow();
        let closest_distance = closest.point.distance(point);

        let mut nearby = Vec::new();
        for center in sorted.iter() {
            let c_b = center.borrow();
            if c_b.point.distance(point) <= 4.0 * BIOME_BLEND_RADIUS + closest_distance {
                nearby.push(Rc::new(RefCell::new((center.clone(), 1.0))));
            }
        }

        for (first_node, second_node) in nearby.clone().into_iter().tuple_combinations() {
            let mut first_node = first_node.borrow_mut();
            let mut second_node = second_node.borrow_mut();
            let first = first_node.0.borrow().point;
            let second = second_node.0.borrow().point;

            let distance_from_midpoint =
                (point - (first + second) / 2.0).dot(second - first) / (second - first).length();
            let weight = fade((distance_from_midpoint / BIOME_BLEND_RADIUS).clamp(-1.0, 1.0) * 0.5 + 0.5);

            first_node.1 *= 1.0 - weight;
            second_node.1 *= weight;
        }

        let mut to_blend = SmallVec::<[BiomeEntry; EXPECTED_BIOME_COUNT]>::new();
        let (mut point_elevation, mut point_temperature, mut point_moisture) = (0.0, 0.0, 0.0);

        for node in nearby {
            let node = node.borrow();
            let (center, weight) = node.deref();
            let center = center.borrow();
            let weight = *weight;

            point_elevation += center.noise.elevation * weight;
            point_temperature += center.noise.temperature * weight;
            point_moisture += center.noise.moisture * weight;

            let blend = to_blend.iter_mut().find(|e| e.id == center.biome.unwrap_or(default));
            if let Some(blend) = blend {
                blend.weight += weight;
            } else {
                to_blend.push(BiomeEntry {
                    id: center.biome.unwrap_or(default),
                    weight,
                });
            }
        }

        self.biome_map.biome_map.insert(p, to_blend);
        self.biome_map
            .noise_map
            .insert(p, (point_elevation, point_temperature, point_moisture));
    }

    /// Get the biomes at the given point from the biome map.
    pub fn get_biomes_at_point(&self, point: &[i32; 2]) -> Option<&SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>> {
        self.biome_map.biome_map.get(point)
    }

    /// Get the noise values at the given point from the biome map.
    pub fn get_noises_at_point(&self, point: &[i32; 2]) -> Option<&(f64, f64, f64)> {
        self.biome_map.noise_map.get(point)
    }

    fn map_range(from_range: (f64, f64), to_range: (f64, f64), s: f64) -> f64 {
        to_range.0 + (s - from_range.0) * (to_range.1 - to_range.0) / (from_range.1 - from_range.0)
    }

    /// Get all voronoi edges this map contains.
    pub fn edges(&self) -> &Vec<Rc<RefCell<Edge>>> {
        &self.edges
    }

    /// Get the +XZ size of the world, in blocks.
    pub fn size_blocks_xz(&self) -> i32 {
        self.size_chunks_xz * CHUNK_DIM
    }

    /// Get the biome map of this generator.
    pub fn biome_map(&self) -> &BiomeMap {
        &self.biome_map
    }
}

#[allow(dead_code)]
fn is_inside(point: DVec2, polygon: &[DVec2]) -> bool {
    let len = polygon.len();
    for i in 0..len {
        let v1 = polygon[i] - point;
        let v2 = polygon[(i + 1) % len] - point;
        let edge = v1 - v2;

        let x = edge.perp_dot(v1);
        if x > 0.0 {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Debug)]
struct NoiseValues {
    elevation: f64,
    temperature: f64,
    moisture: f64,
}

/// Center of a voronoi cell, corner of a delaunay triangle
#[derive(Clone, PartialEq, Debug)]
pub struct Center {
    /// Center of the cell
    pub point: DVec2,
    noise: NoiseValues,
    biome: Option<RegistryId>,

    water: bool,
    ocean: bool,
    coast: bool,

    neighbors: Vec<Rc<RefCell<Center>>>,
    borders: Vec<Rc<RefCell<Edge>>>,
    corners: Vec<Rc<RefCell<Corner>>>,
}

impl Center {
    fn new(point: DVec2) -> Center {
        Self {
            point,
            noise: NoiseValues::default(),
            biome: None,

            water: false,
            ocean: false,
            coast: false,

            neighbors: Vec::new(),
            borders: Vec::new(),
            corners: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct PointEdge(DVec2, DVec2);

/// Edge of a voronoi cell & delaunay triangle
#[derive(Clone, PartialEq, Debug)]
pub struct Edge {
    /// Delaunay edge start
    pub d0: Option<Rc<RefCell<Center>>>,
    /// Delaunay edge end
    pub d1: Option<Rc<RefCell<Center>>>,
    /// Voronoi edge start
    pub v0: Option<Rc<RefCell<Corner>>>,
    /// Voronoi edge end
    pub v1: Option<Rc<RefCell<Corner>>>,
    /// halfway between v0,v1
    pub midpoint: DVec2,

    noise: NoiseValues,        // noise value at midpoint
    biome: Option<RegistryId>, // biome at midpoint

    river: i32, // 0 if no river, or volume of water in river
}

impl Edge {
    fn new() -> Edge {
        Self {
            d0: None,
            d1: None,
            v0: None,
            v1: None,
            midpoint: DVec2::default(),

            noise: NoiseValues::default(),
            biome: None,

            river: 0,
        }
    }
}

/// Corner of a voronoi cell, center of a delaunay triangle
#[derive(Clone, PartialEq, Debug)]
pub struct Corner {
    /// Location of the corner
    pub point: DVec2,
    noise: NoiseValues,
    border: bool,
    biome: Option<RegistryId>,

    downslope: Option<Rc<RefCell<Corner>>>,
    watershed: Option<Rc<RefCell<Corner>>>,
    watershed_size: i32,

    water: bool,
    ocean: bool,
    coast: bool,
    river: i32,

    touches: Vec<Rc<RefCell<Center>>>,
    protrudes: Vec<Rc<RefCell<Edge>>>,
    adjacent: Vec<Rc<RefCell<Corner>>>,
}

impl Corner {
    fn new(position: DVec2) -> Corner {
        Self {
            noise: NoiseValues::default(),
            point: position,
            border: false,
            biome: None,

            downslope: None,
            watershed: None,
            watershed_size: 0,

            water: false,
            ocean: false,
            coast: false,
            river: 0,

            touches: Vec::new(),
            protrudes: Vec::new(),
            adjacent: Vec::new(),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
struct DVec2Wrapper(DVec2);

impl DVec2Wrapper {
    fn new(x: f64, y: f64) -> DVec2Wrapper {
        DVec2Wrapper(DVec2::new(x, y))
    }
}

impl Add<DVec2Wrapper> for DVec2Wrapper {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        DVec2Wrapper(self.0.add(rhs.0))
    }
}
impl AddAssign<DVec2Wrapper> for DVec2Wrapper {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0.add_assign(rhs.0)
    }
}
impl Sub<DVec2Wrapper> for DVec2Wrapper {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        DVec2Wrapper(self.0.sub(rhs.0))
    }
}
impl SubAssign<DVec2Wrapper> for DVec2Wrapper {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0.sub_assign(rhs.0)
    }
}
impl Deref for DVec2Wrapper {
    type Target = DVec2;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl HasPosition for DVec2Wrapper {
    type Scalar = f64;
    fn position(&self) -> Point2<Self::Scalar> {
        Point2::new(self.x, self.y)
    }
}
impl From<DVec2> for DVec2Wrapper {
    fn from(value: DVec2) -> Self {
        DVec2Wrapper(value)
    }
}
impl From<Point2<f64>> for DVec2Wrapper {
    fn from(value: Point2<f64>) -> Self {
        DVec2Wrapper::new(value.x, value.y)
    }
}
impl Into<Point2<f64>> for DVec2Wrapper {
    fn into(self) -> Point2<f64> {
        Point2::new(self.x, self.y)
    }
}
impl Into<DVec2> for DVec2Wrapper {
    fn into(self) -> DVec2 {
        *self
    }
}
