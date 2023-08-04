//! Representation of chunks of voxel data in the game.
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::voxel::chunk_storage::{ArrayStorage, PaletteStorage};
use crate::voxel::voxeltypes::BlockId;

/// RGB block light data (in a R5G5B5 format).
#[repr(transparent)]
#[derive(Copy, Clone, Default, Eq, PartialEq, Ord, PartialOrd, Debug, Hash, Pod, Zeroable, Serialize, Deserialize)]
pub struct BlockLight(u16);

/// A 32³ grid of voxel data
#[derive(Clone, Eq, PartialEq)]
pub struct Chunk {
    blocks: PaletteStorage<BlockId>,
    light_level: ArrayStorage<BlockLight>,
}
