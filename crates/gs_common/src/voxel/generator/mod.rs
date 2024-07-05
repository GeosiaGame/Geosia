//! Standard world generator.

pub mod flat;

use std::{cell::RefCell, cmp::Ordering, collections::VecDeque, mem::MaybeUninit, ops::Deref, rc::Rc, time::Instant};

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
use tracing::{info, warn};
use voronoice::*;

use crate::voxel::biomes::{BEACH_BIOME_NAME, OCEAN_BIOME_NAME, RIVER_BIOME_NAME};

/// World size of the +X & +Z axis, in chunks.
pub const WORLD_SIZE_XZ: i32 = 2;
/// World size of the +Y axis, in chunks.
pub const WORLD_SIZE_Y: i32 = 1;

const TRIANGLE_VERTICES: [(usize, usize); 3] = [(0, 1), (1, 2), (2, 0)];
const LAKE_TRESHOLD: f64 = 0.3;

const BIOME_BLEND_RADIUS: f64 = 16.0;

/// A chunk generator
pub trait VoxelGenerator<ExtraData: GsExtraData>: Send + Sync {
    /// Generates a single chunk at the given coordinates, with the given pre-filled extra data.
    fn generate_chunk(&self, position: AbsChunkPos, extra_data: ExtraData::ChunkData) -> Chunk<ExtraData>;
}

// TODO: move to a separate module
/// Standard world generator implementation.
pub struct StdGenerator {
    seed: u64,
    size_chunks_xz: i32,
    biome_point_count: u32,

    random: Xoshiro128StarStar,
    biome_map: BiomeMap,
    noises: Noises,

    voronoi: Option<Voronoi>,
    points: Vec<Point>,
    centers: Vec<Rc<RefCell<Center>>>,
    corners: Vec<Rc<RefCell<Corner>>>,
    edges: Vec<Rc<RefCell<Edge>>>,

    delaunay_centers: Vec<DelaunayCenter>,
    voronoi_centers: Vec<VoronoiCenter>,
}

impl StdGenerator {
    /// create a new StdGenerator.
    pub fn new(seed: u64, size_chunks_xz: i32, biome_point_count: u32) -> Self {
        let seed_int = seed as u32;
        Self {
            seed,
            size_chunks_xz,
            biome_point_count,

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

            voronoi: None,
            points: Vec::new(),
            centers: Vec::new(),
            corners: Vec::new(),
            edges: Vec::new(),

            delaunay_centers: Vec::new(),
            voronoi_centers: Vec::new(),
        }
    }

    /// Generate the biome map for the world.
    pub fn generate_world_biomes(&mut self, biome_registry: &BiomeRegistry) {
        // initialize generatable biomes
        let mut biomes: Vec<(RegistryId, BiomeDefinition)> = Vec::new();
        for (id, _name, def) in biome_registry.iter() {
            if def.can_generate {
                biomes.push((id, def.to_owned()));
            }
        }
        self.biome_map.generatable_biomes = biomes;

        let total = Instant::now();
        info!("starting biome generation");

        let size = (self.size_chunks_xz * CHUNK_DIM) as f64;

        let start = Instant::now();
        self.pick_biome_points(self.biome_point_count, size, size);
        self.make_diagram(size, size, self.points.clone());
        let duration = start.elapsed();
        info!("picking points & building graph took {:?}", duration);

        let start = Instant::now();
        self.assign_noise_and_oceans();
        let duration = start.elapsed();
        info!("height calculations took {:?}", duration);

        let start = Instant::now();
        self.calculate_downslopes();
        self.calculate_watersheds(biome_registry);
        self.create_rivers(biome_registry); // stack overflow???
        let duration = start.elapsed();
        info!("moisture calculations took {:?}", duration);

        let start = Instant::now();
        self.assign_biomes(biome_registry);
        self.blur_biomes(biome_registry);
        let duration = start.elapsed();
        info!("biome map lookup took {:?}", duration);

        let duration = total.elapsed();
        info!("biome generation took {:?} total", duration);
    }

