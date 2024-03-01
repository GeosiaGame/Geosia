use std::{cell::RefCell, collections::VecDeque, mem::MaybeUninit, rc::Rc, time::Instant};

use bevy::{ecs::system::ResMut, utils::hashbrown::HashMap};
use bevy_math::{DVec2, IVec2, IVec3};
use noise::OpenSimplex;
use ocg_schemas::{coordinates::{AbsChunkPos, InChunkPos, CHUNK_DIM, CHUNK_DIM2Z, CHUNK_DIMZ}, dependencies::{itertools::{iproduct, Itertools}, smallvec::{smallvec, SmallVec}}, registry::RegistryId, voxel::{biome::{biome_map::{BiomeMap, EXPECTED_BIOME_COUNT, GLOBAL_BIOME_SCALE, GLOBAL_SCALE_MOD}, BiomeDefinition, BiomeEntry, BiomeRegistry, Noises, PLAINS_BIOME_NAME}, chunk_storage::{ChunkStorage, PaletteStorage}, generation::{fbm_noise::Fbm, positional_random::PositionalRandomFactory, Context, Noise4DTo2D}, voxeltypes::{BlockEntry, BlockRegistry}}};
use rand::{distributions::Uniform, Rng, SeedableRng};
use rand_xoshiro::Xoshiro128StarStar;
use serde::{Deserialize, Serialize};
use voronoice::*;

use crate::voxel::biomes::{BEACH_BIOME_NAME, OCEAN_BIOME_NAME, RIVER_BIOME_NAME};


const TRIANGLE_VERTICES: [(usize, usize); 3] = [(0,1),(1,2),(2,0)];
pub const WATER_TRESHOLD: f64 = 0.3;

/// gosh this is jank why can't I just use DVec2
fn lerp(start: &Point, end: &Point, value: f64) -> Point {
    let mul = |p: &Point, v: f64| Point {x: p.x * v, y: p.y * v};
    let add = |p1: &Point, p2: &Point| Point {x: p1.x + p2.x, y: p1.y + p2.y};
    add(&mul(start, 1.0 - value),& mul(end, value))
}

pub struct NewGenerator<'a> {
    seed: u64,
    size_chunks_xz: i32,
    biome_point_count: u32,

    random: Xoshiro128StarStar,
    biome_map: ResMut<'a, BiomeMap>,
    noises: Noises,

    voronoi: Option<Voronoi>,
    points: Vec<Point>,
    centers: Vec<Rc<RefCell<Center>>>,
    corners: Vec<Rc<RefCell<Corner>>>,
    edges: Vec<Rc<RefCell<Edge>>>,
}

impl<'a> NewGenerator<'a> {
    pub fn new(seed: u64, size_chunks_xz: i32, biome_point_count: u32, biome_map: ResMut<'a, BiomeMap>) -> Self {
        let seed_int = seed as u32;
        Self {
            seed,
            size_chunks_xz,
            biome_point_count,

            random: Xoshiro128StarStar::seed_from_u64(seed),
            biome_map,
            noises: Noises {
                base_terrain_noise: Box::new(Fbm::<OpenSimplex>::new(seed_int).set_octaves(vec![-4.0, 1.0, 1.0, 0.0])),
                elevation_noise: Box::new(Fbm::<OpenSimplex>::new(seed_int + 1).set_octaves(vec![1.0, 2.0, 2.0, 1.0])),
                temperature_noise: Box::new(Fbm::<OpenSimplex>::new(seed_int + 2).set_octaves(vec![1.0, 2.0, 2.0, 1.0])),
                moisture_noise: Box::new(Fbm::<OpenSimplex>::new(seed_int + 3).set_octaves(vec![1.0, 2.0, 2.0, 1.0])),
            },

            voronoi: None,
            points: vec![],
            centers: vec![],
            corners: vec![],
            edges: vec![],
        }
    }

