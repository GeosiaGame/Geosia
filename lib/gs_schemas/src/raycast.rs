//! Ray-world intersection query API types

use bevy_math::Vec3A;
use bytemuck::{Pod, Zeroable};

use crate::{
    coordinates::{AbsBlockPos, WorldPos},
    dependencies::bitflags::bitflags,
    direction::Direction,
    voxel::voxeltypes::BlockEntry,
};

bitflags! {
    #[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Pod, Zeroable)]
    #[repr(transparent)]
    /// Flags determining what type of objects to test for intersections in the world
    pub struct RaycastHitMask: u32 {
        /// Solid voxels
        const BLOCKS = 0x1;
        /// Fluid voxels
        const FLUIDS = 0x2;
        /// Non-voxel entities
        const ENTITIES = 0x4;
    }
}

/// A specification for a raycast query to resolve
#[derive(Clone, Debug, PartialEq)]
pub struct RaycastSpec {
    /// Position to start the cast from
    pub start: WorldPos,
    /// Direction to cast towards, normalized when the query is started and adjusted to Z- if zero
    pub direction: Vec3A,
    /// Distance at which the raycast should fail to resolve
    pub distance_limit: f32,
    /// Flags determining which type of objects to search for
    pub hit_mask: RaycastHitMask,
}

/// The default range for raycast queries, should be fairly inexpensive to compute.
pub const DEFAULT_RAYCAST_RANGE: f32 = 32.0;

impl Default for RaycastSpec {
    /// A query at `(0,0,0)` facing Z-, with the default range limit and hitting all possible target types
    fn default() -> Self {
        Self {
            start: WorldPos::ZERO,
            direction: Direction::ZMinus.as_vec(),
            distance_limit: DEFAULT_RAYCAST_RANGE,
            hit_mask: RaycastHitMask::all(),
        }
    }
}

/// Results of a raycast query
#[derive(Default, Clone, Debug, PartialEq)]
pub enum RaycastResult {
    /// Nothing was found within the distance limit
    #[default]
    NothingHit,
    /// The context provided was insufficient, e.g. there was no voxel world provided for a block raycast
    MissingContext,
    /// A block was hit first
    BlockHit(RaycastBlockResult),
}

/// A block hit raycast result
#[derive(Clone, Debug, PartialEq)]
pub struct RaycastBlockResult {
    /// Position of the hit block
    pub position: AbsBlockPos,
    /// The exact position hit on the block, relative to the block origin
    pub f32_offset: Vec3A,
    /// Type of the hit block
    pub entry: BlockEntry,
    /// The face of the block hit first
    pub face: Direction,
    /// Type of the block the ray went through just before hitting the target
    pub normal_entry: Option<BlockEntry>,
}
