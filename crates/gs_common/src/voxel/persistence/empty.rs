//! Empty chunk persistence layer for testing purposes.

use std::collections::VecDeque;

use gs_schemas::voxel::chunk::Chunk;
use gs_schemas::voxel::voxeltypes::BlockEntry;
use gs_schemas::GsExtraData;
use gs_schemas::{coordinates::AbsChunkPos, mutwatcher::MutWatcher};

use crate::voxel::persistence::{ChunkPersistenceLayer, ChunkPersistenceLayerStats, ChunkProviderResult};

/// Synchronous, empty persistence layer providing fresh, blank chunks every time.
pub struct EmptyPersistenceLayer<ExtraData: GsExtraData> {
    fill_block: BlockEntry,
    extra_data: ExtraData::ChunkData,
    queue: VecDeque<ChunkProviderResult<ExtraData>>,
}

impl<ExtraData: GsExtraData> EmptyPersistenceLayer<ExtraData> {
    /// Constructs a new persistence layer that provides chunks filled with `fill_block` and the given `extra_data`.
    pub fn new(fill_block: BlockEntry, extra_data: ExtraData::ChunkData) -> Self {
        Self {
            fill_block,
            extra_data,
            queue: VecDeque::with_capacity(32),
        }
    }
}

impl<ExtraData: GsExtraData> ChunkPersistenceLayer<ExtraData> for EmptyPersistenceLayer<ExtraData> {
    fn request_load(&mut self, coordinates: &[AbsChunkPos]) {
        for pos in coordinates {
            let chunk = MutWatcher::new(Chunk::new(self.fill_block, self.extra_data.clone()));
            self.queue.push_back((*pos, Ok(chunk)));
        }
    }

    fn cancel_load(&mut self, _coordinates: &[AbsChunkPos]) {
        // no-op
    }

    fn request_save(&mut self, _chunks: Box<[(AbsChunkPos, MutWatcher<Chunk<ExtraData>>)]>) {
        // no-op
    }

    fn try_dequeue_responses(&mut self, max_count: usize) -> Vec<ChunkProviderResult<ExtraData>> {
        let drain_amount = usize::min(max_count, self.queue.len());
        let mut out = Vec::with_capacity(drain_amount);
        out.extend(self.queue.drain(0..drain_amount));
        out
    }

    fn stats(&self) -> ChunkPersistenceLayerStats {
        ChunkPersistenceLayerStats {
            loads_queued: 0,
            saves_queued: 0,
            responses_queued: self.queue.len(),
        }
    }
}
