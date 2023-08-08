//! Representation of chunks of voxel data in the game.
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::voxel::chunk_storage::{ArrayStorage, PaletteStorage};
use crate::voxel::voxeltypes::BlockEntry;

/// RGB block light data (in a R5G5B5 format).
#[repr(transparent)]
#[derive(Copy, Clone, Default, Eq, PartialEq, Ord, PartialOrd, Debug, Hash, Pod, Zeroable, Serialize, Deserialize)]
pub struct BlockLight(u16);

/// A 32³ grid of voxel data
#[derive(Clone, Eq, PartialEq)]
pub struct Chunk<ExtraChunkData> {
    /// Block data
    pub blocks: PaletteStorage<BlockEntry>,
    /// Light data
    pub light_level: ArrayStorage<BlockLight>,
    /// Any extra per-chunk data needed by the API user
    pub extra_data: ExtraChunkData,
}

impl<ECD> Chunk<ECD> {
    pub fn new(fill_block: BlockEntry, extra_data: ECD) -> Self {
        Self {
            blocks: PaletteStorage::new(fill_block),
            light_level: ArrayStorage::default(),
            extra_data,
        }
    }
}
