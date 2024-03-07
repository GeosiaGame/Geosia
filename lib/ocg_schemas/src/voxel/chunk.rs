//! Representation of chunks of voxel data in the game.
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::voxel::chunk_storage::{ArrayStorage, PaletteStorage};
use crate::voxel::voxeltypes::BlockEntry;
use crate::OcgExtraData;

/// RGB block light data (in a R5G5B5 format).
#[repr(transparent)]
#[derive(Copy, Clone, Default, Eq, PartialEq, Ord, PartialOrd, Debug, Hash, Pod, Zeroable, Serialize, Deserialize)]
pub struct BlockLight(u16);

/// A 32³ grid of voxel data
#[derive(Eq, PartialEq)]
pub struct Chunk<ExtraData: OcgExtraData> {
    /// Block data
    pub blocks: PaletteStorage<BlockEntry>,
    /// Light data
    pub light_level: ArrayStorage<BlockLight>,
    /// Any extra per-chunk data needed by the API user
    pub extra_data: ExtraData::ChunkData,
}

/// Manual clone implementation, because the auto-derived one puts an unnecessary bound on ExtraData.
impl<ExtraData: OcgExtraData> Clone for Chunk<ExtraData> {
    fn clone(&self) -> Self {
        Self {
            blocks: self.blocks.clone(),
            light_level: self.light_level.clone(),
            extra_data: self.extra_data.clone(),
        }
    }
}

impl<ExtraData: OcgExtraData> Chunk<ExtraData> {
    /// Creates a new chunk filled with fill_block and the given extra data.
    pub fn new(fill_block: BlockEntry, extra_data: ExtraData::ChunkData) -> Self {
        Self {
            blocks: PaletteStorage::new(fill_block),
            light_level: ArrayStorage::default(),
            extra_data,
        }
    }
}
