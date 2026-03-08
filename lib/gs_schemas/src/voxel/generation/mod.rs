//! World generation related methods.

use crate::coordinates::AbsChunkPos;
use crate::GsExtraData;
use crate::voxel::chunk::Chunk;
use super::{chunk_storage::PaletteStorage, voxeltypes::BlockEntry};

pub mod decorator;
pub mod noises;

/// Context data for world generation.
pub struct Context<'a> {
    /// The world seed.
    pub seed: u64,
    /// The chunk. Unmodifiable through here.
    pub chunk: &'a PaletteStorage<BlockEntry>,
    /// The ground Y level in this block position.
    pub ground_y: i32,
    /// The sea level for this planet.
    pub sea_level: i32,
}

/// A chunk generator
pub trait VoxelGenerator<ExtraData: GsExtraData>: Send + Sync {
    /// Generates a single chunk at the given coordinates, with the given pre-filled extra data.
    fn generate_chunk(&self, position: AbsChunkPos, extra_data: ExtraData::ChunkData) -> Chunk<ExtraData>;
}
