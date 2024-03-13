//! In-memory representation of a group of loaded chunks

use hashbrown::HashMap;

use crate::coordinates::AbsChunkPos;
use crate::voxel::chunk::Chunk;
use crate::voxel::neighborhood::OptionalChunkRefNeighborhood;
use crate::OcgExtraData;

/// A group of loaded chunks in memory, for example a planet, or a movable contraption.
#[derive(Clone)]
pub struct ChunkGroup<ExtraData: OcgExtraData> {
    /// Chunk storage
    pub chunks: HashMap<AbsChunkPos, Chunk<ExtraData>>,
    /// Extra data as needed by the user API
    pub extra_data: ExtraData::GroupData,
}

impl<ED: OcgExtraData> Default for ChunkGroup<ED>
where
    <ED as OcgExtraData>::GroupData: Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<ED: OcgExtraData> ChunkGroup<ED> {
    /// Constructs an empty chunk group.
    pub fn new() -> Self
    where
        ED::GroupData: Default,
    {
        Self::with_data(Default::default())
    }

    /// Constructs an empty chunk group with the given per-group data.
    pub fn with_data(data: ED::GroupData) -> Self {
        Self {
            chunks: HashMap::default(),
            extra_data: data,
        }
    }

    /// Provides a convenient accessor for a chunk and all its neighbors.
    pub fn get_neighborhood_around(&self, center: AbsChunkPos) -> OptionalChunkRefNeighborhood<ED> {
        OptionalChunkRefNeighborhood::from_center(center, |coord| self.chunks.get(&coord))
    }
}
