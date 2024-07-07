//! Standard multi noise world generator

use std::iter::once;
use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::sync::Arc;
use std::{cell::RefCell, cmp::Ordering, mem::MaybeUninit, ops::Deref, rc::Rc};

use bevy_math::{DVec2, IVec2, IVec3, Vec3Swizzles};
use gs_schemas::coordinates::CHUNK_DIM3V;
use gs_schemas::voxel::voxeltypes::EMPTY_BLOCK_NAME;
use gs_schemas::{
    coordinates::{AbsChunkPos, InChunkPos, CHUNK_DIM, CHUNK_DIM2Z, CHUNK_DIMZ},
    dependencies::{
        itertools::{iproduct, Itertools},
        smallvec::SmallVec,
    },
    registry::RegistryId,
    voxel::{
        biome::{
            biome_map::{EXPECTED_BIOME_COUNT, GLOBAL_BIOME_SCALE, GLOBAL_SCALE_MOD},
            BiomeDefinition, BiomeEntry, BiomeRegistry, Noises, VOID_BIOME_NAME,
        },
        chunk::Chunk,
        chunk_storage::ChunkStorage,
        generation::{fbm_noise::Fbm, positional_random::PositionalRandomFactory, Context, NoiseNDTo2D},
        voxeltypes::{BlockEntry, BlockRegistry},
    },
    GsExtraData,
};
use hashbrown::HashMap;
use noise::OpenSimplex;
use rand::{distributions::Uniform, Rng, SeedableRng};
use rand_xoshiro::Xoshiro128StarStar;
use serde::{Deserialize, Serialize};
use spade::handles::FixedVertexHandle;
use spade::{DelaunayTriangulation, HasPosition, Point2, Triangulation};
use tracing::{info, warn};

use crate::voxel::biomes::{BEACH_BIOME_NAME, OCEAN_BIOME_NAME};
use crate::voxel::generator::VoxelGenerator;

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

const BIOME_BLEND_RADIUS: f64 = 4.0;

/// Standard world generator implementation
pub struct MultiNoiseGenerator {
    biome_registry: Arc<BiomeRegistry>,
    block_registry: Arc<BlockRegistry>,

    seed: u64,

    noises: Noises,
    point_offset_noise: OpenSimplex,

    generatable_biomes: Vec<(RegistryId, BiomeDefinition)>,
}