    /// Generate a single chunk's blocks for the world.
    pub fn generate_chunk(
        &mut self,
        c_pos: AbsChunkPos,
        chunk: &mut PaletteStorage<BlockEntry>,
        block_registry: &BlockRegistry,
        biome_registry: &BiomeRegistry,
    ) {
        let mut blended = vec![SmallVec::new(); CHUNK_DIM2Z];

        let vparams: [i32; CHUNK_DIM2Z] = {
            let mut vparams: [MaybeUninit<i32>; CHUNK_DIM2Z] = unsafe { MaybeUninit::uninit().assume_init() };
            for (i, v) in vparams[..].iter_mut().enumerate() {
                let ix = (i % CHUNK_DIMZ) as i32;
                let iz = ((i / CHUNK_DIMZ) % CHUNK_DIMZ) as i32;
                self.get_biomes_at_point(&[ix + c_pos.x * CHUNK_DIM, iz + c_pos.z * CHUNK_DIM])
                    .unwrap_or(&SmallVec::<[BiomeEntry; EXPECTED_BIOME_COUNT]>::new())
                    .clone_into(&mut blended[(ix + iz * CHUNK_DIM) as usize]);
                let p = Self::elevation_noise(
                    IVec2::new(ix, iz),
                    IVec2::new(c_pos.x, c_pos.z),
                    biome_registry,
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
                let e = b.lookup(biome_registry).unwrap();
                let w = b.weight * e.block_influence;
                biomes.push((e, w));
            }
            // sort by block influence, then registry id if influence is same
            biomes.sort_by(|a, b| {
                a.1.partial_cmp(&b.1).unwrap_or_else(|| {
                    biome_registry
                        .search_object_to_id(a.0)
                        .cmp(&biome_registry.search_object_to_id(b.0))
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
                let result = (biome.rule_source)(&g_pos, &ctx, block_registry);
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

    fn pick_biome_points(&mut self, count: u32, x_size: f64, y_size: f64) {
        let range_x = Uniform::new(-x_size / 2.0, x_size / 2.0);
        let range_y = Uniform::new(-x_size / 2.0, y_size / 2.0);
        for _ in 0..count {
            let x = self.random.sample(range_x);
            let y = self.random.sample(range_y);
            self.points.push(Point { x, y });
        }
    }

    fn make_diagram(&mut self, x_size: f64, y_size: f64, points: Vec<Point>) {
        let diagram = VoronoiBuilder::default()
            .set_sites(points.clone())
            .set_lloyd_relaxation_iterations(2)
            .set_bounding_box(BoundingBox::new_centered(x_size, y_size))
            .build();
        self.voronoi = diagram;
        let diagram = self.voronoi.as_ref().unwrap();
        let edges = Self::make_edges(diagram, &mut self.delaunay_centers, &mut self.voronoi_centers);

        let mut center_lookup: HashMap<[i32; 2], Rc<RefCell<Center>>> = HashMap::new();

        for point in diagram.sites() {
            let point = DVec2::new(point.x, point.y);
            let center = Rc::new(RefCell::new(Center::new(point)));
            self.centers.push(center.clone());
            center_lookup.insert([point.x.round() as i32, point.y.round() as i32], center.clone());
        }

        for delaunay_center in self.delaunay_centers.iter_mut() {
            for pos in delaunay_center.center_locations.into_iter() {
                let center = center_lookup[&[pos.x.round() as i32, pos.y.round() as i32]].clone();
                delaunay_center.centers.push(center);
            }
        }

        let mut corner_map: Vec<Vec<Rc<RefCell<Corner>>>> = Vec::new();
        let mut make_corner = |point: DVec2| {
            let mut bucket = point.x.abs() as usize;
            while bucket <= point.x.abs() as usize + 2 {
                if corner_map.get(bucket).is_none() {
                    break;
                }
                for q in &corner_map[bucket] {
                    let dx = -q.borrow().point.x;
                    let dy = point.y - q.borrow().point.y;
                    if dx * dx + dy * dy < 1e-6 {
                        return q.clone();
                    }
                }
                bucket += 1;
            }

            let bucket = point.x.abs() as usize + 1;
            while corner_map.get(bucket).is_none() {
                corner_map.push(Vec::new());
            }
            let q = Corner::new(point);
            //q.border = q.point.x == -x_size/2.0 || q.point.x == x_size/2.0
            //            || q.point.y == -y_size/2.0 || q.point.y == y_size/2.0;
            let q = Rc::new(RefCell::new(q));
            self.corners.push(q.clone());
            corner_map[bucket].push(q.clone());
            q
        };

        let add_to_corner_list = |v: &mut Vec<Rc<RefCell<Corner>>>, x: &Option<Rc<RefCell<Corner>>>| {
            if x.is_some() && !v.iter().any(|y| Rc::ptr_eq(y, x.as_ref().unwrap())) {
                v.push(x.clone().unwrap());
            }
        };
        let add_to_center_list = |v: &mut Vec<Rc<RefCell<Center>>>, x: &Option<Rc<RefCell<Center>>>| {
            if x.is_some() && !v.iter().any(|y| Rc::ptr_eq(y, x.as_ref().unwrap())) {
                v.push(x.clone().unwrap());
            }
        };

        for (delaunay_edge, voronoi_edge) in edges {
            let mut edge = Edge::new();
            edge.midpoint = voronoi_edge.0.lerp(voronoi_edge.1, 0.5);

            // Edges point to corners. Edges point to centers.
            edge.v0 = Some(make_corner(voronoi_edge.0));
            edge.v1 = Some(make_corner(voronoi_edge.1));
            edge.d0 = center_lookup
                .get(&[delaunay_edge.0.x.round() as i32, delaunay_edge.0.y.round() as i32])
                .cloned();
            edge.d1 = center_lookup
                .get(&[delaunay_edge.1.x.round() as i32, delaunay_edge.1.y.round() as i32])
                .cloned();

            let rc = Rc::new(RefCell::new(edge));

            // Centers point to edges. Corners point to edges.
            if let Some(d0) = &rc.borrow().d0 {
                d0.borrow_mut().borders.push(rc.clone());
            }
            if let Some(d1) = &rc.borrow().d1 {
                d1.borrow_mut().borders.push(rc.clone());
            }
            if let Some(v0) = &rc.borrow().v0 {
                v0.borrow_mut().protrudes.push(rc.clone());
            }
            if let Some(v1) = &rc.borrow().v1 {
                v1.borrow_mut().protrudes.push(rc.clone());
            }

            // Centers point to centers.
            if let (Some(d0), Some(d1)) = (&rc.borrow().d0, &rc.borrow().d1) {
                add_to_center_list(&mut d0.borrow_mut().neighbors, &Some(d1.clone()));
                add_to_center_list(&mut d1.borrow_mut().neighbors, &Some(d0.clone()));
            }

            // Corners point to corners
            if let (Some(v0), Some(v1)) = (&rc.borrow().v0, &rc.borrow().v1) {
                add_to_corner_list(&mut v0.borrow_mut().adjacent, &Some(v1.clone()));
                add_to_corner_list(&mut v1.borrow_mut().adjacent, &Some(v0.clone()));
            }

            // Centers point to corners
            if let Some(d0) = &rc.borrow().d0 {
                add_to_corner_list(&mut d0.borrow_mut().corners, &rc.borrow().v0);
                add_to_corner_list(&mut d0.borrow_mut().corners, &rc.borrow().v1);
            }

            // Centers point to corners
            if let Some(d1) = &rc.borrow().d1 {
                add_to_corner_list(&mut d1.borrow_mut().corners, &rc.borrow().v0);
                add_to_corner_list(&mut d1.borrow_mut().corners, &rc.borrow().v1);
            }

            // Corners point to centers
            if let Some(v0) = &rc.borrow().v0 {
                add_to_center_list(&mut v0.borrow_mut().touches, &rc.borrow().d0);
                add_to_center_list(&mut v0.borrow_mut().touches, &rc.borrow().d1);
            }
            if let Some(v1) = &rc.borrow().v1 {
                add_to_center_list(&mut v1.borrow_mut().touches, &rc.borrow().d0);
                add_to_center_list(&mut v1.borrow_mut().touches, &rc.borrow().d1);
            }

            self.edges.push(rc);
        }

        for voronoi_center in self.voronoi_centers.iter_mut() {
            for pos in voronoi_center.corner_locations.iter() {
                voronoi_center.corners.push(make_corner(*pos));
            }
        }
    }

    /// returns: \[(delaunay edges, voronoi edges)\]
    fn make_edges(
        voronoi: &Voronoi,
        delaunay_centers: &mut Vec<DelaunayCenter>,
        voronoi_centers: &mut Vec<VoronoiCenter>,
    ) -> Vec<(PointEdge, PointEdge)> {
        let points = voronoi.sites().iter().map(|p| DVec2::new(p.x, p.y)).collect_vec();
        let mut list_of_delaunay_edges: Vec<PointEdge> = Vec::new();

        let triangles = &voronoi.triangulation().triangles;
        let triangles = (0..triangles.len() / 3)
            .map(|t| {
                [
                    points[triangles[3 * t]],
                    points[triangles[3 * t + 1]],
                    points[triangles[3 * t + 2]],
                ]
            })
            .collect_vec();

        for (site, triangle) in triangles.into_iter().enumerate() {
            for e in TRIANGLE_VERTICES {
                // for all edges of triangle
                let vertex_1 = triangle[e.0];
                let vertex_2 = triangle[e.1];
                list_of_delaunay_edges.push(PointEdge(vertex_1, vertex_2)); // always lesser index first
            }
            let center_point = &voronoi.vertices()[site];
            let center_point = DVec2::new(center_point.x, center_point.y);
            delaunay_centers.push(DelaunayCenter {
                point: center_point,
                center_locations: triangle,
                centers: SmallVec::new(),
            });
        }

        let mut list_of_voronoi_edges: Vec<PointEdge> = Vec::new();

        for cell in voronoi.iter_cells() {
            let vertices = cell
                .iter_vertices()
                .map(|p| DVec2::new(p.x, p.y))
                .collect::<Vec<DVec2>>();
            for i in 0..vertices.len() {
                let vertex_1 = vertices[i];
                let vertex_2 = vertices[(i + 1) % vertices.len()];
                list_of_voronoi_edges.push(PointEdge(vertex_1, vertex_2));
            }
            let center_point = cell.site_position();
            let center_point = DVec2::new(center_point.x, center_point.y);
            voronoi_centers.push(VoronoiCenter {
                point: center_point,
                corner_locations: vertices,
                corners: SmallVec::new(),
            });
        }

        list_of_delaunay_edges
            .iter()
            .copied()
            .zip(list_of_voronoi_edges.iter().copied())
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
    fn assign_noise_and_oceans(&mut self) {
        let mut queue = VecDeque::new();

        for p in &mut self.centers {
            let mut p_b = p.borrow_mut();
            // assign noise parameters based on node position
            p_b.noise = Self::make_noise(&self.noises, p_b.point);
            let mut num_water = 0;

            for q in p_b.corners.clone() {
                let q = q.borrow();
                if q.border {
                    p_b.ocean = true;
                    p_b.water = true;
                    queue.push_back(p.clone());
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
        for p in &mut self.centers {
            let mut num_ocean = 0;
            let mut num_land = 0;
            for r in &p.borrow().neighbors {
                if r.borrow().ocean {
                    num_ocean += 1;
                }
                if !r.borrow().water {
                    num_land += 1;
                }
            }
            p.borrow_mut().coast = num_land > 0 && num_ocean > 0;
        }

        // Set the corner attributes based on the computed polygon
        // attributes. If all polygons connected to this corner are
        // ocean, then it's ocean; if all are land, then it's land;
        // otherwise it's coast.
        for q in &mut self.corners {
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

        for e in &self.edges {
            let mut e_b = e.borrow_mut();
            e_b.noise = Self::make_noise(&self.noises, e_b.midpoint);
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

    fn assign_biomes(&mut self, biome_registry: &BiomeRegistry) {
        // go over all centers and assign biomes to them based on noise & other parameters.
        for p in &mut self.centers {
            let mut center = p.borrow_mut();

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
                continue;
            }
            if center.ocean {
                center.biome = Some(
                    biome_registry
                        .lookup_name_to_object(OCEAN_BIOME_NAME.as_ref())
                        .unwrap()
                        .0,
                );
                continue;
            } else if center.water {
                // TODO make lake biome(s)
                center.biome = Some(
                    biome_registry
                        .lookup_name_to_object(OCEAN_BIOME_NAME.as_ref())
                        .unwrap()
                        .0,
                );
                continue;
            } else if center.coast {
                center.biome = Some(
                    biome_registry
                        .lookup_name_to_object(BEACH_BIOME_NAME.as_ref())
                        .unwrap()
                        .0,
                );
                continue;
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
    }

    fn blur_biomes(&mut self, biome_registry: &BiomeRegistry) {
        let (void_id, _) = biome_registry.lookup_name_to_object(VOID_BIOME_NAME.as_ref()).unwrap();

        let size = self.size_blocks_xz();
        for (x, y) in iproduct!(-size..size, -size..size) {
            self.find_biomes_at_point(DVec2::new(x as f64, y as f64), void_id);
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

        let p = [point.x.round() as i32, point.y.round() as i32];
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

    /// Get the Voronoi diagram for this generator.
    pub fn voronoi(&self) -> &Voronoi {
        self.voronoi
            .as_ref()
            .expect("voronoi map should exist, but it somehow failed to generate.")
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

#[derive(Clone, PartialEq, Debug)]
struct DelaunayCenter {
    point: DVec2,
    center_locations: [DVec2; 3],
    centers: SmallVec<[Rc<RefCell<Center>>; 3]>,
}

#[derive(Clone, PartialEq, Debug)]
struct VoronoiCenter {
    point: DVec2,
    corner_locations: Vec<DVec2>,
    corners: SmallVec<[Rc<RefCell<Corner>>; 6]>,
}
