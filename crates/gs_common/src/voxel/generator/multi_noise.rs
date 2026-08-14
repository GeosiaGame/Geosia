//! Standard multi noise world generator.
//! Biomes are decided for every 2x2 block 'quart' instead of every block to save system resources.

use std::ops::{Add, AddAssign, Deref, Sub, SubAssign};
use std::sync::Arc;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::mem::MaybeUninit;
use std::rc::Rc;
use std::iter::zip;

use bevy_math::{DVec2, IVec2, Vec3Swizzles};
use gs_schemas::{
    GsExtraData,
    dependencies::itertools::{Itertools, iproduct},
    registry::RegistryId,
    voxel::{
        biome::*,
        chunk::Chunk,
        voxeltypes::{BlockEntry, BlockRegistry, EMPTY_BLOCK_NAME},
    },
};
use hashbrown::HashMap;
use noise::{NoiseFn, OpenSimplex, Value};
use serde::{Deserialize, Serialize};
use smallvec::*;
use spade::handles::{FixedVertexHandle, VertexHandle};
use spade::{DelaunayTriangulation, HasPosition, Point2, Triangulation};
use tracing::debug;

use gs_schemas::coordinates::*;
use gs_schemas::registry::RegistryNameRef;
use gs_schemas::voxel::chunk_storage::{ChunkStorage, PaletteStorage};
use gs_schemas::voxel::generation::decorator::DecoratorRegistry;
use gs_schemas::voxel::generation::{Context, VoxelGenerator};
use crate::voxel::generator::noises::*;
use crate::voxel::biomes::*;

/// Biome size in chunks
///
/// (untrue) Warning: decimal values break blending.
pub const BIOME_SIZE: f64 = 1.0;

const BIOME_BLEND_RADIUS: f64 = 32.0;
const BIOME_BLEND_RADIUS2: f64 = BIOME_BLEND_RADIUS * BIOME_BLEND_RADIUS;

const THREE_CHUNK_DIM_QUARTZ: usize = CHUNK_DIM_QUARTZ * 3;
/// offset for noise value lists so that they can contain values `-1..1` chunks around the current chunk.
const NOISE_TABLE_OFFSET: i32 = CHUNK_DIM_QUART * 2;
/// size of list 3x3 chunk area-sized list offset by [`NOISE_TABLE_OFFSET`] so that no values are negative.
const NOISE_TABLE_SIZE: usize = (CHUNK_DIM_QUART2 * 9 + NOISE_TABLE_OFFSET) as usize;

const fn table_index(x: i32, z: i32) -> usize {
    assert!(x < NOISE_TABLE_OFFSET && x >= -CHUNK_DIM_QUART);
    assert!(z < NOISE_TABLE_OFFSET && z >= -CHUNK_DIM_QUART);
    let x = (x + CHUNK_DIM_QUART) as usize;
    let z = (z + CHUNK_DIM_QUART) as usize;
    x + z * THREE_CHUNK_DIM_QUARTZ
}

/// Standard world generator implementation
pub struct MultiNoiseGenerator {
    biome_registry: Arc<BiomeRegistry>,
    block_registry: Arc<BlockRegistry>,
    decorator_registry: Arc<DecoratorRegistry>,

    seed: u64,

    noises: Noises,
    point_offset_noise: OpenSimplex,

    default_biome_id: RegistryId,
}

