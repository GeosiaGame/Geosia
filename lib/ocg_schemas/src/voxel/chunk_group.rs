//! In-memory representation of a group of loaded chunks

use hashbrown::HashMap;

use crate::coordinates::AbsChunkPos;
use crate::voxel::chunk::Chunk;
use crate::voxel::neighborhood::OptionalChunkRefNeighborhood;

/// A group of loaded chunks in memory, for example a planet, or a movable contraption.
#[derive(Clone)]
pub struct ChunkGroup<ExtraChunkData, ExtraGroupData> {
    /// Chunk storage
    pub chunks: HashMap<AbsChunkPos, Chunk<ExtraChunkData>>,
    /// Extra data as needed by the user API
    pub extra_data: ExtraGroupData,
}

impl<ECD, EGD: Default> Default for ChunkGroup<ECD, EGD> {
    fn default() -> Self {
        Self::new()
    }
}

impl<ECD, EGD> ChunkGroup<ECD, EGD> {
    /// Constructs an empty chunk group.
    pub fn new() -> Self
    where
        EGD: Default,
    {
        Self {
            chunks: HashMap::default(),
            extra_data: EGD::default(),
        }
    }

    /// Provides a convenient accessor for a chunk and all its neighbors.
    pub fn get_neighborhood_around(&self, center: AbsChunkPos) -> OptionalChunkRefNeighborhood<ECD> {
        OptionalChunkRefNeighborhood::from_center(center, |coord| self.chunks.get(&coord))
    }
}