impl<ED: GsExtraData> VoxelGenerator<ED> for MultiNoiseGenerator {
    /// Generate a single chunk's blocks for the world.
    fn generate_chunk(&self, position: AbsChunkPos, extra_data: <ED as GsExtraData>::ChunkData) -> Chunk<ED> {
        let point: IVec3 = <IVec3>::from(position) * CHUNK_DIM3V;
        let offset_point = DVec2Wrapper::new((point.x + CHUNK_DIM / 2) as f64, (point.z + CHUNK_DIM / 2) as f64);

        let seed_bytes_be = self.seed.to_be_bytes();
        let seed_bytes_le = self.seed.to_le_bytes();
        let x = offset_point.x.to_le_bytes();
        let y = offset_point.y.to_be_bytes();
        let mut seed = [0_u8; 16];
        for i in 0..8 {
            seed[i] = x[i].wrapping_mul(seed_bytes_be[i]);
            seed[i + 8] = y[i].wrapping_mul(seed_bytes_le[i]);
        }
        let mut rand = Xoshiro128StarStar::from_seed(seed);

        let mut centers: Vec<Center> = Vec::new();
        let mut center_lookup: HashMap<[i32; 2], usize> = HashMap::new();
        let mut corners: Vec<Corner> = Vec::new();
        let mut corner_map: HashMap<[i32; 2], usize> = HashMap::new();
        let mut edges: Vec<Edge> = Vec::new();

        // Construct a new triangulation for this zone only.
        let mut delaunay = DelaunayTriangulation::new();
        let mut vertex_point = None;
        let mut points = Vec::new();
        for (x, z) in iproduct!(-2..=2, -2..=2) {
            let mut position: DVec2 = (point.xz() + IVec2::new(x * CHUNK_DIM, z * CHUNK_DIM)).into();
            let noise = 0.75 * <OpenSimplex as NoiseNDTo2D<4>>::get_2d(&self.point_offset_noise, position.to_array());
            position = DVec2::new(position.x + noise, position.y + noise);
            let point = delaunay
                .insert(DVec2Wrapper(position))
                .expect(format!("failed to insert point {position:?} into delaunay triangulation").as_str());
            if x == 0 && z == 0 {
                vertex_point = Some(point);
            } else {
                points.push(point);
            }
        }
        for point in points {
            self.make_edge_center_corner(
                point,
                &delaunay,
                &mut centers,
                &mut center_lookup,
                &mut corners,
                &mut corner_map,
                &mut edges,
            );
        }

        let center = self.make_edge_center_corner(
            vertex_point.unwrap(),
            &delaunay,
            &mut centers,
            &mut center_lookup,
            &mut corners,
            &mut corner_map,
            &mut edges,
        );
        self.assign_biome(center, &mut centers, &mut rand);

        let mut blended = vec![SmallVec::new(); CHUNK_DIM2Z];

        let void_id = self
            .biome_registry
            .lookup_name_to_object(VOID_BIOME_NAME.as_ref())
            .unwrap()
            .0;
        let vparams: [i32; CHUNK_DIM2Z] = {
            let mut vparams: [MaybeUninit<i32>; CHUNK_DIM2Z] = unsafe { MaybeUninit::uninit().assume_init() };
            for (i, v) in vparams[..].iter_mut().enumerate() {
                let ix = (i % CHUNK_DIMZ) as i32;
                let iz = ((i / CHUNK_DIMZ) % CHUNK_DIMZ) as i32;
                let (biomes, _) = self.find_biomes_at_point(
                    DVec2::new((ix + point.x) as f64, (iz + point.z) as f64),
                    void_id,
                    &centers,
                );
                biomes.clone_into(&mut blended[(ix + iz * CHUNK_DIM) as usize]);

                let p = Self::elevation_noise(
                    IVec2::new(ix, iz),
                    IVec2::new(position.x, position.z),
                    &self.biome_registry,
                    &blended,
                    &self.noises,
                )
                .round() as i32;
                unsafe {
                    std::ptr::write(v.as_mut_ptr(), p);
                }
            }
            unsafe { std::mem::transmute(vparams) }
        };

        let air = self
            .block_registry
            .lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref())
            .unwrap()
            .0;
        let mut chunk = Chunk::new(BlockEntry::new(air, 0), extra_data);

        for (pos_x, pos_y, pos_z) in iproduct!(0..CHUNK_DIM, 0..CHUNK_DIM, 0..CHUNK_DIM) {
            let b_pos = InChunkPos::try_new(pos_x, pos_y, pos_z).unwrap();

            let g_pos = <IVec3>::from(b_pos) + (<IVec3>::from(position) * CHUNK_DIM);
            let height = vparams[(pos_x + pos_z * CHUNK_DIM) as usize];

            let mut biomes: SmallVec<[(&BiomeDefinition, f64); 3]> = SmallVec::new();
            for b in blended[(pos_x + pos_z * CHUNK_DIM) as usize].iter() {
                let e = b.lookup(&self.biome_registry).unwrap();
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
                    chunk: &chunk.blocks,
                    random: PositionalRandomFactory::default(),
                    ground_y: height,
                    sea_level: 0, /* hardcoded for now... */
                };
                let result = (biome.rule_source)(&g_pos, &ctx, &self.block_registry);
                if let Some(result) = result {
                    chunk.blocks.put(b_pos, result);
                }
            }
        }
        chunk
    }
}