impl<ED: GsExtraData> VoxelGenerator<ED> for MultiNoiseGenerator {
    fn generate_chunk(&self, position: AbsChunkPos, extra_data: <ED as GsExtraData>::ChunkData) -> Chunk<ED> {
        let point = AbsBlockPos::from(position);

        let mut centers: Vec<Center> = Vec::new();
        {
            let mut center_map: HashMap<IVec2, usize> = HashMap::new();
            let mut corners: Vec<Corner> = Vec::new();
            let mut corner_map: HashMap<IVec2, usize> = HashMap::new();
            let mut edges: Vec<Edge> = Vec::new();

            // Construct a new triangulation for this zone only.
            let mut delaunay = DelaunayTriangulation::new();
            let mut points = Vec::new();
            for (x, z) in iproduct!(-2..=2, -2..=2) {
                let mut position: DVec2 = DVec2::from(point.xz()) + DVec2::new(x as f64 * CHUNK_DIMD, z as f64 * CHUNK_DIMD);
                position *= BIOME_SIZE * (GLOBAL_SCALE_MOD / CHUNK_DIMD);
                let noise = CHUNK_DIMD
                    * 0.75
                    * <OpenSimplex as NoiseNDTo2D<f64, f64, NOISE_DIMS>>::get_2d(&self.point_offset_noise, position.to_array());
                let position = IVec2::new((position.x + noise) as i32, (position.y + noise) as i32);

                let point = delaunay
                    .insert(DelaunayVertex(position))
                    .unwrap_or_else(|_| panic!("failed to insert point {position:?} into delaunay triangulation"));
                points.push(point);
            }
            for point in points {
                let center = self.make_center_with_edges_corners(
                    point,
                    &delaunay,
                    &mut centers,
                    &mut center_map,
                    &mut corners,
                    &mut corner_map,
                    &mut edges,
                );
                self.assign_biome(center, &mut centers);
            }
        }

        let vparams: [(SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>, (f64, f64, f64), i32); NOISE_TABLE_SIZE] = {
            // FIXME this is UB.
            let mut vparams: [MaybeUninit<(SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>, (f64, f64, f64), i32)>; NOISE_TABLE_SIZE] = unsafe { MaybeUninit::uninit().assume_init() };
            for (i, v) in vparams[..].iter_mut().enumerate() {
                let ix = (i % THREE_CHUNK_DIM_QUARTZ) as i32 - CHUNK_DIM_QUART;
                let iz = ((i / THREE_CHUNK_DIM_QUARTZ) % THREE_CHUNK_DIM_QUARTZ) as i32 - CHUNK_DIM_QUART;
                let (biomes, noise) = Self::find_biomes_at_point(
                    IVec2::new(ix + point.x, iz + point.z),
                    &centers,
                );

                let h = Self::elevation_noise(
                    IVec2::new(ix, iz),
                    IVec2::new(position.x, position.z),
                    &self.biome_registry,
                    &biomes,
                    &self.noises,
                );
                unsafe {
                    std::ptr::write(v.as_mut_ptr(), (biomes, noise, h));
                }
            }
            unsafe { std::mem::transmute(vparams) }
        };

        let (air_block_id, _) = self.block_registry
            .lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref()).unwrap();
        let (_void_biome_id, _) = self.biome_registry
            .lookup_name_to_object(VOID_BIOME_NAME.as_ref()).unwrap();
        let mut chunk = Chunk::new(BlockEntry::new(air_block_id, 0), extra_data);

        for (quart_x, quart_y, quart_z) in iproduct!(0..CHUNK_DIM_QUART, 0..CHUNK_DIM_QUART, 0..CHUNK_DIM_QUART) {
            let index = table_index(quart_x, quart_z);
            let (ref blended_biomes, _, height) = vparams[index];

            let mut biomes: SmallVec<[(&BiomeDefinition, f64); EXPECTED_BIOME_COUNT]> = SmallVec::new();
            for b in blended_biomes {
                let e = b.lookup(&self.biome_registry).unwrap();
                let w = b.weight * e.block_influence;
                biomes.push((e, w));
            }
            // sort by block influence, then registry id if influence is same
            biomes.sort_by(|(_, a_weight), (_, b_weight)| a_weight.partial_cmp(b_weight).unwrap_or(Ordering::Equal));

            for (ox, oy, oz) in iproduct!(0..QUART_DIM, 0..QUART_DIM, 0..QUART_DIM) {
                let b_pos = InChunkPos::try_new(quart_x * QUART_DIM + ox, quart_y * QUART_DIM + oy, quart_z * QUART_DIM + oz).unwrap();
                let g_pos = position.block_pos(b_pos);

                let shift = RelBlockPos::new(
                    <OpenSimplex as NoiseNDTo2D<i32, f64, 4>>::get_2d(&self.point_offset_noise, [g_pos.x, g_pos.z]) as i32,
                    0,
                    <OpenSimplex as NoiseNDTo2D<i32, f64, 4>>::get_2d(&self.point_offset_noise, [g_pos.x, -g_pos.z]) as i32,
                );
                let g_pos = g_pos + shift;

                for (biome, _) in biomes.iter() {
                    let ctx = Context {
                        seed: self.seed,
                        chunk: &chunk.blocks,
                        ground_y: height,
                        sea_level: 0, /* hardcoded for now... */
                    };
                    let result = (biome.rule_source)(g_pos, &ctx, &self.block_registry);
                    if let Some(result) = result {
                        chunk.blocks.put(b_pos, result);
                    }
                }
            }
        }

