//! In-memory chunk persistence layer for testing purposes.

use std::collections::VecDeque;

use gs_schemas::voxel::chunk::Chunk;
use gs_schemas::GsExtraData;
use gs_schemas::{coordinates::AbsChunkPos, mutwatcher::MutWatcher};
use hashbrown::HashMap;

use crate::voxel::persistence::{ChunkPersistenceLayer, ChunkPersistenceLayerStats, ChunkProviderResult};

/// Synchronous, in-memory persistence layer.
/// Missing chunks are generated from an underlying provider, they are only cached in memory on explicit save requests.
/// Chunk palette storage is optimized on save.
pub struct MemoryPersistenceLayer<ExtraData: GsExtraData> {
    underlying_provider: Box<dyn ChunkPersistenceLayer<ExtraData>>,
    queue: VecDeque<ChunkProviderResult<ExtraData>>,
    storage: HashMap<AbsChunkPos, MutWatcher<Chunk<ExtraData>>>,
}

impl<ExtraData: GsExtraData> MemoryPersistenceLayer<ExtraData> {
    /// Constructs a new persistence layer that provides chunks from memory, generating any missing chunks with the given underlying provider.
    pub fn new(underlying_provider: Box<dyn ChunkPersistenceLayer<ExtraData>>) -> Self {
        Self {
            underlying_provider,
            queue: VecDeque::with_capacity(32),
            storage: HashMap::new(),
        }
    }
}

impl<ExtraData: GsExtraData> ChunkPersistenceLayer<ExtraData> for MemoryPersistenceLayer<ExtraData> {
    fn request_load(&mut self, coordinates: &[AbsChunkPos]) {
        let mut underlying_requests = Vec::new();
        for pos in coordinates {
            match self.storage.get(pos) {
                Some(chunk) => {
                    self.queue.push_back((*pos, Ok(chunk.clone())));
                }
                None => {
                    underlying_requests.push(*pos);
                }
            }
        }
        if !underlying_requests.is_empty() {
            self.underlying_provider.request_load(&underlying_requests);
        }
    }

    fn cancel_load(&mut self, coordinates: &[AbsChunkPos]) {
        self.underlying_provider.cancel_load(coordinates);
    }

    fn request_save(&mut self, chunks: Box<[(AbsChunkPos, MutWatcher<Chunk<ExtraData>>)]>) {
        for (pos, mut chunk) in chunks.into_vec().into_iter() {
            chunk.mutate_without_revision().blocks.optimize();
            self.storage.insert(pos, chunk);
        }
    }

    fn try_dequeue_responses(&mut self, max_count: usize) -> Vec<ChunkProviderResult<ExtraData>> {
        let mut out = self.underlying_provider.try_dequeue_responses(max_count);
        assert!(out.len() <= max_count, "{} > {}", out.len(), max_count);
        let drain_amount = usize::min(max_count - out.len(), self.queue.len());
        out.reserve(drain_amount);
        out.extend(self.queue.drain(0..drain_amount));
        out
    }

    fn stats(&self) -> ChunkPersistenceLayerStats {
        let underlying = self.underlying_provider.stats();
        ChunkPersistenceLayerStats {
            loads_queued: underlying.loads_queued,
            saves_queued: underlying.saves_queued,
            responses_queued: self.queue.len() + underlying.responses_queued,
        }
    }
}
