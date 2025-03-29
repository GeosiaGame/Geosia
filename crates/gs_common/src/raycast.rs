//! Ray-world intersection query implementation

use bevy_math::{DVec3, prelude::*};
use gs_schemas::{
    GsExtraData,
    coordinates::{AbsBlockPos, AbsChunkPos, CHUNK_DIM, InChunkPos},
    direction::Direction,
    math::ZeroRespectingSignumToInt,
    mutwatcher::MutWatcher,
    raycast::{RaycastBlockResult, RaycastHitMask, RaycastResult, RaycastSpec},
    voxel::{
        chunk_storage::ChunkStorage,
        voxeltypes::{BlockRegistry, EMPTY_BLOCK},
    },
};

use crate::voxel::plugin::VoxelUniverse;

/// The world references in the context of which the raycast is to be performed.
pub struct RaycastContext<'world, ExtraData: GsExtraData> {
    /// Block type registry
    pub block_registry: Option<&'world BlockRegistry>,
    /// Voxel data
    pub voxel_world: Option<&'world VoxelUniverse<ExtraData>>,
}

/// Compute a single raycast in the given context
pub fn raycast<ED: GsExtraData>(context: RaycastContext<ED>, spec: &RaycastSpec) -> RaycastResult {
    let distance_limit = spec.distance_limit.min(1000.0);
    let direction = spec.direction.normalize_or(Direction::ZMinus.as_vec());
    let ddirection = direction.as_dvec3();

    // fast voxel traversal
    // https://www.gamedev.net/blogs/entry/2265248-voxel-traversal-algorithm-ray-casting/
    // http://citeseerx.ist.psu.edu/viewdoc/download?doi=10.1.1.42.3443&rep=rep1&type=pdf
    if spec.hit_mask.contains(RaycastHitMask::BLOCKS) {
        let Some(voxels) = context.voxel_world else {
            return RaycastResult::MissingContext;
        };
        let Some(registry) = context.block_registry else {
            return RaycastResult::MissingContext;
        };
        let offset_start: DVec3 = spec.start.as_dvec3(); //  + dvec3(0.5, 0.5, 0.5)
        let start_blockpos: AbsBlockPos = spec.start.as_blockpos();
        let (start_cpos, start_inpos) = start_blockpos.split_chunk_component();
        let end_bpos: AbsBlockPos =
            AbsBlockPos::from_ivec3((offset_start + ddirection * distance_limit as f64).floor().as_ivec3());
        let iters: i32 = (end_bpos - start_blockpos).abs().element_sum() + 1;

        let step: IVec3 = direction.zero_respecting_signum_int();
        let next_vox_boundary: DVec3 =
            (*start_blockpos + IVec3::select(step.cmpge(IVec3::ZERO), IVec3::ONE, IVec3::ZERO)).as_dvec3();
        let mut t_max: DVec3 = (next_vox_boundary - offset_start) / ddirection;
        let t_delta: DVec3 = step.as_dvec3() / ddirection;
        let normals: [Direction; 3] = [
            if step.x < 0 {
                Direction::XPlus
            } else {
                Direction::XMinus
            },
            if step.y < 0 {
                Direction::YPlus
            } else {
                Direction::YMinus
            },
            if step.z < 0 {
                Direction::ZPlus
            } else {
                Direction::ZMinus
            },
        ];
        let mut normal = normals[0];

        let mut inpos = start_inpos;
        let mut cpos = start_cpos;
        let mut chunk = voxels.loaded_chunks().get_chunk(cpos).map(MutWatcher::read);
        let mut normal_datum = None;
        let mut t_total = 0.0;

        for _ in 0..iters {
            // check block
            if let Some(chunk) = chunk {
                let datum = chunk.blocks.get_copy(inpos);
                let vdef = registry.lookup_id_to_object(datum.id).unwrap_or(&EMPTY_BLOCK);
                if vdef.has_selection_box {
                    let block_position = cpos.get_block_pos(inpos);
                    let intersect_pos = spec.start + direction * t_total;
                    // hit!
                    return RaycastResult::BlockHit(RaycastBlockResult {
                        position: block_position,
                        f32_offset: (intersect_pos.as_dvec3() - block_position.as_dvec3()).as_vec3a(),
                        entry: datum,
                        face: normal,
                        normal_entry: normal_datum,
                    });
                }
                normal_datum = Some(datum);
            } else {
                normal_datum = None;
            }

            // move to next block
            let min_tmax = {
                if t_max.x < t_max.y {
                    if t_max.x < t_max.z { 0 } else { 2 }
                } else if t_max.y < t_max.z {
                    1
                } else {
                    2
                }
            };

            let mut new_inpos: IVec3 = *inpos;
            let mut new_cpos: IVec3 = *cpos;

            new_inpos[min_tmax] += step[min_tmax];
            if new_inpos[min_tmax] < 0 || new_inpos[min_tmax] >= CHUNK_DIM {
                new_inpos[min_tmax] -= CHUNK_DIM * step[min_tmax];
                new_cpos[min_tmax] += step[min_tmax];
                cpos = AbsChunkPos::from_ivec3(new_cpos);
                chunk = voxels.loaded_chunks().get_chunk(cpos).map(MutWatcher::read);
            }
            inpos = InChunkPos::try_from_ivec3(new_inpos).expect("Illegal algorithm state encountered");
            t_total = t_max[min_tmax] as f32;
            t_max[min_tmax] += t_delta[min_tmax];
            normal = normals[min_tmax];
        }
    }

    RaycastResult::NothingHit
}