        // FIXME this is way too slow, make biome noise & placement be precomputed.
        for (ix, iz) in iproduct!(
            -CHUNK_DIM_QUART..NOISE_TABLE_OFFSET,
            -CHUNK_DIM_QUART..NOISE_TABLE_OFFSET
        ) {
            let index = table_index(ix, iz);
            let (ref blend, (elevation, temperature, moisture), height) = vparams[index];

            for (ox, iy, oz) in iproduct!(0..QUART_DIM, -CHUNK_DIM..(CHUNK_DIM * 2), 0..QUART_DIM) {
                let b_pos = RelBlockPos::new(ix * QUART_DIM + ox, iy, iz * QUART_DIM + oz);
                let g_pos = AbsBlockPos::from(position) + b_pos;

                let shift = RelBlockPos::new(
                    <OpenSimplex as NoiseNDTo2D<i32, f64, 4>>::get_2d(&self.point_offset_noise, [-g_pos.x, g_pos.z]) as i32,
                    0,
                    <OpenSimplex as NoiseNDTo2D<i32, f64, 4>>::get_2d(&self.point_offset_noise, [g_pos.x, -g_pos.z]) as i32,
                );
                let b_pos = b_pos + shift;

                Self::place_decorators(
                    &mut chunk.blocks,
                    blend,
                    position,
                    b_pos,
                    &self.decorator_registry,
                    &self.block_registry,
                    &self.biome_registry,
                    height,
                    elevation,
                    temperature,
                    moisture,
                    &self.noises.weird_noise,
                );
            }
        }

        chunk
    }
}

impl MultiNoiseGenerator {
    /// create a new [`MultiNoiseGenerator`].
    pub fn new(
        seed: u64,
        biome_registry: Arc<BiomeRegistry>,
        block_registry: Arc<BlockRegistry>,
        decorator_registry: Arc<DecoratorRegistry>,
        default_biome: RegistryNameRef,
    ) -> Self {
        let seed_int = seed as u32;

        Self {
            default_biome_id: biome_registry.lookup_name_to_object(default_biome)
                .expect(&format!("Default biome {default_biome} is invalid."))
                .0,

            biome_registry,
            block_registry,
            decorator_registry,

            seed,

            noises: Noises {
                base_terrain_noise: configure_noise_interpolation(Fbm::<OpenSimplex>::new(seed_int)
                    .set_octaves(&[-4.0, 1.0, 1.0, 0.0])),
                elevation_noise: configure_noise_interpolation(Fbm::<OpenSimplex>::new(seed_int.wrapping_pow(1347))
                    .set_octaves(&[1.0, 2.0, 2.0, 1.0])),
                temperature_noise: configure_noise_interpolation(Fbm::<OpenSimplex>::new(seed_int.wrapping_pow(2349))
                    .set_octaves(&[1.0, 2.0, 2.0, 1.0])),
                moisture_noise: configure_noise_interpolation(Fbm::<OpenSimplex>::new(seed_int.wrapping_shl(3243))
                    .set_octaves(&[1.0, 2.0, 2.0, 1.0])),
                weird_noise: configure_noise_interpolation(Fbm::<Value>::new(seed_int.wrapping_shr(9357))
                    .set_octaves(&[4.0, 2.0, 0.0, 4.0, -25.0])),
            },
            point_offset_noise: OpenSimplex::new(seed_int.wrapping_mul(5463)),
        }
    }

