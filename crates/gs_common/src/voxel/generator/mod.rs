//! Standard world generator.

use std::cell::RefMut;
use std::fmt::Debug;
use std::ops::Deref;
use std::{cell::RefCell, cmp::Ordering, collections::VecDeque, mem::MaybeUninit, rc::Rc, time::Instant};

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
        chunk_storage::{ChunkStorage, PaletteStorage},
        generation::{fbm_noise::Fbm, positional_random::PositionalRandomFactory, Context, Noise4DTo2D},
        voxeltypes::{BlockEntry, BlockRegistry},
    },
};
use noise::OpenSimplex;
use rand::{distributions::Uniform, Rng, SeedableRng};
use rand_xoshiro::Xoshiro128StarStar;
use serde::{Deserialize, Serialize};
use voronoice::*;

use crate::voxel::biomes::{BEACH_BIOME_NAME, OCEAN_BIOME_NAME, RIVER_BIOME_NAME};

/// World size of the +X & +Z axis, in chunks.
pub const WORLD_SIZE_XZ: i32 = 8;
/// World size of the +Y axis, in chunks.
pub const WORLD_SIZE_Y: i32 = 1;

const TRIANGLE_VERTICES: [(usize, usize); 3] = [(0, 1), (1, 2), (2, 0)];
const LAKE_TRESHOLD: f64 = 0.3;

const BIOME_BLEND_RADIUS: f64 = 16.0;

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
    centers: Vec<Center>,
    corners: Vec<Corner>,
    edges: Vec<Edge>,

    delaunay_centers: Vec<DelaunayCenter>,
    voronoi_centers: Vec<VoronoiCenter>,
}