impl MultiNoiseGenerator {
    /// create a new StdGenerator.
    pub fn new(seed: u64, biome_registry: Arc<BiomeRegistry>, block_registry: Arc<BlockRegistry>) -> Self {
        let seed_int = seed as u32;

        Self {
            generatable_biomes: {
                let mut biomes: Vec<(RegistryId, BiomeDefinition)> = Vec::new();
                for (id, _name, def) in biome_registry.iter() {
                    if def.can_generate {
                        biomes.push((id, def.to_owned()));
                    }
                }
                biomes
            },

            biome_registry,
            block_registry,

            seed,

            noises: Noises {
                base_terrain_noise: Fbm::<OpenSimplex>::new(seed_int).set_octaves(vec![-4.0, 1.0, 1.0, 0.0]),
                elevation_noise: Fbm::<OpenSimplex>::new(seed_int.wrapping_pow(1347))
                    .set_octaves(vec![1.0, 2.0, 2.0, 1.0]),
                temperature_noise: Fbm::<OpenSimplex>::new(seed_int.wrapping_pow(2349))
                    .set_octaves(vec![1.0, 2.0, 2.0, 1.0]),
                moisture_noise: Fbm::<OpenSimplex>::new(seed_int.wrapping_pow(3243))
                    .set_octaves(vec![1.0, 2.0, 2.0, 1.0]),
            },
            point_offset_noise: OpenSimplex::new(seed_int.wrapping_mul(5463)),
        }
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

    fn elevation_noise(
        in_chunk_pos: IVec2,
        chunk_pos: IVec2,
        biome_registry: &BiomeRegistry,
        blended: &[SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>],
        noises: &Noises,
    ) -> f64 {
        let nf = |p: DVec2, b: &BiomeDefinition| ((b.surface_noise)(p, &noises.base_terrain_noise) + 1.0) / 2.0;
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

    fn add_to_list(v: &mut Vec<usize>, x: &Option<usize>) {
        if x.is_some() && !v.contains(x.as_ref().unwrap()) {
            v.push(x.unwrap());
        }
    }
    fn make_corner(&self, point: DVec2, corners: &mut Vec<Corner>, corner_map: &mut HashMap<[i32; 2], usize>) -> usize {
        let mut x = point.x.round() as i32;
        let mut y = point.y.round() as i32;
        for (x, y) in iproduct!(
            x.wrapping_sub(2)..=x.wrapping_add(2),
            y.wrapping_sub(2)..=y.wrapping_add(2)
        ) {
            if corner_map.get(&[x, y]).is_none() {
                continue;
            }
            let q = corner_map[&[x, y]];
            if point.distance(corners[q].point) < 1e-6 {
                return q;
            }
        }

        let index = {
            let index = corners.len();
            corners.push(Corner::new(point));
            index
        };
        corner_map.insert([x, y], index);
        index
    }
    fn make_centers_corners_for_edge(
        &self,
        edge: &Edge,
        index: usize,
        centers: &mut Vec<Center>,
        corners: &mut Vec<Corner>,
    ) {
        // Centers point to edges. Corners point to edges.
        if let Some(d0) = edge.d0 {
            let d0 = &mut centers[d0];
            d0.borders.push(index);
        }
        if let Some(d1) = edge.d1 {
            let d1 = &mut centers[d1];
            d1.borders.push(index);
        }
        if let Some(v0) = edge.v0 {
            let v0 = &mut corners[v0];
            v0.protrudes.push(index);
        }
        if let Some(v1) = edge.v1 {
            let v1 = &mut corners[v1];
            v1.protrudes.push(index);
        }

        // Centers point to centers.
        if let (Some(i0), Some(i1)) = (edge.d0, edge.d1) {
            let d0 = &mut centers[i0];
            Self::add_to_list(&mut d0.neighbors, &Some(i1));
            let d1 = &mut centers[i1];
            Self::add_to_list(&mut d1.neighbors, &Some(i0));
        }

        // Corners point to corners
        if let (Some(i0), Some(i1)) = (edge.v0, edge.v0) {
            let v0 = &mut corners[i0];
            Self::add_to_list(&mut v0.adjacent, &Some(i1));
            let v1 = &mut corners[i1];
            Self::add_to_list(&mut v1.adjacent, &Some(i0));
        }

        // Centers point to corners
        if let Some(d0) = edge.d0 {
            let d0 = &mut centers[d0];
            Self::add_to_list(&mut d0.corners, &edge.v0);
            Self::add_to_list(&mut d0.corners, &edge.v1);
        }

        // Centers point to corners
        if let Some(d1) = edge.d1 {
            let d1 = &mut centers[d1];
            Self::add_to_list(&mut d1.corners, &edge.v0);
            Self::add_to_list(&mut d1.corners, &edge.v1);
        }

        // Corners point to centers
        if let Some(v0) = edge.v0 {
            let v0 = &mut corners[v0];
            Self::add_to_list(&mut v0.touches, &edge.d0);
            Self::add_to_list(&mut v0.touches, &edge.d1);
        }
        if let Some(v1) = edge.v1 {
            let v1 = &mut corners[v1];
            Self::add_to_list(&mut v1.touches, &edge.d0);
            Self::add_to_list(&mut v1.touches, &edge.d1);
        }
    }

    fn make_edge_center_corner(
        &self,
        handle: FixedVertexHandle,
        delaunay: &DelaunayTriangulation<DVec2Wrapper>,
        centers: &mut Vec<Center>,
        center_lookup: &mut HashMap<[i32; 2], usize>,
        corners: &mut Vec<Corner>,
        corner_map: &mut HashMap<[i32; 2], usize>,
        edges: &mut Vec<Edge>,
    ) -> usize {
        let point = delaunay.vertex(handle);
        let point: DVec2 = *<DVec2Wrapper>::from(point.position());
        let center_lookup_pos = [point.x.round() as i32, point.y.round() as i32];
        let center = if center_lookup.contains_key(&center_lookup_pos) {
            return *center_lookup.get(&center_lookup_pos).unwrap();
        } else {
            let mut center = Center::new(point);
            let index = centers.len();
            center.noise = Self::make_noise(&self.noises, center.point);
            centers.push(center);
            center_lookup.insert(center_lookup_pos, index);
            index
        };

        let map_edges = Self::make_edges(&delaunay, handle);
        for (delaunay_edge, voronoi_edge) in map_edges {
            let mut edge = Edge::new();
            edge.midpoint = voronoi_edge.0.lerp(voronoi_edge.1, 0.5);

            // Edges point to corners. Edges point to centers.
            edge.v0 = Some(self.make_corner(voronoi_edge.0, corners, corner_map));
            edge.v1 = Some(self.make_corner(voronoi_edge.1, corners, corner_map));
            let d0_pos = [delaunay_edge.0.x.round() as i32, delaunay_edge.0.y.round() as i32];
            edge.d0 = center_lookup.get(&d0_pos).copied().or_else(|| {
                let mut center = Center::new(delaunay_edge.0);
                let index = centers.len();
                center.noise = Self::make_noise(&self.noises, center.point);
                centers.push(center);
                center_lookup.insert(d0_pos, index);
                Some(index)
            });
            let d1_pos = [delaunay_edge.1.x.round() as i32, delaunay_edge.1.y.round() as i32];
            edge.d1 = center_lookup.get(&d1_pos).copied().or_else(|| {
                let mut center = Center::new(delaunay_edge.1);
                let index = centers.len();
                center.noise = Self::make_noise(&self.noises, center.point);
                centers.push(center);
                center_lookup.insert(d1_pos, index);
                Some(index)
            });

            let index = edges.len();
            self.make_centers_corners_for_edge(&edge, index, centers, corners);
            edges.push(edge);
        }

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
            list_of_delaunay_edges.push(PointEdge(*vertex_1, *vertex_2));
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
        let elevation = Self::map_range(
            (-1.5, 1.5),
            (0.0, 5.0),
            <Fbm<OpenSimplex> as NoiseNDTo2D<4>>::get_2d(&noises.elevation_noise, point),
        );
        let temperature = Self::map_range(
            (-1.5, 1.5),
            (0.0, 5.0),
            <Fbm<OpenSimplex> as NoiseNDTo2D<4>>::get_2d(&noises.temperature_noise, point),
        );
        let moisture: f64 = Self::map_range(
            (-1.5, 1.5),
            (0.0, 5.0),
            <Fbm<OpenSimplex> as NoiseNDTo2D<4>>::get_2d(&noises.moisture_noise, point),
        );

        NoiseValues {
            elevation,
            temperature,
            moisture,
        }
    }

    fn assign_biome(&self, center: usize, centers: &mut Vec<Center>, rand: &mut Xoshiro128StarStar) {
        // go over all centers and assign biomes to them based on noise & other parameters.
        let center = &mut centers[center];
        if center.biome.is_some() {
            return;
        }
        if center.ocean {
            center.biome = Some(
                self.biome_registry
                    .lookup_name_to_object(OCEAN_BIOME_NAME.as_ref())
                    .unwrap()
                    .0,
            );
            return;
        } else if center.water {
            // TODO make lake biome(s)
            center.biome = Some(
                self.biome_registry
                    .lookup_name_to_object(OCEAN_BIOME_NAME.as_ref())
                    .unwrap()
                    .0,
            );
            return;
        } else if center.coast {
            center.biome = Some(
                self.biome_registry
                    .lookup_name_to_object(BEACH_BIOME_NAME.as_ref())
                    .unwrap()
                    .0,
            );
            return;
        }
        let mut found = false;
        for (id, biome) in &self.generatable_biomes {
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
            let index = rand.gen_range(0..self.generatable_biomes.len());
            center.biome = Some(self.generatable_biomes[index].0);
            warn!(
                "picked {}",
                self.biome_registry.lookup_id_to_object(center.biome.unwrap()).unwrap()
            );
        }
    }

    fn find_biomes_at_point(
        &self,
        point: DVec2,
        default: RegistryId,
        centers: &Vec<Center>,
    ) -> (SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>, (f64, f64, f64)) {
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

        let mut sorted = centers.clone();
        sorted.sort_by(distance_ordering);

        let closest = &sorted[0];
        let closest_distance = closest.point.distance(point);

        let mut nearby = Vec::new();
        for center in sorted {
            if center.point.distance(point) <= 4.0 * BIOME_BLEND_RADIUS + closest_distance {
                nearby.push(Rc::new(RefCell::new((center, 1.0))));
            }
        }

        for (first_node, second_node) in nearby.clone().into_iter().tuple_combinations() {
            let mut first_node = first_node.borrow_mut();
            let mut second_node = second_node.borrow_mut();
            let first = first_node.0.point;
            let second = second_node.0.point;

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

        (to_blend, (point_elevation, point_temperature, point_moisture))
    }

    /// Get the biomes at the given point from the biome map.
    pub fn get_biomes_at_point(&self, point: &[i32; 2]) -> Option<SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>> {
        //self.biome_map.lock().unwrap().biome_map.get(point).cloned()
        None
    }

    /// Get the noise values at the given point from the biome map.
    pub fn get_noises_at_point(&self, point: &[i32; 2]) -> Option<(f64, f64, f64)> {
        //self.biome_map.lock().unwrap().noise_map.get(point).cloned()
        None
    }

    fn map_range(from_range: (f64, f64), to_range: (f64, f64), s: f64) -> f64 {
        to_range.0 + (s - from_range.0) * (to_range.1 - to_range.0) / (from_range.1 - from_range.0)
    }

    /// Get all voronoi edges this map contains.
    pub fn edges(&self) -> Vec<Edge> {
        vec![] //self.edges.try_lock().unwrap().to_vec()
    }

    /// Get all voronoi centers this map contains.
    pub fn centers(&self) -> Vec<Center> {
        vec![] //self.centers.try_lock().unwrap().to_vec()
    }

    /// Get all voronoi corners this map contains.
    pub fn corners(&self) -> Vec<Corner> {
        vec![] //self.corners.try_lock().unwrap().to_vec()
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
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Center {
    /// Center of the cell
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

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
struct PointEdge(DVec2, DVec2);

/// Edge of a voronoi cell & delaunay triangle
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Edge {
    /// Delaunay edge start (center)
    pub d0: Option<usize>,
    /// Delaunay edge end (center)
    pub d1: Option<usize>,
    /// Voronoi edge start (corner)
    pub v0: Option<usize>,
    /// Voronoi edge end (corner)
    pub v1: Option<usize>,
    /// halfway between v0,v1
    pub midpoint: DVec2,
}

impl Edge {
    fn new() -> Edge {
        Self {
            d0: None,
            d1: None,
            v0: None,
            v1: None,
            midpoint: DVec2::default(),
        }
    }
}

/// Corner of a voronoi cell, center of a delaunay triangle
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Corner {
    /// Location of the corner
    pub point: DVec2,

    /// Adjacent center indices
    touches: Vec<usize>,
    /// adjacent edge indices
    protrudes: Vec<usize>,
    /// adjacent corner indices
    adjacent: Vec<usize>,
}

impl Corner {
    fn new(position: DVec2) -> Corner {
        Self {
            point: position,

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
