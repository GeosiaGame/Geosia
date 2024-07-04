//! In-memory representation of a group of loaded chunks

use std::collections::BTreeMap;

use crate::coordinates::AbsChunkPos;
use crate::mutwatcher::MutWatcher;
use crate::voxel::chunk::Chunk;
use crate::voxel::neighborhood::OptionalChunkRefNeighborhood;
use crate::GsExtraData;

/// A group of loaded chunks in memory, for example a planet, or a movable contraption.
#[derive(Clone)]
pub struct ChunkGroup<ExtraData: GsExtraData> {
    /// Chunk storage.
    pub chunks: BTreeMap<AbsChunkPos, MutWatcher<Chunk<ExtraData>>>,
    /// Extra data as needed by the user API
    pub extra_data: ExtraData::GroupData,
}

impl<ED: GsExtraData> Default for ChunkGroup<ED>
where
    <ED as GsExtraData>::GroupData: Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<ED: GsExtraData> ChunkGroup<ED> {
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
            chunks: BTreeMap::default(),
            extra_data: data,
        }
    }

    /// Provides a convenient accessor for a chunk and all its neighbors.
    pub fn get_neighborhood_around(&self, center: AbsChunkPos) -> OptionalChunkRefNeighborhood<ED> {
        OptionalChunkRefNeighborhood::from_center(center, |coord| self.chunks.get(&coord))
    }

    /// Accesses the chunk at the given position if loaded.
    #[inline]
    pub fn get_chunk(&self, pos: AbsChunkPos) -> Option<&MutWatcher<Chunk<ED>>> {
        self.chunks.get(&pos)
    }
}