impl Debug for StdGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdGenerator")
            .field("seed", &self.seed)
            .field("size_chunks_xz", &self.size_chunks_xz)
            .field("biome_point_count", &self.biome_point_count)
            .finish()
    }
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
        println!("starting biome generation");

        let size = (self.size_chunks_xz * CHUNK_DIM) as f64;

        let start = Instant::now();
        let bpc = self.biome_point_count;
        self.pick_biome_points(bpc, size, size);
        let points = self.points.clone();
        self.make_diagram(size, size, points);
        let duration = start.elapsed();
        println!("picking points & building graph took {:?}", duration);

        let start = Instant::now();
        self.assign_noise_and_oceans();
        let duration = start.elapsed();
        println!("height calculations took {:?}", duration);

        let start = Instant::now();
        self.calculate_downslopes();
        self.calculate_watersheds(biome_registry);
        self.create_rivers(biome_registry); // stack overflow???
        let duration = start.elapsed();
        println!("moisture calculations took {:?}", duration);

        let start = Instant::now();
        self.assign_biomes(biome_registry);
        self.blur_biomes(biome_registry);
        let duration = start.elapsed();
        println!("biome map lookup took {:?}", duration);

        let duration = total.elapsed();
        println!("biome generation took {:?} total", duration);
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

        let mut center_lookup: HashMap<[i32; 2], usize> = HashMap::new();

        for point in diagram.sites() {
            let point = DVec2::new(point.x, point.y);
            let center = Center::new(point);
            let index = self.centers.len();
            self.centers.push(center);
            center_lookup.insert(
                [point.x.round() as i32, point.y.round() as i32],
                index,
            );
        }

        for delaunay_center in self.delaunay_centers.iter_mut() {
            for pos in delaunay_center.center_locations.into_iter() {
                let center = center_lookup[&[pos.x.round() as i32, pos.y.round() as i32]].clone();
                delaunay_center.centers.push(center);
            }
        }

        let mut corner_map: Vec<Vec<usize>> = Vec::new();
        let mut corners: Vec<Corner> = Vec::new();
        let mut make_corner = |point: DVec2| {
            let mut bucket = point.x.abs() as usize;
            while bucket <= point.x.abs() as usize + 2 {
                if corner_map.get(bucket).is_none() {
                    break;
                }
                for q in &corner_map[bucket] {
                    let corner = &corners.get(*q);
                    if corner.is_none() {
                        return None;
                    }
                    let dx = -corner.unwrap().point.x;
                    let dy = point.y - corner.unwrap().point.y;
                    if dx * dx + dy * dy < 1e-6 {
                        return Some(*q);
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
            let value = corners.len();
            corners.push(q);
            corner_map[bucket].push(value);
            Some(value)
        };

        let add_to_corner_list = |v: &mut Vec<usize>, x: &Option<usize>| {
            if x.is_some() && !v.contains(x.as_ref().unwrap()) {
                v.push(x.unwrap());
            }
        };
        let add_to_center_list = |v: &mut Vec<usize>, x: &Option<usize>| {
            if x.is_some() && !v.contains(x.as_ref().unwrap()) {
                v.push(x.unwrap());
            }
        };

        let mut new_edges: Vec<Edge> = Vec::new();
        for (delaunay_edge, voronoi_edge) in edges {
            let edge = Edge::new();
            let index = new_edges.len();
            new_edges.push(edge);
            let edge = &mut new_edges[index];
            edge.midpoint = voronoi_edge.0.lerp(voronoi_edge.1, 0.5);

            // Edges point to corners. Edges point to centers.
            edge.v0 = make_corner(voronoi_edge.0).clone();
            edge.v1 = make_corner(voronoi_edge.1).clone();
            edge.d0 = center_lookup.get(&[delaunay_edge.0.x.round() as i32, delaunay_edge.0.y.round() as i32]).map(|u| *u);
            edge.d1 = center_lookup.get(&[delaunay_edge.1.x.round() as i32, delaunay_edge.1.y.round() as i32]).map(|u| *u);

            // Centers point to edges. Corners point to edges.
            if let Some(d0) = &mut edge.d0(self) {
                d0.borders.push(index);
            }
            if let Some(d1) = &mut edge.d1(self) {
                d1.borders.push(index);
            }
            if let Some(v0) = &mut edge.v0(self) {
                v0.protrudes.push(index);
            }
            if let Some(v1) = &mut edge.v1(self) {
                v1.protrudes.push(index);
            }

            // Centers point to centers.
            if let (Some(d0), Some(d1)) = (&edge.d0, &edge.d1) {
                add_to_center_list(&mut self.centers[*d0].neighbors, &Some(*d1));
                add_to_center_list(&mut self.centers[*d1].neighbors, &Some(*d0));
            }

            // Corners point to corners
            let v0 = edge.v0.clone();
            let v1 = edge.v1.clone();
            if let (Some(v0), Some(i1)) = (edge.v0_mut(self), v1) {
                add_to_corner_list(&mut v0.adjacent, &Some(i1));
            }
            if let (Some(v1), Some(i0)) = (edge.v1_mut(self), v0) {
                add_to_corner_list(&mut v1.adjacent, &Some(i0));
            }

            // Centers point to corners
            if let Some(d0) = edge.d0_mut(self) {
                add_to_corner_list(&mut d0.corners, &v0);
                add_to_corner_list(&mut d0.corners, &v1);
            }

            // Centers point to corners
            if let Some(d1) = edge.d1_mut(self) {
                add_to_corner_list(&mut d1.corners, &v0);
                add_to_corner_list(&mut d1.corners, &v1);
            }

            // Corners point to centers
            let d0 = edge.d0.clone();
            let d1 = edge.d1.clone();
            if let Some(v0) = edge.v0_mut(self) {
                add_to_center_list(&mut v0.touches, &d0);
                add_to_center_list(&mut v0.touches, &d1);
            }
            if let Some(v1) = edge.v1_mut(self) {
                add_to_center_list(&mut v1.touches, &d0);
                add_to_center_list(&mut v1.touches, &d1);
            }

        }

        for voronoi_center in self.voronoi_centers.iter_mut() {
            for pos in voronoi_center.corner_locations.iter() {
                voronoi_center.corners.push(make_corner(*pos).unwrap());
            }
        }

        self.corners = corners;
        self.edges = new_edges;
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
        //let mut queue = VecDeque::new();

        // FIXME figure out indexing, borrows
        for (i, p) in self.centers.iter_mut().enumerate() {
            // assign noise parameters based on node position
            p.noise = Self::make_noise(&self.noises, p.point);
            /*
            let mut num_water = 0.0;

            let corner_len = p.corners.len() as f64;
            for q in p.get_corners_mut(self) {
                if q.border {
                    // p.ocean = true;
                    // p.water = true;
                    queue.push_back(i);
                }
                if q.water {
                    num_water += 1.0;
                }
            }
            p.water = p.ocean || num_water >= corner_len * LAKE_TRESHOLD;
            */
        }
        /*
        while !queue.is_empty() {
            let p = queue.pop_back();
            if p.is_none() {
                break;
            }
            let p = p.unwrap();
            let p: &mut Center = &mut self.centers[p];
            for (mut r, i) in p.get_neighbors(self).zip(p.neighbors.clone()) {
                if r.water && !r.ocean {
                    r.ocean = true;
                    queue.push_back(i);
                }
            }
        }

        // Set the polygon attribute 'coast' based on its neighbors. If
        // it has at least one ocean and at least one land neighbor,
        // then self is a coastal polygon.
        for p in &mut self.centers {
            let mut num_ocean = 0;
            let mut num_land = 0;
            for r in p.get_neighbors(self) {
                if r.ocean {
                    num_ocean += 1;
                }
                if !r.water {
                    num_land += 1;
                }
            }
            p.coast = num_land > 0 && num_ocean > 0;
        }

        // Set the corner attributes based on the computed polygon
        // attributes. If all polygons connected to self corner are
        // ocean, then it's ocean; if all are land, then it's land;
        // otherwise it's coast.
        for q in &mut self.corners {
            q.noise = Self::make_noise(&self.noises, q.point);
            let mut num_ocean = 0;
            let mut num_land = 0;
            for p in q.get_touches(self) {
                if p.ocean {
                    num_ocean += 1;
                }
                if !p.water {
                    num_land += 1;
                }
            }
            q.ocean = num_ocean == q.touches.len();
            q.coast = num_land > 0 && num_ocean > 0;
            q.water = q.border || (num_land != q.touches.len() && !q.coast);
        }

        for e in &mut self.edges {
            e.noise = Self::make_noise(&self.noises, e.midpoint);
        }
        */
    }

    fn calculate_downslopes(&mut self) {
        // FIXME figure out indexing, borrows
        /*
        let mut r;
        for (i, q) in self.corners.iter_mut().enumerate() {
            r = i;
            for (s, i) in q.get_adjacent(self).zip(&q.adjacent) {
                if s.noise.elevation <= q.noise.elevation {
                    r = *i;
                }
            }
            q.downslope = Some(r);
        }
        */
    }

    /// Calculate the watershed of every land point. The watershed is
    /// the last downstream land point in the downslope graph. TODO:
    /// watersheds are currently calculated on corners, but it'd be
    /// more useful to compute them on polygon centers so that every
    /// polygon can be marked as being in one watershed.
    #[allow(clippy::assigning_clones)] // false positive, "fixing" self causes a borrow checker error
    fn calculate_watersheds(&mut self, biome_registry: &BiomeRegistry) {
        let (ocean_id, _) = biome_registry.lookup_name_to_object(OCEAN_BIOME_NAME.as_ref()).unwrap();
        let (beach_id, _) = biome_registry.lookup_name_to_object(BEACH_BIOME_NAME.as_ref()).unwrap();

        // Initially the watershed pointer points downslope one step.
        for (i, q) in self.corners.iter_mut().enumerate() {
            q.watershed = Some(i);
            if q.biome != Some(ocean_id) && q.biome != Some(beach_id) {
                q.watershed = q.downslope;
            }
        }

        // Follow the downslope pointers to the coast. Limit to 100
        // iterations although most of the time with numPoints==2000 it
        // only takes 20 iterations because most points are not far from
        // a coast.  TODO: can run faster by looking at
        // p.watershed.watershed instead of p.downslope.watershed.
        
        // FIXME figure out indexing, borrows
        /*
        for _ in 0..100 {
            let mut changed = false;
            for q in &mut self.corners {
                if &q.get_watershed(self).unwrap() == q {
                    continue;
                }
                if !q.ocean && !q.coast && !q.get_watershed(self).unwrap().coast && {
                    let downslope = q.get_downslope(self).unwrap();
                    let r = downslope.get_watershed(self).unwrap();
                    !r.ocean
                } {
                    let downslope_watershed = q.get_downslope(self).unwrap().watershed.unwrap();
                    q.watershed = Some(downslope_watershed);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        // How big is each watershed?
        for q in &mut self.corners {
            let q_shed = q.watershed.unwrap();
            let mut r = q.get_watershed(self).unwrap();
            if q_shed == r.watershed.unwrap() {
                q.watershed_size += 1;
            } else {
                r.watershed_size += 1;
            }
        }
        */
    }

    fn create_rivers(&mut self, biome_registry: &BiomeRegistry) {
        let (river_id, _) = biome_registry.lookup_name_to_object(RIVER_BIOME_NAME.as_ref()).unwrap();

        // FIXME figure out indexing, borrows
        /*
        for _ in 0..(self.size_chunks_xz / 2) {
            let mut i = self.random.gen_range(0..self.corners.len());
            let mut q = self.corners[i].clone();
            if q.ocean || q.noise.elevation < 1.0 || q.noise.elevation > 3.5 {
                continue;
            }
            while !q.coast {
                if i == q.downslope.unwrap() {
                    break;
                }
                let index = self.lookup_edge_from_corner(&q, &q.get_downslope(self).unwrap()).unwrap();
                let edge = &mut self.edges[index];
                edge.river += 1;
                q.river += 1;
                q.get_downslope(self).unwrap().river += 1;
                edge.biome = Some(river_id);

                i = q.downslope.unwrap();
                q = q.get_downslope(self).unwrap();
            }
        }
        */
    }

    fn assign_biomes(&mut self, biome_registry: &BiomeRegistry) {
        // go over all centers and assign biomes to them based on noise & other parameters.

        // FIXME figure out indexing, borrows
        /*
        for center in &mut self.centers {
            // first assign the corners' biomes
            for mut corner in center.get_corners_mut(self) {
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
            for mut edge in center.get_borders_mut(self) {
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
                    //println!("biome at point {:?} is {biome}, noise values: {:?}", center.point, center.noise);
                    center.biome = Some(*id);
                    found = true;
                    break;
                }
            }
            if !found {
                println!(
                    "found no biome for point {:?}, noise values: {:?}. Picking randomly.",
                    center.point, center.noise
                );
                let index = self.random.gen_range(0..self.biome_map.generatable_biomes.len());
                center.biome = Some(self.biome_map.generatable_biomes[index].0);
                println!(
                    "picked {}",
                    biome_registry.lookup_id_to_object(center.biome.unwrap()).unwrap()
                );
            }
        }
        */
    }

    fn blur_biomes(&mut self, biome_registry: &BiomeRegistry) {
        let (void_id, _) = biome_registry.lookup_name_to_object(VOID_BIOME_NAME.as_ref()).unwrap();

        let size = self.size_blocks_xz();
        for (x, y) in iproduct!(-size..size, -size..size) {
            self.find_biomes_at_point(DVec2::new(x as f64, y as f64), void_id);
        }
    }

    fn lookup_edge_from_corner(&self, q: &Corner, s: &Corner) -> Option<usize> {
        for (edge, i) in q.get_protrudes(self).zip(&q.protrudes) {
            if edge.v0.is_some() && edge.v0(self).unwrap() == *s {
                return Some(*i);
            }
            if edge.v1.is_some() && edge.v1(self).unwrap() == *s {
                return Some(*i);
            }
        }
        None
    }

    fn find_biomes_at_point(&mut self, point: DVec2, default: RegistryId) {
        let distance_ordering = |a: &Center, b: &Center| -> Ordering {
            let dist_a = point.distance(a.point);
            let dist_b = point.distance(b.point);
            if dist_a < dist_b {
                Ordering::Less
            } else if dist_a > dist_b {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        };
        let fade = |t: f64| -> f64 { t * t * (3.0 - 2.0 * t) };

        let mut sorted = &mut self.centers;
        sorted.sort_by(distance_ordering);

        let closest = &sorted[0];
        let closest_distance = closest.point.distance(point);

        let mut nearby = Vec::new();
        for center in sorted.iter() {
            if center.point.distance(point) <= 4.0 * BIOME_BLEND_RADIUS + closest_distance {
                nearby.push(Rc::new(RefCell::new((center.clone(), 1.0))));
            }
        }

        let mut total_weight = 0.0;
        for (first_node, second_node) in nearby.clone().into_iter().tuple_combinations() {
            let mut first_node = first_node.borrow_mut();
            let mut second_node = second_node.borrow_mut();
            let first = first_node.0.point;
            let second = second_node.0.point;

            let distance_from_midpoint =
                (point - (first + second) / 2.0).dot(second - first) / (second - first).length();
            let weight = fade((distance_from_midpoint / BIOME_BLEND_RADIUS).max(-1.0).min(1.0) * 0.5 + 0.5);

            first_node.1 *= 1.0 - weight;
            second_node.1 *= weight;

            total_weight += weight;
        }
        if total_weight < 0.1 {
            total_weight += 1.0;
        }

        let p = [point.x.round() as i32, point.y.round() as i32];
        let mut to_blend = SmallVec::<[BiomeEntry; EXPECTED_BIOME_COUNT]>::new();
        let (mut point_elevation, mut point_temperature, mut point_moisture) = (0.0, 0.0, 0.0);

        for node in nearby {
            let node = node.borrow();
            let (center, mut weight) = node.deref();
            //weight /= total_weight;

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

    /// Get the Voronoi diagram for self generator.
    pub fn voronoi(&self) -> &Voronoi {
        self.voronoi
            .as_ref()
            .expect("voronoi map should exist, but it somehow failed to generate.")
    }

    pub fn delaunay_centers(&self) -> &Vec<DelaunayCenter> {
        &self.delaunay_centers
    }

    pub fn edges(&self) -> &Vec<Edge> {
        &self.edges
    }

    /// Get the +XZ size of the world, in blocks.
    pub fn size_blocks_xz(&self) -> i32 {
        self.size_chunks_xz * CHUNK_DIM
    }

    /// Get the biome map of self generator.
    pub fn biome_map(&self) -> &BiomeMap {
        &self.biome_map
    }
}

pub fn is_inside(point: DVec2, polygon: &[DVec2]) -> bool {
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
    return true;
}

#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, Debug)]
struct NoiseValues {
    elevation: f64,
    temperature: f64,
    moisture: f64,
}

#[derive(Clone, Debug)]
pub struct Center {
    pub point: DVec2,
    noise: NoiseValues,
    biome: Option<RegistryId>,

    water: bool,
    ocean: bool,
    coast: bool,

    neighbors: Vec<usize>,
    borders: Vec<usize>,
    corners: Vec<usize>,
}

impl PartialEq for Center {
    fn eq(&self, other: &Self) -> bool {
        self.point == other.point
            && self.noise == other.noise
            && self.biome == other.biome
            && self.water == other.water
            && self.ocean == other.ocean
            && self.coast == other.coast
            && self.neighbors == other.neighbors
            && self.borders == other.borders
            && self.corners == other.corners
    }
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

    fn get_neighbors<'a>(&'a self, generator: &'a StdGenerator) -> impl Iterator<Item = &Center> + Clone + '_ {
        (0..self.neighbors.len())
            .map(move |s| &generator.centers[s])
    }

    fn get_borders<'a>(&'a self, generator: &'a StdGenerator) -> impl Iterator<Item = &Edge> + Clone + '_ {
        (0..self.borders.len())
            .map(move |s| &generator.edges[s])
    }
    fn get_borders_mut<'a>(&'a mut self, generator: &'a mut StdGenerator) -> impl Iterator<Item = Edge> + '_ {
        (0..self.borders.len())
            .map(move |s| generator.edges[s].clone())
    }
    
    fn get_corners<'a>(&'a self, generator: &'a StdGenerator) -> impl Iterator<Item = &Corner> + Clone + '_ {
        (0..self.corners.len())
            .map(move |s| &generator.corners[s])
    }
    fn get_corners_mut<'a>(&'a mut self, generator: &'a mut StdGenerator) -> impl Iterator<Item = Corner> + '_ {
        (0..self.corners.len())
            .map(move |s| generator.corners[s].clone())
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct PointEdge(DVec2, DVec2);

#[derive(Clone, Debug)]
pub struct Edge {
    pub d0: Option<usize>,
    pub d1: Option<usize>, // Delaunay edge
    pub v0: Option<usize>,
    pub v1: Option<usize>, // Voronoi edge
    pub midpoint: DVec2,   // halfway between v0,v1

    noise: NoiseValues,        // noise value at midpoint
    biome: Option<RegistryId>, // biome at midpoint

    river: i32, // 0 if no river, or volume of water in river
}

impl PartialEq for Edge {
    fn eq(&self, other: &Self) -> bool {
        self.d0 == other.d0
            && self.d1 == other.d1
            && self.v0 == other.v0
            && self.v1 == other.v1
            && self.midpoint == other.midpoint
            && self.noise == other.noise
            && self.biome == other.biome
            && self.river == other.river
    }
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

    pub fn d0<'a>(&'a self, generator: &'a StdGenerator) -> Option<Center> {
        match self.d0 {
            Some(d) => Some(generator.centers[d].clone()),
            None => None,
        }
    }
    pub fn d0_mut<'a>(&'a mut self, generator: &'a mut StdGenerator) -> Option<&'a mut Center> {
        match self.d0 {
            Some(d) => Some(&mut generator.centers[d]),
            None => None,
        }
    }
    pub fn d1<'a>(&'a self, generator: &'a StdGenerator) -> Option<Center> {
        match self.d1 {
            Some(d) => Some(generator.centers[d].clone()),
            None => None,
        }
    }
    pub fn d1_mut<'a>(&'a mut self, generator: &'a mut StdGenerator) -> Option<&'a mut Center> {
        match self.d1 {
            Some(d) => Some(&mut generator.centers[d]),
            None => None,
        }
    }

    pub fn v0<'a>(&'a self, generator: &'a StdGenerator) -> Option<Corner> {
        match self.v0 {
            Some(d) => if generator.corners.len() > d { Some(generator.corners[d].clone()) } else { None },
            None => None,
        }
    }
    pub fn v0_mut<'a>(&'a mut self, generator: &'a mut StdGenerator) -> Option<&'a mut Corner> {
        match self.v0 {
            Some(d) => if generator.corners.len() > d { Some(&mut generator.corners[d]) } else { None },
            None => None,
        }
    }
    pub fn v1<'a>(&'a self, generator: &'a StdGenerator) -> Option<Corner> {
        match self.v1 {
            Some(d) => Some(generator.corners[d].clone()),
            None => None,
        }
    }
    pub fn v1_mut<'a>(&'a mut self, generator: &'a mut StdGenerator) -> Option<&'a mut Corner> {
        match self.v1 {
            Some(d) => Some(&mut generator.corners[d]),
            None => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Corner {
    pub point: DVec2,
    noise: NoiseValues,
    border: bool,
    biome: Option<RegistryId>,

    downslope: Option<usize>,
    watershed: Option<usize>,
    watershed_size: i32,

    water: bool,
    ocean: bool,
    coast: bool,
    river: i32,

    touches: Vec<usize>,
    protrudes: Vec<usize>,
    adjacent: Vec<usize>,
}

impl PartialEq for Corner {
    fn eq(&self, other: &Self) -> bool {
        self.point == other.point
            && self.noise == other.noise
            && self.border == other.border
            && self.biome == other.biome
            && self.downslope == other.downslope
            && self.watershed == other.watershed
            && self.watershed_size == other.watershed_size
            && self.water == other.water
            && self.ocean == other.ocean
            && self.coast == other.coast
            && self.river == other.river
            && self.touches == other.touches
            && self.protrudes == other.protrudes
            && self.adjacent == other.adjacent
    }
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

    fn get_touches<'a>(&'a self, generator: &'a StdGenerator) -> impl Iterator<Item = Center> + Clone + '_ {
        (0..self.touches.len())
            .map(|s| generator.centers[s].clone())
    }

    fn get_protrudes<'a>(&'a self, generator: &'a StdGenerator) -> impl Iterator<Item = Edge> + Clone + '_ {
        (0..self.protrudes.len())
            .map(|s| generator.edges[s].clone())
    }

    fn get_adjacent<'a>(&'a self, generator: &'a StdGenerator) -> impl Iterator<Item = Corner> + Clone + '_ {
        (0..self.adjacent.len())
            .map(|s| generator.corners[s].clone())
    }

    fn get_downslope<'a>(&'a self, generator: &'a StdGenerator) -> Option<Corner> {
        match self.downslope {
            Some(d) => Some(generator.corners[d].clone()),
            None => None,
        }
    }

    fn get_watershed<'a>(&'a self, generator: &'a StdGenerator) -> Option<Corner> {
        match self.watershed {
            Some(d) => Some(generator.corners[d].clone()),
            None => None,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
struct DelaunayCenter {
    point: DVec2,
    center_locations: [DVec2; 3],
    centers: SmallVec<[usize; 3]>,
}

#[derive(Clone, PartialEq, Debug)]
struct VoronoiCenter {
    point: DVec2,
    corner_locations: Vec<DVec2>,
    corners: SmallVec<[usize; 6]>,
}