    fn place_decorators(
        chunk: &mut PaletteStorage<BlockEntry>,
        biomes: &SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>,

        chunk_pos: AbsChunkPos,
        in_chunk_pos: RelBlockPos,

        decorator_registry: &DecoratorRegistry,
        block_registry: &BlockRegistry,
        biome_registry: &BiomeRegistry,

        height: i32,
        elevation: f64, temperature: f64, moisture: f64,
        weird_noise: &Box<dyn NoiseFn<i32, 4> + Send + Sync>,
    ) {
        for (_, _, decorator) in decorator_registry.iter() {
            if !biomes.iter().any(|b| decorator.biomes.contains_value(b.lookup(biome_registry).unwrap(), biome_registry)) {
                continue;
            }
            let g_pos = in_chunk_pos + AbsBlockPos::from(chunk_pos);
            if (decorator.placement_check)(decorator, weird_noise, g_pos, height, elevation, temperature, moisture) {
                (decorator.placer)(decorator, chunk, weird_noise, in_chunk_pos, chunk_pos, block_registry);
            }
        }
    }

    fn elevation_noise(
        in_chunk_quart_pos: IVec2,
        chunk_pos: IVec2,
        biome_registry: &BiomeRegistry,
        blend: &SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>,
        noises: &Noises,
    ) -> i32 {
        let nf = |p: IVec2, b: &BiomeDefinition| ((b.surface_noise)(p, &noises.base_terrain_noise) + 1.0) / 2.0;
        let pos = IVec2::new(
            ((in_chunk_quart_pos.x * QUART_DIM + (chunk_pos.x * CHUNK_DIM)) as f64 / GLOBAL_SCALE_MOD) as i32,
            ((in_chunk_quart_pos.y * QUART_DIM + (chunk_pos.y * CHUNK_DIM)) as f64 / GLOBAL_SCALE_MOD) as i32,
        );

        let mut heights = 0.0;
        let mut weights = 0.0;
        for entry in blend {
            let biome = entry.lookup(biome_registry).unwrap();
            let noise = nf(pos, biome);
            let strength = entry.weight * biome.blend_influence;
            heights += noise * strength;
            weights += strength;
        }
        (heights / weights) as i32
    }

    fn make_center(point: IVec2, centers: &mut Vec<Center>, center_map: &mut HashMap<IVec2, usize>, noises: &Noises) -> usize {
        *center_map.entry(point).or_insert_with(|| {
            let mut center = Center::new(point);
            let index = centers.len();
            center.noise = Self::make_noise(noises, center.point);
            centers.push(center);
            index
        })
    }

    fn make_corner(point: IVec2, corners: &mut Vec<Corner>, corner_map: &mut HashMap<IVec2, usize>) -> usize {
        *corner_map.entry(point).or_insert_with(|| {
            let index = corners.len();
            corners.push(Corner::new(point));
            index
        })
    }