    pub fn generate_world_biomes(&mut self, biome_registry: &BiomeRegistry) {
        // initialize generatable biomes
        let mut biomes: Vec<(RegistryId, BiomeDefinition)> = Vec::new();
        for def in biome_registry.get_objects_ids().iter() {
            if def.1.can_generate {
                biomes.push((*def.0, def.1.to_owned()));
            }
        }
        self.biome_map.generatable_biomes = biomes;

        let total = Instant::now();
        println!("starting biome generation");

        let size = (self.size_chunks_xz * CHUNK_DIM) as f64;

        let start = Instant::now();
        self.pick_biome_points(self.biome_point_count, size, size);
        self.make_diagram(size, size, self.points.clone());
        let duration = start.elapsed();
        println!("picking points & building graph took {:?}", duration);

        let start = Instant::now();
        self.assign_noise_and_oceans(biome_registry);
        //self.assign_polygon_noise();
        let duration = start.elapsed();
        println!("height calculations took {:?}", duration);

        let start = Instant::now();
        self.calculate_downslopes();
        //self.calculate_watersheds(biome_registry); // very broken. also doesn't do anything rn.
        //self.create_rivers(biome_registry); // stack overflow???
        let duration = start.elapsed();
        println!("moisture calculations took {:?}", duration);

        let start = Instant::now();
        self.assign_biomes();
        let duration = start.elapsed();
        println!("biome map lookup took {:?}", duration);

        let duration = total.elapsed();
        println!("biome generation took {:?} total", duration);
    }


