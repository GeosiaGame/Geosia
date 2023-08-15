//! Biome blender.

use std::{f64::consts::PI, marker::PhantomData};

use ocg_schemas::{voxel::biome::{BiomeEntry, BiomeRegistry}, coordinates::CHUNK_DIM, registry::RegistryId};


    // For handling a (jittered) hex grid
const SQRT_HALF: f64 = (1.0_f64 / 2.0).sqrt();
const TRIANGLE_EDGE_LENGTH: f64 = (2.0_f64 / 3.0).sqrt();
const TRIANGLE_HEIGHT: f64 = SQRT_HALF;
const INVERSE_TRIANGLE_HEIGHT: f64 = SQRT_HALF * 2.0;
const TRIANGLE_CIRCUMRADIUS: f64 = TRIANGLE_HEIGHT * (2.0 / 3.0);
const JITTER_AMOUNT: f64 = TRIANGLE_HEIGHT;
const MAX_GRIDSCALE_DISTANCE_TO_CLOSEST_POINT: f64 = JITTER_AMOUNT + TRIANGLE_CIRCUMRADIUS;


// Primes for jitter hash.
const PRIME_X: i32 = 7691;
const PRIME_Z: i32 = 30869;
 
    // Jitter in JITTER_VECTOR_COUNT_MULTIPLIER*12 directions, symmetric about the hex grid.
    // cos(t)=sin(t+const) where const=(1/4)*2pi, and N*12 is a multiple of 4, so we can overlap arrays.
    // Repeat the first in every set of three due to how the pseudo-modulo indexer works.
    // I started out with the idea of letting JITTER_VECTOR_COUNT_MULTIPLIER_POWER be configurable,
    // but it may need bit more work to ensure there are enough bits in the selector.
const JITTER_VECTOR_COUNT_MULTIPLIER_POWER: i32 = 1;
const JITTER_VECTOR_COUNT_MULTIPLIER: i32 = 1 << JITTER_VECTOR_COUNT_MULTIPLIER_POWER;
const N_VECTORS: i32 = JITTER_VECTOR_COUNT_MULTIPLIER * 12;
const N_VECTORS_WITH_REPETITION: i32 = N_VECTORS * 4 / 3;
const VECTOR_INDEX_MASK: usize = N_VECTORS_WITH_REPETITION as usize - 1;
const JITTER_SINCOS_OFFSET: i32 = JITTER_VECTOR_COUNT_MULTIPLIER * 4;
const JITTER_SINCOS: Vec<f64> = {
        let sinCosArraySize = N_VECTORS_WITH_REPETITION * 5 / 4;
        let sinCosOffsetFactor = 1.0 / JITTER_VECTOR_COUNT_MULTIPLIER as f64;
        let mut jitter_sincos = Vec::with_capacity(sinCosArraySize as usize);
        let mut j = 0;
        for i in 0..N_VECTORS {
            jitter_sincos[j] = ((i as f64 + sinCosOffsetFactor) * ((2.0 * PI) / N_VECTORS as f64)).sin() * JITTER_AMOUNT;
            j+= 1;

            // Every time you start a new set, repeat the first entry.
            // This is because the pseudo-modulo formula,
            // which aims for an even selection over 24 values,
            // reallocates the distribution over every four entries
            // from 25%,25%,25%,25% to a,b,33%,33%, where a+b=33%.
            // The particular one used here does 0%,33%,33%,33%.
            if ((j & 3) == 1) {
                jitter_sincos[j] = jitter_sincos[j - 1];
                j += 1;
            }
        }
        for j in N_VECTORS_WITH_REPETITION..sinCosArraySize {
            jitter_sincos[j as usize] = jitter_sincos[j as usize - N_VECTORS_WITH_REPETITION as usize];
        }
        jitter_sincos
    };


struct ScatteredBiomeBlender {
    chunk_column_count: i32,
    blend_radius_bound_array_center: i32,
    chunk_width_minus_one: i32,
    blend_radius: f64,
    blend_radius_sq: f64,
    blend_radius_bound: Vec<f64>,
    chunk_point_gatherer: ChunkPointGatherer<BiomeEntry>,
}