    fn bind_centers_and_corners_for_edge(edge: &Edge, edge_index: usize, centers: &mut [Center], corners: &mut [Corner]) {
        fn add_if_empty(v: &mut Vec<usize>, x: usize) {
            if !v.contains(&x) {
                v.push(x);
            }
        }
        fn fill_triangulation_vertex_fields(
            edge_index: usize,
            // adjacent edge indices
            borders_protrudes: &mut Vec<usize>,
            // Adjacent center indices
            neighbors_adjacent: &mut Vec<usize>,
            // adjacent corner indices
            corners_touches: &mut Vec<usize>,
            // Index of the vertex opposite to this one
            opposite: usize,
            // Index of Voronoi vertex 0 (if this is a delaunay vertex) or Delaunay vertex 0 if this is a Voronoi vertex
            v0_d0: usize,
            // Same as above, but for vertex 1
            v1_d1: usize,
        ) {
            // Centers point to Delaunay edges
            // Corners point to Voronoi edges
            borders_protrudes.push(edge_index);
            // Centers point to centers (Delaunay edges)
            // Corners point to corners (Voronoi edges)
            add_if_empty(neighbors_adjacent, opposite);
            // Centers point to corners (Voronoi edges)
            // Corners point to centers (Delaunay edges)
            add_if_empty(corners_touches, v0_d0);
            add_if_empty(corners_touches, v1_d1);
        }

        // Centers point to Delaunay edges
        let d0 = &mut centers[edge.d0];
        fill_triangulation_vertex_fields(
            edge_index,
            &mut d0.borders, &mut d0.neighbors, &mut d0.corners,
            edge.d1, edge.v0, edge.v1
        );

        let d1 = &mut centers[edge.d1];
        fill_triangulation_vertex_fields(
            edge_index,
            &mut d1.borders, &mut d1.neighbors, &mut d1.corners,
            edge.d0, edge.v0, edge.v1
        );

        // Corners point to Voronoi edges
        let v0 = &mut corners[edge.v0];
        fill_triangulation_vertex_fields(
            edge_index,
            &mut v0.protrudes, &mut v0.adjacent, &mut v0.touches,
            edge.v1, edge.d0, edge.d1
        );
        // Corners point to Voronoi edges
        let v1 = &mut corners[edge.v1];
        fill_triangulation_vertex_fields(
            edge_index,
            &mut v1.protrudes, &mut v1.adjacent, &mut v1.touches,
            edge.v0, edge.d0, edge.d1
        );
    }

    fn make_center_with_edges_corners(
        &self,
        handle: FixedVertexHandle,
        delaunay: &DelaunayTriangulation<DelaunayVertex>,
        centers: &mut Vec<Center>,
        center_map: &mut HashMap<IVec2, usize>,
        corners: &mut Vec<Corner>,
        corner_map: &mut HashMap<IVec2, usize>,
        edges: &mut Vec<Edge>,
    ) -> usize {

        let point = delaunay.vertex(handle);
        let map_edges = Self::make_edges(&point);

        for (PointEdge(delaunay_start, delaunay_end), PointEdge(voronoi_start, voronoi_end)) in map_edges {

            // Delaunay edges point to centers
            let d0 = Self::make_center(delaunay_start, centers, center_map, &self.noises);
            let d1 = Self::make_center(delaunay_end, centers, center_map, &self.noises);
            // Voronoi edges point to corners
            let v0 = Self::make_corner(voronoi_start, corners, corner_map);
            let v1 = Self::make_corner(voronoi_end, corners, corner_map);

            let edge = Edge { d0, d1, v0, v1 };

            let index = edges.len();
            Self::bind_centers_and_corners_for_edge(&edge, index, centers, corners);
            edges.push(edge);
        }

        let point: IVec2 = spade_point_to_vector(point.position());
        Self::make_center(point, centers, center_map, &self.noises)
    }

    /// returns: \[(delaunay edges, voronoi edges)\]
    fn make_edges(vertex: &VertexHandle<DelaunayVertex>) -> Vec<(PointEdge, PointEdge)> {
        let mut list_of_delaunay_edges = Vec::new();
        // iterate in clockwise order
        for edge in vertex.out_edges().rev() {
            let v1 = **edge.from().data();
            let v2 = **edge.to().data();
            list_of_delaunay_edges.push(PointEdge(v1, v2));
        }

        let mut list_of_voronoi_edges = Vec::new();
        // iterate in clockwise order
        for edge in vertex.as_voronoi_face().adjacent_edges() {
            if let (Some(from), Some(to)) = (edge.from().position(), edge.to().position()) {
                list_of_voronoi_edges.push(Some(PointEdge(spade_point_to_vector(from), spade_point_to_vector(to))));
            } else {
                list_of_voronoi_edges.push(None);
            }
        }

        zip(list_of_delaunay_edges, list_of_voronoi_edges)
            .filter_map(|(delaunay, voronoi)| {
                if let Some(voronoi) = voronoi {
                    Some((delaunay, voronoi))
                } else {
                    None
                }
            })
            .collect_vec()
    }