    pub fn generate_chunk(&mut self, c_pos: AbsChunkPos, chunk: &mut PaletteStorage<BlockEntry>, block_registry: &BlockRegistry, biome_registry: &BiomeRegistry) {
        let (plains_id, _) = biome_registry.lookup_name_to_object(PLAINS_BIOME_NAME.as_ref()).unwrap();

        let mut blended = vec![smallvec![]; CHUNK_DIM2Z];
        for (ix, iz) in iproduct!(0..CHUNK_DIM, 0..CHUNK_DIM) {
            let point = Point {x: (c_pos.x * CHUNK_DIM + ix) as f64, y: (c_pos.y * CHUNK_DIM + iz) as f64};
            blended[(ix + iz * CHUNK_DIM) as usize] = self.find_biomes_at_point(&point, plains_id);
        }

        let vparams: [i32; CHUNK_DIM2Z] = {
            let mut vparams: [MaybeUninit<i32>; CHUNK_DIM2Z] =
                unsafe { std::mem::MaybeUninit::uninit().assume_init() };
            for (i, v) in vparams[..].iter_mut().enumerate() {
                let in_chunk_x = (i % CHUNK_DIMZ) as i32;
                let in_chunk_z = ((i / CHUNK_DIMZ) % CHUNK_DIMZ) as i32;
                let p = Self::elevation_noise(IVec2::new(in_chunk_x, in_chunk_z), IVec2::new(c_pos.x, c_pos.z), biome_registry, &blended, &mut self.noises).round() as i32;
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

    fn elevation_noise(in_chunk_pos: IVec2, chunk_pos: IVec2, biome_registry: &BiomeRegistry, blended: &Vec<SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>>, noises: &mut Noises) -> f64 {
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

    

    fn pick_biome_points(&mut self, count: u32, x_size: f64, y_size: f64) {
        let range_x = Uniform::new(-x_size/2.0, x_size/2.0);
        let range_y = Uniform::new(-x_size/2.0, y_size/2.0);
        for _ in 0..count {
            let x = self.random.sample(range_x);
            let y = self.random.sample(range_y);
            self.points.push(Point {x, y});
        }
        println!("points: {:?}", self.points)
    }

    fn make_diagram(&mut self, x_size: f64, y_size: f64, points: Vec<Point>) {
        let diagram = VoronoiBuilder::default()
            .set_sites(points.clone())
            .set_lloyd_relaxation_iterations(2)
            .set_bounding_box(BoundingBox::new_centered(x_size, y_size))
            .build();
        self.voronoi = diagram;
        let diagram = self.voronoi.as_ref().unwrap();
        let mut center_lookup: HashMap<[i32; 2], Rc<RefCell<Center>>> = HashMap::new();

        for point in &points {
            let center = Rc::new(RefCell::new(Center::new(point.clone())));
            self.centers.push(center.clone());
            center_lookup.insert([point.x.round() as i32, point.y.round() as i32], center);
        }

        let mut corner_map: Vec<Vec<Rc<RefCell<Corner>>>> = vec![];
        let mut make_corner = |point: Point| {
            let mut bucket = point.x.abs() as i32 - 1;
            while bucket < point.x.abs() as i32 + 1 {
                if corner_map.get(bucket as usize).is_none() {
                    break;
                }
                for q in &corner_map[bucket as usize] {
                    let dx = point.x - q.borrow().point.x;
                    let dy = point.y - q.borrow().point.y;
                    if dx*dx + dy*dy < 1e-6 {
                        return q.clone();
                    }
                }
                bucket += 1;
            }

            let bucket = point.x.abs() as usize;
            while corner_map.get(bucket).is_none() {
                corner_map.push(vec![]);
            }
            let mut q = Corner::new(point);
            q.border = q.point.x == -x_size || q.point.x == x_size
                        || q.point.y == -y_size || q.point.y == y_size;
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

        let edges = Self::make_edges(diagram, &points, x_size, y_size);
        for (delaunay_edge, voronoi_edge) in edges {
            let mut edge = Edge::new();
            edge.midpoint = lerp(&voronoi_edge.0, &voronoi_edge.1, 0.5);

            // Edges point to corners. Edges point to centers. 
            edge.v0 = Some(make_corner(voronoi_edge.0));
            edge.v1 = Some(make_corner(voronoi_edge.1));
            edge.d0 = center_lookup.get(&[delaunay_edge.0.x.round() as i32, delaunay_edge.0.y.round() as i32]).cloned();
            edge.d1 = center_lookup.get(&[delaunay_edge.1.x.round() as i32, delaunay_edge.1.y.round() as i32]).cloned();

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
            if let Some(d0) = &rc.borrow().d0 && let Some(d1) = &rc.borrow().d1 {
                add_to_center_list(&mut d0.borrow_mut().neighbors, &Some(d1.clone()));
                add_to_center_list(&mut d1.borrow_mut().neighbors, &Some(d0.clone()));
            }

            // Corners point to corners
            if let Some(v0) = &rc.borrow().v0 && let Some(v1) = &rc.borrow().v1 {
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
    }
    
    /// returns: \[(delaunay edges, voronoi edges)\]
    fn make_edges(voronoi: &Voronoi, points: &Vec<Point>, x_size: f64, y_size: f64) -> Vec<(PointEdge, PointEdge)> {
        let mut list_of_delaunay_edges: Vec<PointEdge> = vec![];
    
        let triangles = &voronoi.triangulation().triangles;
        let triangles: Vec<[&Point; 3]> = (0..triangles.len() / 3).map(|t| [
            &points[triangles[3 * t + 0]],
            &points[triangles[3 * t + 1]],
            &points[triangles[3 * t + 2]]]
        ).collect();

        for triangle in triangles {
            for e in TRIANGLE_VERTICES { // for all edges of triangle
                let vertex_1 = triangle[e.0].to_owned();
                //let vertex_1 = Point {x: vertex_1.x * x_size + 0.5, y: vertex_1.y * y_size + 0.5};
                let vertex_2 = triangle[e.1].to_owned();
                //let vertex_2 = Point {x: vertex_2.x * x_size + 0.5, y: vertex_2.y * y_size + 0.5};
                list_of_delaunay_edges.push(PointEdge(vertex_1,vertex_2)); // always lesser index first
            }
        }

        let mut list_of_voronoi_edges: Vec<PointEdge> = vec![];

        for cell in voronoi.iter_cells() {
            let vertices = cell.iter_vertices().collect::<Vec<&Point>>();
            for i in 0..vertices.len() {
                let vertex_1 = vertices[i].to_owned();
                //let vertex_1 = Point {x: vertex_1.x * x_size + 0.5, y: vertex_1.y * y_size + 0.5};
                let vertex_2 = vertices[(i+1) % vertices.len()].to_owned();
                //let vertex_2 = Point {x: vertex_2.x * x_size + 0.5, y: vertex_2.y * y_size + 0.5};
                list_of_voronoi_edges.push(PointEdge(vertex_1,vertex_2));
            }
        }

        list_of_delaunay_edges.iter().cloned().zip(list_of_voronoi_edges.iter().cloned()).collect_vec()
    }

    fn make_noise(noises: &Noises, point: &Point) -> NoiseValues {
        let point = [point.x * 10.0, point.y * 10.0];
        let elevation = Self::map_range((-1.5, 1.5), (0.0, 5.0), noises.elevation_noise.get_2d(point));
        let temperature = Self::map_range((-1.5, 1.5), (0.0, 5.0), noises.temperature_noise.get_2d(point));
        let moisture: f64 = Self::map_range((-1.5, 1.5), (0.0, 5.0), noises.moisture_noise.get_2d(point));

        NoiseValues { elevation, temperature, moisture }
    }

    /// Compute polygon attributes 'ocean' and 'water' based on the
    /// corner attributes. Count the water corners per
    /// polygon. Oceans are all polygons connected to the edge of the
    /// map. In the first pass, mark the edges of the map as ocean;
    /// in the second pass, mark any water-containing polygon
    /// connected to an ocean as ocean.
    fn assign_noise_and_oceans(&mut self, biome_registry: &BiomeRegistry) {
        let (ocean_id, ocean) = biome_registry.lookup_name_to_object(OCEAN_BIOME_NAME.as_ref()).unwrap();
        let (beach_id, _) = biome_registry.lookup_name_to_object(BEACH_BIOME_NAME.as_ref()).unwrap();

        let mut queue = VecDeque::new();

        for p in &mut self.centers {
            let mut p_b = p.borrow_mut();
            // assign noise parameters based on node position
            p_b.noise = Self::make_noise(&self.noises, &p_b.point);
            let mut num_water = 0;

            for q in &p_b.corners {
                let q = q.borrow();
                if q.border {
                    queue.push_back(p.clone());
                }
                if ocean.moisture.contains(q.noise.moisture) && ocean.elevation.contains(q.noise.elevation) {
                    num_water += 1;
                }
            }
            p_b.biome = if num_water as f64 >= p_b.corners.len() as f64 * WATER_TRESHOLD {
                Some(ocean_id)
            } else {
                None
            };
        }
        while queue.len() > 0 {
            let p = queue.pop_back();
            if p.is_none() {
                break;
            }
            for r in &p.unwrap().borrow().neighbors {
                let mut r_b = r.borrow_mut();
                if ocean.moisture.contains(r_b.noise.moisture) && ocean.elevation.contains(r_b.noise.elevation) && r_b.biome != Some(ocean_id) {
                    r_b.biome = Some(ocean_id);
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
                if r.borrow().biome == Some(ocean_id) {
                    num_ocean += 1;
                } else  {
                    num_land += 1;
                }
            }
            p.borrow_mut().biome = if num_land > 0 && num_ocean > 0 {
                Some(beach_id)
            } else {
                None
            };
        }

        // Set the corner attributes based on the computed polygon
        // attributes. If all polygons connected to this corner are
        // ocean, then it's ocean; if all are land, then it's land;
        // otherwise it's coast.
        for q in &mut self.corners {
            let mut q_b = q.borrow_mut();
            q_b.noise = Self::make_noise(&self.noises, &q_b.point);
            let mut num_ocean = 0;
            let mut num_land = 0;
            for p in &q_b.touches {
                if p.borrow().biome == Some(ocean_id) {
                    num_ocean += 1;
                } else  {
                    num_land += 1;
                }
            }
            q_b.biome = if num_ocean == q_b.touches.len() {
                Some(ocean_id)
            } else if num_land > 0 && num_ocean > 0 {
                Some(beach_id)
            } else {
                None
            };
        }
    }

    fn assign_polygon_noise(&mut self) {
        for p in &self.centers {
            let mut p_b = p.borrow_mut();
            let mut sum_elevation = 0.0;
            let mut sum_temperature = 0.0;
            let mut sum_moisture = 0.0;
            for q in &p_b.corners {
                sum_elevation += q.borrow().noise.elevation;
                sum_temperature += q.borrow().noise.temperature;
                sum_moisture += q.borrow().noise.moisture;
            }
            p_b.noise.elevation = sum_elevation / p_b.corners.len() as f64;
            p_b.noise.temperature = sum_temperature / p_b.corners.len() as f64;
            p_b.noise.moisture = sum_moisture / p_b.corners.len() as f64;
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
    fn calculate_watersheds(&mut self, biome_registry: &BiomeRegistry) {
        let (ocean_id, _) = biome_registry.lookup_name_to_object(OCEAN_BIOME_NAME.as_ref()).unwrap();
        let (beach_id, _) = biome_registry.lookup_name_to_object(BEACH_BIOME_NAME.as_ref()).unwrap();

        // Initially the watershed pointer points downslope one step.
        for q in &self.corners {
            let mut q_b = q.borrow_mut();
            q_b.watershed = Some(q.clone());
            if q_b.biome != Some(ocean_id) &&& q_b.biome != &Some(beach_id) {
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
                if Rc::ptr_eq(q_b.watershed.as_ref().unwrap(), &q) {
                    continue;
                }
                if q_b.biome != Some(ocean_id) && q_b.biome != Some(beach_id) && q_b.watershed.as_ref().unwrap().borrow().biome != Some(beach_id) {
                    let r = q_b.watershed.as_ref().unwrap();
                    if r.borrow().biome != Some(ocean_id) {
                        q_b.watershed = Some(r.clone());
                        changed = true;
                    }
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
        let (ocean_id, _) = biome_registry.lookup_name_to_object(OCEAN_BIOME_NAME.as_ref()).unwrap();
        let (beach_id, _) = biome_registry.lookup_name_to_object(BEACH_BIOME_NAME.as_ref()).unwrap();
        let (river_id, _) = biome_registry.lookup_name_to_object(RIVER_BIOME_NAME.as_ref()).unwrap();

        for _ in 0..(self.size_chunks_xz / 2) {
            let mut q = self.corners[self.random.gen_range(0..self.corners.len())].clone();
            if q.borrow_mut().biome == Some(ocean_id) || q.borrow_mut().noise.elevation < 0.3 || q.borrow_mut().noise.elevation > 0.9 {
                continue;
            }
            while q.borrow().biome != Some(beach_id) {
                if Rc::ptr_eq(&q, q.borrow().downslope.as_ref().unwrap()) {
                    continue;
                }
                let edge = Self::lookup_edge_from_corner(&q, &q.borrow().downslope.as_ref().unwrap()).unwrap();
                edge.borrow_mut().river += 1;
                q.borrow_mut().river += 1;
                q.borrow_mut().downslope.as_mut().unwrap().borrow_mut().river += 1;
                q.borrow_mut().biome = Some(river_id);

                q = q.clone().borrow_mut().downslope.as_ref().unwrap().clone();
            }

        }
    }

    fn assign_biomes(&mut self) {
        for p in &mut self.centers {
            let mut center = p.borrow_mut();
            if center.biome.is_some() {
                continue;
            }
            let mut found = false;
            for (id, biome) in &self.biome_map.generatable_biomes {
                if biome.elevation.contains(center.noise.elevation) && biome.temperature.contains(center.noise.temperature) && biome.moisture.contains(center.noise.moisture) {
                    //println!("biome at point {:?} is {biome}, noise values: {:?}", center.point, center.noise);
                    center.biome = Some(*id);
                    found = true;
                    break;
                }
            }
            if !found {
                println!("found no biome for point {:?}, noise values: {:?}. Picking randomly.", center.point, center.noise);
                let index = self.random.gen_range(0..self.biome_map.generatable_biomes.len());
                center.biome = Some(self.biome_map.generatable_biomes[index].0);
            }
        }
    }

    pub fn lookup_edge_from_corner(q: &Rc<RefCell<Corner>>, s: &Rc<RefCell<Corner>>) -> Option<Rc<RefCell<Edge>>> {
        for edge in &q.borrow().protrudes {
            if edge.borrow().v0 == Some(s.clone()) || edge.borrow().v1 == Some(s.clone()) {
                return Some(edge.clone());
            }
        }
        None
    }

    pub fn find_biomes_at_point(&mut self, point: &Point, default: RegistryId) -> SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]> {
        let sqr_distance = |a: &Point, b: &Point| {
            let x = b.x - a.x;
            let y = b.y - a.y;

            x*x + y*y
        };

        for center in &self.centers {
            let center = center.borrow_mut();
            let edge_points = center.corners.iter()
                .map(|p| p.borrow().point.clone())
                .collect_vec();
            
            if Self::contains_point(&edge_points, point) {
                let mut to_blend: SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]> = smallvec![];

                to_blend.push(BiomeEntry {id: center.biome.unwrap_or(default), weight: 1.0 });

                for corner in &center.corners {
                    let corner = corner.borrow();
                    let total_distance_sqr = sqr_distance(&center.point, &corner.point).abs();
                    let distance_sqr = sqr_distance(point, &corner.point).abs();
                    let percent = distance_sqr.min(total_distance_sqr) / total_distance_sqr.max(distance_sqr);
                    // weight at this point is distance from neighbor as % of travel
                    let blend = to_blend.iter_mut().find(|e| e.id == corner.biome.unwrap_or(default));
                    if let Some(blend) = blend {
                        blend.weight += percent;
                    } else {
                        to_blend.push(BiomeEntry {id: corner.biome.unwrap_or(default), weight: percent});
                    }
                }

                let p = [point.x.round() as i32, point.y.round() as i32];
                self.biome_map.map.insert(p, to_blend);
                return self.biome_map.map[&p].clone();
            }
        }
        smallvec![]
    }

    
    pub fn get_biomes_at_point(&self, point: &Point) -> Option<&SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>> {
        for center in &self.centers {
            let center = center.borrow();
            let edge_points = center.corners.iter()
                .map(|p| p.borrow().point.clone())
                .collect_vec();
            
            if Self::contains_point(&edge_points, point) {
                let p = [center.point.x.round() as i32, center.point.y.round() as i32];
                return self.biome_map.map.get(&p)
            }
        }
        None
    }

    fn contains_point(points: &Vec<Point>, test: &Point) -> bool {
        let v_sub = |p1: &Point, p2: &Point| Point { x: p1.x - p2.x, y: p1.y - p2.y };
        let cosine_sign = |p1: &Point, p2: &Point| p1.x*p2.y - p1.y*p2.x;
        let get_side = |a: &Point, b: &Point| -> Option<Side> {
            let x = cosine_sign(a, b);
            if x < 0.0 {
                Some(Side::Left)
            } else if x > 0.0 {
                Some(Side::Right)
            } else {
                None
            }
        };

        let mut previvous_side = None;
        let point_count = points.len();
        for n in 0..point_count {
            let a = &points[n];
            let b = &points[(n+1) % point_count];
            let affine_segment = v_sub(b, a);
            let affine_point = v_sub(test, a);
            let current_side = get_side(&affine_segment, &affine_point);
            if current_side.is_none() {
                return false;
            } else if previvous_side.is_none() {
                previvous_side = current_side;
            } else if previvous_side != current_side {
                return false;
            } else {
                return true;
            }
        }
        false
    }

    fn map_range(from_range: (f64, f64), to_range: (f64, f64), s: f64) -> f64 {
        to_range.0 + (s - from_range.0) * (to_range.1 - to_range.0) / (from_range.1 - from_range.0)
    }

    pub fn voronoi(&self) -> &Voronoi {
        self.voronoi.as_ref().unwrap()
    }

    pub fn size_blocks_xz(&self) -> i32 {
        self.size_chunks_xz * CHUNK_DIM
    }

    pub fn biome_map(&self) -> &BiomeMap {
        &self.biome_map
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Debug)]
struct NoiseValues {
    elevation: f64,
    temperature: f64,
    moisture: f64,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Center {
    point: Point,
    noise: NoiseValues,
    biome: Option<RegistryId>,

    neighbors: Vec<Rc<RefCell<Center>>>,
    borders: Vec<Rc<RefCell<Edge>>>,
    corners: Vec<Rc<RefCell<Corner>>>,
}

impl Center {
    pub fn new(point: Point) -> Center {
        Self {
            point,
            noise: NoiseValues::default(),
            biome: None,

            neighbors: vec![],
            borders: vec![],
            corners: vec![],
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
struct PointEdge(Point, Point);

#[derive(Clone, PartialEq, Debug)]
pub struct Edge {
    d0: Option<Rc<RefCell<Center>>>, d1: Option<Rc<RefCell<Center>>>,   // Delaunay edge
    v0: Option<Rc<RefCell<Corner>>>, v1: Option<Rc<RefCell<Corner>>>,   // Voronoi edge
    midpoint: Point,                                                    // halfway between v0,v1

    river: i32,                                                         // 0 if no river, or volume of water in river
}

impl Edge {
    pub fn new() -> Edge {
        Self {
            d0: None,
            d1: None,
            v0: None,
            v1: None,
            midpoint: Point::default(),

            river: 0,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Corner {
    point: Point,
    noise: NoiseValues,
    border: bool,
    biome: Option<RegistryId>,

    downslope: Option<Rc<RefCell<Corner>>>,
    watershed: Option<Rc<RefCell<Corner>>>,
    watershed_size: i32,

    river: i32,

    touches: Vec<Rc<RefCell<Center>>>,
    protrudes: Vec<Rc<RefCell<Edge>>>,
    adjacent: Vec<Rc<RefCell<Corner>>>,
}

impl Corner {
    pub fn new(position: Point) -> Corner {
        Self { 
            noise: NoiseValues::default(),
            point: position,
            border: false,
            biome: None,

            downslope: None,
            watershed: None,
            watershed_size: 0,

            river: 0,

            touches: vec![],
            protrudes: vec![],
            adjacent: vec![],
        }
    }
}

#[derive(PartialEq)]
pub enum Side {
    Left, Right
}