impl ScatteredBiomeBlender {
    pub fn new(sampling_frequency: f64, blend_radius_padding: f64) -> Self {
        let chunk_width_minus_one = CHUNK_DIM - 1;
        let chunk_column_count = CHUNK_DIM * CHUNK_DIM;
        let blend_radius = blend_radius_padding + ScatteredBiomeBlender::get_internal_min_blend_radius_for_frequency(sampling_frequency);
        let blend_radius_sq = blend_radius * blend_radius;
        let gatherer = ChunkPointGatherer::<BiomeEntry>::new(sampling_frequency, blend_radius);
        
        let blend_radius_bound_array_center = blend_radius.ceil() as i32 - 1;
        let mut blend_radius_bound = Vec::with_capacity(blend_radius_bound_array_center as usize * 2 + 1);
        for i in 0..blend_radius_bound.len() {
            let dx = i as i32 - blend_radius_bound_array_center;
            let max_dx_before_truncate = dx as f64 + 1.0;
            blend_radius_bound[i] = (blend_radius_sq - max_dx_before_truncate).sqrt();
        }
        
        Self {
            chunk_column_count: chunk_column_count,
            blend_radius_bound_array_center: blend_radius_bound_array_center,
            chunk_width_minus_one: chunk_width_minus_one,
            blend_radius: blend_radius,
            blend_radius_sq: blend_radius_sq,
            blend_radius_bound: blend_radius_bound,
            chunk_point_gatherer: gatherer,
        }
    }
    
    pub fn get_blend_for_chunk(&mut self, seed: u64, chunk_base_world_x: f64, chunk_base_world_z: f64, registry: &BiomeRegistry, callback: impl Fn(f64, f64) -> RegistryId) -> BiomeEntry {
        
        // Get the list of data points in range.
        let mut points = self.chunk_point_gatherer.getPoints(seed, chunk_base_world_x, chunk_base_world_z);
        
        // Evaluate and aggregate all biomes to be blended in this chunk.
        let mut linked_biome_map_start_entry: Option<BiomeEntry> = None;
        for point in points.iter_mut() {
            
            // Get the biome for this data point from the callback.
            let biome = callback(point.x, point.z);
            
            // Find or create the chunk biome blend weight layer entry for this biome.
            let mut entry = linked_biome_map_start_entry.clone();
            while entry.is_some() {
                if entry.unwrap().id == biome {
                    break;
                }
                entry = *entry.unwrap().next;
            }
            if None == entry {
                let c_entry = Some(BiomeEntry::new_next(biome, linked_biome_map_start_entry));
                entry = c_entry;
                linked_biome_map_start_entry = c_entry;
            }
            
            point.tag = entry;
        }
        
        // If there is only one biome in range here, we can skip the actual blending step.
        if (linked_biome_map_start_entry.is_some() && linked_biome_map_start_entry.unwrap().next.is_none()) {
            return linked_biome_map_start_entry.unwrap();
        }
        
        let mut entry = linked_biome_map_start_entry;
        while entry.is_some() {
            entry.unwrap().weights = Some(vec![self.chunk_column_count as f64]);
            entry = *entry.unwrap().next;
        }
        
        let mut z = chunk_base_world_z as f64;
        let mut x = chunk_base_world_x as f64;
        let x_start = x;
        let x_end = x_start + self.chunk_width_minus_one as f64;
        for i in 0..self.chunk_column_count {
            
            // Consider each data point to see if it's inside the radius for this column.
            let mut column_total_weight = 0.0;
            for point in points.iter_mut() {
                let dx = x - point.x;
                let dz = z - point.z;
                
                let dist_sq = dx * dx + dz * dz;
                
                // If it's inside the radius...
                if (dist_sq < self.blend_radius_sq) {
                    
                    // Relative weight = [r^2 - (x^2 + z^2)]^2
                    let mut weight = self.blend_radius_sq - dist_sq;
                    weight *= weight;
                    
                    point.tag.unwrap().weights.unwrap()[i as usize] += weight;
                    column_total_weight += weight;
                }
            }
            
            // Normalize so all weights in a column add up to 1.
            let inverse_total_weight = 1.0 / column_total_weight;
            let mut entry = linked_biome_map_start_entry;
            while entry.is_some() {
                entry.unwrap().weights.unwrap()[i as usize] *= inverse_total_weight;
                entry = *entry.unwrap().next;
            }
            
            // A double can fully represent an int, so no precision loss to worry about here.
            if (x == x_end) {
                x = x_start;
                z += 1.0;
            } else {
                x += 1.0;
            }
        }
        
        return linked_biome_map_start_entry.unwrap();
    }