    fn make_noise(noises: &Noises, point: IVec2) -> NoiseValues {
        let point = [point.x / GLOBAL_SCALE_MOD as i32, point.y / GLOBAL_SCALE_MOD as i32];
        let elevation = (*noises.elevation_noise).get_2d(point);
        let temperature = (*noises.temperature_noise).get_2d(point);
        let moisture = (*noises.moisture_noise).get_2d(point);

        NoiseValues {
            elevation,
            temperature,
            moisture,
        }
    }

    fn assign_biome(&self, center: usize, centers: &mut [Center]) {
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
        } else if center.water {
            center.biome = Some(
                self.biome_registry
                    .lookup_name_to_object(LAKE_BIOME_NAME.as_ref())
                    .unwrap()
                    .0,
            );
        } else if center.coast {
            center.biome = Some(
                self.biome_registry
                    .lookup_name_to_object(BEACH_BIOME_NAME.as_ref())
                    .unwrap()
                    .0,
            );
        } else {
            for (id, _, biome) in self.biome_registry.iter() {
                if biome.can_generate
                    && biome.elevation.contains(center.noise.elevation)
                    && biome.temperature.contains(center.noise.temperature)
                    && biome.moisture.contains(center.noise.moisture)
                {
                    center.biome = Some(id);
                    return;
                }
            }
        }

        if center.biome.is_none() {
            // could not find a biome
            debug!("found no biome for point {:?}, noise values: {:?}. Using default biome.", center.point, center.noise);
            center.biome = Some(self.default_biome_id);

            let default_biome = self.biome_registry.lookup_id_to_object(self.default_biome_id);
            debug!("picked {default_biome:?}");
        }
    }

    fn find_biomes_at_point(
        point: IVec2,
        centers: &[Center],
    ) -> (SmallVec<[BiomeEntry; EXPECTED_BIOME_COUNT]>, (f64, f64, f64)) {
        let distance_ordering = |a: &Center, b: &Center| -> Ordering {
            let dist_a = point.distance_squared(a.point);
            let dist_b = point.distance_squared(b.point);
            if dist_a < dist_b {
                Ordering::Less
            } else if dist_a > dist_b {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        };
        fn fade(t: f64) -> f64 {
            t * t * (3.0 - 2.0 * t)
        }

        let mut sorted = centers.to_vec();
        sorted.sort_by(distance_ordering);

        let closest = &sorted[0];
        let closest_distance = closest.point.distance_squared(point);

        let mut nearby = Vec::new();
        for center in sorted {
            if center.point.distance_squared(point) <= (4.0 * BIOME_BLEND_RADIUS2) as i32 + closest_distance {
                nearby.push(Rc::new(RefCell::new((center, 1.0))));
            }
        }

        for (first_node, second_node) in nearby.clone().into_iter().tuple_combinations() {
            let mut first_node = first_node.borrow_mut();
            let mut second_node = second_node.borrow_mut();

            let first = first_node.0.point;
            let second = second_node.0.point;

            let distance_from_midpoint =
                (point - (first + second) / 2).dot(second - first) / (second - first).length_squared();
            let weight = fade((distance_from_midpoint as f64 / BIOME_BLEND_RADIUS2).clamp(-1.0, 1.0) * 0.5 + 0.5);

            first_node.1 *= 1.0 - weight;
            second_node.1 *= weight;
        }

        let mut to_blend = SmallVec::<[BiomeEntry; EXPECTED_BIOME_COUNT]>::new();
        let mut point_elevation = 0.0;
        let mut point_temperature = 0.0;
        let mut point_moisture = 0.0;

        for node in nearby {
            let node = node.borrow();
            let &(ref center, weight) = node.deref();
            let Center { noise, biome, .. } = *center;

            point_elevation += noise.elevation * weight;
            point_temperature += noise.temperature * weight;
            point_moisture += noise.moisture * weight;

            if let Some(biome) = biome {
                if let Some(blend) = to_blend.iter_mut().find(|e| e.id == biome) {
                    blend.weight += weight;
                } else {
                    to_blend.push(BiomeEntry {
                        id: biome,
                        weight,
                    });
                }
            }
        }

        (to_blend, (point_elevation, point_temperature, point_moisture))
    }
}

