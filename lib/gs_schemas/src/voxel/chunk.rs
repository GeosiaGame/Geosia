//! Representation of chunks of voxel data in the game.
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use thiserror::Error;

use crate::voxel::chunk_storage::palette::PaletteDeserializationError;
use crate::voxel::chunk_storage::{ArrayStorage, PaletteStorage};
use crate::voxel::voxeltypes::BlockEntry;
use crate::{GsExtraData, SmallCowVec};

/// RGB block light data (in a R5G5B5 format).
#[repr(transparent)]
#[derive(Copy, Clone, Default, Eq, PartialEq, Ord, PartialOrd, Debug, Hash, Pod, Zeroable, Serialize, Deserialize)]
pub struct BlockLight(u16);

/// A 32³ grid of voxel data
#[derive(Eq, PartialEq)]
pub struct Chunk<ExtraData: GsExtraData> {
    /// Block data
    pub blocks: PaletteStorage<BlockEntry>,
    /// Light data
    pub light_level: ArrayStorage<BlockLight>,
    /// Any extra per-chunk data needed by the API user
    pub extra_data: ExtraData::ChunkData,
}

/// Error during chunk deserialization.
#[derive(Clone, Debug, Error)]
pub enum ChunkDeserializationError {
    /// Low level data encoding error.
    #[error("Low level data encoding error {0}")]
    SchemaError(#[from] capnp::Error),
    /// Palette deserialization error.
    #[error("Palette deserialization error {0}")]
    PaletteError(#[from] PaletteDeserializationError),
    /// Illegal block ID in palette data.
    #[error("Illegal block ID in palette data")]
    IllegalBlockID,
}

/// Manual clone implementation, because the auto-derived one puts an unnecessary bound on ExtraData.
impl<ExtraData: GsExtraData> Clone for Chunk<ExtraData> {
    fn clone(&self) -> Self {
        Self {
            blocks: self.blocks.clone(),
            light_level: self.light_level.clone(),
            extra_data: self.extra_data.clone(),
        }
    }
}

impl<ExtraData: GsExtraData> Chunk<ExtraData> {
    /// Creates a new chunk filled with fill_block and the given extra data.
    pub fn new(fill_block: BlockEntry, extra_data: ExtraData::ChunkData) -> Self {
        Self {
            blocks: PaletteStorage::new(fill_block),
            light_level: ArrayStorage::default(),
            extra_data,
        }
    }

    /// Writes a full copy of the chunk to the given builder.
    pub fn write_full(&self, output: &mut crate::schemas::game_types_capnp::full_chunk_data::Builder) {
        let block_palette = self.blocks.serialized_palette();
        let block_data = self.blocks.serialized_data();
        let mut palette_builder = output
            .reborrow()
            .init_block_palette(block_palette.len().try_into().unwrap());
        for (i, entry) in block_palette.iter().enumerate() {
            palette_builder.set(i as u32, entry.as_packed());
        }
        output.set_block_data(block_data).unwrap();
    }

    /// Reads a fully serialized chunk from the given schema reader.
    pub fn read_full(
        reader: &crate::schemas::game_types_capnp::full_chunk_data::Reader,
        extra_data: ExtraData::ChunkData,
    ) -> Result<Self, ChunkDeserializationError> {
        let palette_reader = reader.get_block_palette()?;
        let data_reader = reader.get_block_data()?;
        let palette = palette_reader.as_slice();
        let palette: SmallVec<[BlockEntry; 16]> = if let Some(palette) = palette {
            let palette: Option<_> = palette.iter().copied().map(BlockEntry::from_packed).collect();
            palette
        } else {
            let palette: Option<_> = palette_reader.iter().map(BlockEntry::from_packed).collect();
            palette
        }
        .ok_or(ChunkDeserializationError::IllegalBlockID)?;
        let data = data_reader.as_slice();
        let data = if let Some(data) = data {
            SmallCowVec::Borrowed(data)
        } else {
            SmallCowVec::Owned(SmallVec::from_iter(data_reader.iter()))
        };

        let chunk = Self {
            blocks: PaletteStorage::from_serialized(palette.into(), data)?,
            light_level: ArrayStorage::default(),
            extra_data,
        };

        Ok(chunk)
    }
}