    pub fn get_internal_min_blend_radius_for_frequency(sampling_frequency: f64) -> f64 {
        MAX_GRIDSCALE_DISTANCE_TO_CLOSEST_POINT / sampling_frequency
    }
}


struct ChunkPointGatherer<TTag> {
    frequency: f64,
    inverse_frequency: f64,
    points_to_search: Vec<LatticePoint>,
    phantom: PhantomData<TTag>
}

impl<TTag> ChunkPointGatherer<TTag> {
    pub fn new(frequency: f64, max_point_contribution_radius: f64) -> Self {
        let frequency = frequency;
        let inverse_frequency = 1.0 / frequency;
        
        // How far out in the jittered hex grid we need to look for points.
        // Assumes the jitter can go any angle, which should only very occasionally
        // cause us to search one more layer out than we need.
        let max_contributing_distance = max_point_contribution_radius * frequency
                + MAX_GRIDSCALE_DISTANCE_TO_CLOSEST_POINT;
        let max_contributing_distance_sq = max_contributing_distance * max_contributing_distance;
        let lattice_search_radius = max_contributing_distance * INVERSE_TRIANGLE_HEIGHT;
        
        // Start at the central point, and keep traversing bigger hexagonal layers outward.
        // Exclude almost all points which can't possibly be jittered into range.
        // The "almost" is again because we assume any jitter angle is possible,
        // when in fact we only use a small set of uniformly distributed angles.
        let mut points_to_search = Vec::new();
        points_to_search.push(LatticePoint::new(0, 0));
        for i in 0..lattice_search_radius as i32 {
            let mut xsv = i;
            let mut zsv = 0;

            while (zsv < i) {
                let point = LatticePoint::new(xsv, zsv);
                if point.xv * point.xv + point.zv * point.zv < max_contributing_distance_sq {
                    points_to_search.push(point);
                }
                zsv += 1;
            }

            while (xsv > 0) {
                let point = LatticePoint::new(xsv, zsv);
                if point.xv * point.xv + point.zv * point.zv < max_contributing_distance_sq {
                    points_to_search.push(point);
                }
                xsv -= 1;
            }

            while (xsv > -i) {
                let point = LatticePoint::new(xsv, zsv);
                if point.xv * point.xv + point.zv * point.zv < max_contributing_distance_sq {
                    points_to_search.push(point);
                }
                xsv -= 1;
                zsv -= 1;
            }

            while (zsv > -i) {
                let point = LatticePoint::new(xsv, zsv);
                if point.xv * point.xv + point.zv * point.zv < max_contributing_distance_sq {
                    points_to_search.push(point);
                }
                zsv -= 1;
            }

            while (xsv < 0) {
                let point = LatticePoint::new(xsv, zsv);
                if point.xv * point.xv + point.zv * point.zv < max_contributing_distance_sq {
                    points_to_search.push(point);
                }
                xsv += 1;
            }

            while (zsv < 0) {
                let point = LatticePoint::new(xsv, zsv);
                if point.xv * point.xv + point.zv * point.zv < max_contributing_distance_sq {
                    points_to_search.push(point);
                }
                xsv += 1;
                zsv += 1;
            }
        }

        Self {
            frequency: frequency,
            inverse_frequency: inverse_frequency,
            points_to_search: points_to_search,
            phantom: PhantomData,
        }
    }