fn configure_noise_interpolation<'a, Source>(source: Source) -> Box<dyn NoiseFn<i32, 4> + Send + Sync + 'a>
where
    Source: NoiseFn<f64, 4> + Send + Sync + 'a
{
    Box::new(Convert::<i32, f64, _, _, 4>::new(
        Interpolate::new(
            source,
            QUART_DIM as f64
        ),
        |pos: i32| pos as f64
    ))
}

fn spade_point_to_vector(point: Point2<f64>) -> IVec2 {
    IVec2::new(point.x as i32, point.y as i32)
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
    pub point: IVec2,
    noise: NoiseValues,
    biome: Option<RegistryId>,

    water: bool,
    ocean: bool,
    coast: bool,

    /// Adjacent center indices
    neighbors: Vec<usize>,
    /// adjacent edge indices
    borders: Vec<usize>,
    /// adjacent corner indices
    corners: Vec<usize>,
}

impl Center {
    fn new(point: IVec2) -> Center {
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
struct PointEdge(IVec2, IVec2);

/// Edge of a voronoi cell & delaunay triangle
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Edge {
    /// Delaunay edge start (center)
    pub d0: usize,
    /// Delaunay edge end (center)
    pub d1: usize,
    /// Voronoi edge start (corner)
    pub v0: usize,
    /// Voronoi edge end (corner)
    pub v1: usize,
}

/// Corner of a voronoi cell, center of a delaunay triangle
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Corner {
    /// Location of the corner
    pub point: IVec2,

    /// Adjacent center indices
    touches: Vec<usize>,
    /// adjacent edge indices
    protrudes: Vec<usize>,
    /// adjacent corner indices
    adjacent: Vec<usize>,
}

impl Corner {
    fn new(position: IVec2) -> Corner {
        Self {
            point: position,

            touches: Vec::new(),
            protrudes: Vec::new(),
            adjacent: Vec::new(),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
struct DelaunayVertex(IVec2);

impl DelaunayVertex {
    fn new(x: i32, y: i32) -> DelaunayVertex {
        DelaunayVertex(IVec2::new(x, y))
    }
}

impl Add<DelaunayVertex> for DelaunayVertex {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        DelaunayVertex(self.0.add(rhs.0))
    }
}
impl AddAssign<DelaunayVertex> for DelaunayVertex {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0.add_assign(rhs.0);
    }
}
impl Sub<DelaunayVertex> for DelaunayVertex {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        DelaunayVertex(self.0.sub(rhs.0))
    }
}
impl SubAssign<DelaunayVertex> for DelaunayVertex {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0.sub_assign(rhs.0);
    }
}
impl Deref for DelaunayVertex {
    type Target = IVec2;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl HasPosition for DelaunayVertex {
    type Scalar = f64;
    fn position(&self) -> Point2<Self::Scalar> {
        Point2::new(self.x as f64, self.y as f64)
    }
}
impl From<IVec2> for DelaunayVertex {
    fn from(value: IVec2) -> Self {
        DelaunayVertex(value)
    }
}
impl From<Point2<i32>> for DelaunayVertex {
    fn from(value: Point2<i32>) -> Self {
        DelaunayVertex::new(value.x, value.y)
    }
}
impl From<DelaunayVertex> for Point2<i32> {
    fn from(value: DelaunayVertex) -> Self {
        Point2::new(value.x, value.y)
    }
}
impl From<DelaunayVertex> for IVec2 {
    fn from(value: DelaunayVertex) -> Self {
        value.0
    }
}
