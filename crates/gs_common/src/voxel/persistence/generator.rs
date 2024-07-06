//! Generator-backed chunk persistence layer, intended to be used as an underlying layer for a caching layer like memory or sqlite.
//! Uses the provided task pool for running async generation tasks.

use bevy::tasks::{AsyncComputeTaskPool, Task};
use gs_schemas::dependencies::itertools::Itertools;
use gs_schemas::voxel::chunk::Chunk;
use gs_schemas::GsExtraData;
use gs_schemas::{coordinates::AbsChunkPos, mutwatcher::MutWatcher};

use crate::prelude::*;
use crate::voxel::generator::VoxelGenerator;
use crate::voxel::persistence::{ChunkPersistenceLayer, ChunkPersistenceLayerStats, ChunkProviderResult};

/// Asynchronous persistence layer wrapping a generator.
pub struct GeneratorPersistenceLayer<ExtraData: GsExtraData> {
    // TODO: remove mutex, generator must be parallel
    generator: Arc<Mutex<dyn VoxelGenerator<ExtraData>>>,
    extra_data: ExtraData::ChunkData,
    live_tasks: HashMap<AbsChunkPos, Task<ChunkProviderResult<ExtraData>>>,
    // counts unfinished tasks
    wip_task_counter: Arc<AtomicI64>,
}

struct CounterDecrOnDrop(Arc<AtomicI64>);
impl Drop for CounterDecrOnDrop {
    fn drop(&mut self) {
        self.0.fetch_sub(1, AtomicOrdering::Relaxed);
    }
}

impl<ExtraData: GsExtraData> GeneratorPersistenceLayer<ExtraData> {
    /// Constructs a new persistence layer using the given generator and cloneable `extra_data`.
    pub fn new(generator: Arc<Mutex<dyn VoxelGenerator<ExtraData>>>, extra_data: ExtraData::ChunkData) -> Self {
        Self {
            generator,
            extra_data,
            live_tasks: HashMap::with_capacity(256),
            wip_task_counter: Arc::new(AtomicI64::new(0)),
        }
    }
}

impl<ExtraData: GsExtraData> ChunkPersistenceLayer<ExtraData> for GeneratorPersistenceLayer<ExtraData> {
    fn request_load(&mut self, coordinates: &[AbsChunkPos]) {
        for &pos in coordinates {
            self.wip_task_counter.fetch_add(1, AtomicOrdering::Relaxed);
            let counter = Arc::clone(&self.wip_task_counter);
            let counter = CounterDecrOnDrop(counter);
            let gen = Arc::clone(&self.generator);
            let extra_data = self.extra_data.clone();
            let task = AsyncComputeTaskPool::get().spawn(async move {
                let _counter = counter; // decrement on drop()
                let chunk = (*gen)
                    .lock()
                    .expect("Failed to lock generator")
                    .generate_chunk(pos, extra_data);
                (pos, Ok(MutWatcher::new(chunk)))
            });
            let _ = self.live_tasks.try_insert(pos, task);
        }
    }

    fn cancel_load(&mut self, coordinates: &[AbsChunkPos]) {
        coordinates.iter().for_each(|c| drop(self.live_tasks.remove(c)));
    }

    fn request_save(&mut self, _chunks: Box<[(AbsChunkPos, MutWatcher<Chunk<ExtraData>>)]>) {
        // no-op
    }

    fn try_dequeue_responses(&mut self, max_count: usize) -> Vec<ChunkProviderResult<ExtraData>> {
        let done_task_positions = self
            .live_tasks
            .iter()
            .filter(|(_, t)| t.is_finished())
            .map(|(&c, _)| c)
            .take(max_count)
            .collect_vec();
        done_task_positions
            .into_iter()
            .map(|p| {
                let task = self.live_tasks.remove(&p).unwrap();
                bevy::tasks::block_on(task)
            })
            .collect_vec()
    }

    fn stats(&self) -> ChunkPersistenceLayerStats {
        let total_cnt = self.live_tasks.len();
        let wip_cnt = self
            .wip_task_counter
            .load(AtomicOrdering::Relaxed)
            .clamp(0, total_cnt as i64) as usize;
        ChunkPersistenceLayerStats {
            loads_queued: wip_cnt,
            saves_queued: 0,
            responses_queued: total_cnt - wip_cnt,
        }
    }
}