    /// AAAAAAAAA HALPPP
    pub fn getPoints(&mut self, seed: u64, x: f64, z: f64) -> Vec<GatheredPoint<TTag>> {
        x *= self.frequency; z *= self.frequency;
        
        // Simplex 2D Skew.
        let s = (x + z) * 0.366025403784439;
        let xs = x + s;
        let zs = z + s;

        // Base vertex of compressed square.
        let mut xsb = xs as i32; 
        if xs < xsb as f64 {
            xsb -= 1;
        }
        let mut zsb = zs as i32; 
        if zs < zsb as f64 {
            xsb -= 1;
        }
        let xsi = xs - xsb as f64;
        let zsi = zs - zsb as f64;

        // Find closest vertex on triangle lattice.
        let p = 2.0 * xsi - zsi;
        let q = 2.0 * zsi - xsi;
        let r = xsi + zsi;
        if r > 1.0 {
            if p < 0.0 {
                zsb += 1;
            } else if q < 0.0 {
                xsb += 1;
            } else {
                xsb += 1; zsb += 1;
            }
        } else {
            if p > 1.0 {
                xsb += 1;
            } else if q > 1.0 {
                zsb += 1;
            }
        }

        // Pre-multiply for hash.
        let xsbp = xsb * PRIME_X;
        let zsbp = zsb * PRIME_Z;
        
        // Unskewed coordinate of the closest triangle lattice vertex.
        // Everything will be relative to this.
        let bt = (xsb + zsb) as f64 * -0.211324865405187;
        let xb = xsb as f64 + bt;
        let zb = zsb as f64 + bt;
        
        // Loop through pregenerated array of all points which could be in range, relative to the closest.
        let world_points_list = Vec::<GatheredPoint<TTag>>::with_capacity(self.points_to_search.len());
        for i in 0..self.points_to_search.len() {
            let point = self.points_to_search[i];
            
            // Prime multiplications for jitter hash
            let xsvp = xsbp + point.xsvp;
            let zsvp = zsbp + point.zsvp;
            
            // Compute the jitter hash
            let hash = xsvp ^ zsvp;
            hash = (((seed & 0xFFFFFFFF) ^ hash) * 668908897)
                    ^ (((seed >> 32) ^ hash) * 35311);
            
            // Even selection within 0-24, using pseudo-modulo technique.
            let index_base = (hash & 0x3FFFFFF) * 0x5555555;
            let index = (index_base >> 26) & VECTOR_INDEX_MASK;
            let remaining_hash = index_base & 0x3FFFFFF; // The lower bits are still good as a normal hash.

            // Jittered point, not yet unscaled for frequency
            let scaled_x = xb + point.xv + JITTER_SINCOS[index];
            let scaled_z = zb + point.zv + JITTER_SINCOS[index + JITTER_SINCOS_OFFSET];
            
            // Unscale the coordinate and add it to the list.
            // "Unfiltered" means that, even if the jitter took it out of range, we don't check for that.
            // It's up to the user to handle out-of-range points as if they weren't there.
            // This is so that a user can implement a more limiting check (e.g. confine to a chunk square),
            // without the added overhead of this less limiting check.
            // A possible alternate implementation of this could employ a callback function,
            // to avoid adding the points to the list in the first place.
            let worldpoint = GatheredPoint::<TTag>::new(scaled_x * self.inverseFrequency, scaled_z * self.inverseFrequency, remaining_hash);
            world_points_list.add(worldpoint);
        }
        
        return world_points_list;
    }
}

struct LatticePoint {
    xsvp: i32,
    zsvp: i32,
    xv: f64,
    zv: f64,
}

impl LatticePoint {
    pub fn new(xsv: i32, zsv: i32) -> Self {
        let xsvp = xsv * PRIME_X;
        let zsvp = zsv * PRIME_Z;
        let t = (xsv + zsv) * -0.211324865405187;
        let xv = xsv + t;
        let zv = zsv + t;
        Self {
            xsvp: xsvp,
            zsvp: zsvp,
            xv: xv,
            zv: zv
        }
    }
}

pub struct GatheredPoint<TTag> {
    x: f64,
    z: f64,
    hash: i32,
    tag: Option<TTag>,
}

impl<TTag> GatheredPoint<TTag> {
    pub fn new(x: f64, z: f64, hash: i32) -> Self {
        Self {
            x: x,
            z: z,
            hash: hash,
            tag: None,
        }
    }
}