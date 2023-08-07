//! In-memory representation of a group of loaded chunks

use hashbrown::HashMap;

use crate::coordinates::AbsChunkPos;
use crate::voxel::chunk::Chunk;
use crate::voxel::neighborhood::OptionalChunkRefNeighborhood;

/// A group of loaded chunks in memory, for example a planet, or a movable contraption.
pub struct ChunkGroup<ExtraChunkData, ExtraGroupData> {
    /// Chunk storage
    pub chunks: HashMap<AbsChunkPos, Chunk<ExtraChunkData>>,
    /// Extra data as needed by the user API
    pub extra_data: ExtraGroupData,
}

impl<ECD, EGD> ChunkGroup<ECD, EGD> {
    /// Provides a convenient accessor for a chunk and all its neighbors.
    pub fn get_neighborhood_around(&self, center: AbsChunkPos) -> OptionalChunkRefNeighborhood<ECD> {
        OptionalChunkRefNeighborhood::from_center(center, |coord| self.chunks.get(&coord))
    }
}
