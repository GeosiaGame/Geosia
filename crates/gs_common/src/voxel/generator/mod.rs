//! Standard world generator.

use gs_schemas::coordinates::AbsChunkPos;
use gs_schemas::voxel::chunk::Chunk;
use gs_schemas::GsExtraData;

pub mod flat;
pub mod multi_noise;

/// A chunk generator
pub trait VoxelGenerator<ExtraData: GsExtraData>: Send + Sync {
    /// Generates a single chunk at the given coordinates, with the given pre-filled extra data.
    fn generate_chunk(&self, position: AbsChunkPos, extra_data: ExtraData::ChunkData) -> Chunk<ExtraData>;
}
