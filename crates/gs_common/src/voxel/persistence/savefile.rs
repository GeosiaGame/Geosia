//! SQLite-backed persistence provider

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::thread::JoinHandle;

use capnp::message::ScratchSpaceHeapAllocator;
use gs_schemas::GsExtraData;
use gs_schemas::dependencies::rusqlite::{Connection, TransactionBehavior};
use gs_schemas::savefile::queries::{OverwriteChunkRequest, ReadChunkResult, overwrite_chunks, try_read_chunks};
use gs_schemas::savefile::{SAVEFILE_CAPNP_READER_OPTIONS, SavefileMetadata};
use gs_schemas::schemas::AlignedBytesMut;
use gs_schemas::schemas::game_types_capnp::full_chunk_data;
use gs_schemas::voxel::chunk::Chunk;
use gs_schemas::{coordinates::AbsChunkPos, mutwatcher::MutWatcher};
use smallvec::SmallVec;

use crate::prelude::*;
use crate::voxel::persistence::{ChunkPersistenceLayer, ChunkPersistenceLayerStats, ChunkProviderResult};

/// Asynchronous, SQLite-backed persistence layer.
/// Missing chunks are generated from an underlying generator provider, they are only cached in memory on explicit save requests.
/// Chunk palette storage is optimized on save.
pub struct SavefilePersistenceLayer<ExtraData: GsExtraData> {
    db_thread: Option<JoinHandle<()>>,
    shared_state: Arc<DatabaseThreadSharedState<ExtraData>>,
    load_response_queue: VecDeque<ChunkProviderResult<ExtraData>>,
    generator_provider: Arc<Mutex<dyn ChunkPersistenceLayer<ExtraData>>>,
    extra_data: PhantomData<ExtraData>,
}

#[derive(Default)]
struct DatabaseThreadSharedState<ExtraData: GsExtraData> {
    command_queue: Mutex<Vec<DatabaseThreadCommand<ExtraData>>>,
    response_queue: Mutex<Vec<DatabaseThreadResponse<ExtraData>>>,
}

struct DatabaseThreadState<ExtraData: GsExtraData> {
    metadata: SavefileMetadata,
    db_connection: Connection,
    shared_state: Arc<DatabaseThreadSharedState<ExtraData>>,
    generator_provider: Arc<Mutex<dyn ChunkPersistenceLayer<ExtraData>>>,
}

enum DatabaseThreadCommand<ExtraData: GsExtraData> {
    Shutdown,
    Load(Vec<AbsChunkPos>),
    Save(Vec<(AbsChunkPos, MutWatcher<Chunk<ExtraData>>)>),
}
enum DatabaseThreadResponse<ExtraData: GsExtraData> {
    ShutdownDone,
    Error(anyhow::Error),
    LoadDone(Box<[ChunkProviderResult<ExtraData>]>),
    SaveDone,
}

impl<ExtraData: GsExtraData> SavefilePersistenceLayer<ExtraData> {
    /// Constructs a new persistence layer that provides chunks from a savefile's database, generating any missing chunks with the given generator provider.
    pub fn new(
        savefile_metadata: SavefileMetadata,
        generator_provider: Arc<Mutex<dyn ChunkPersistenceLayer<ExtraData>>>,
    ) -> Result<Self> {
        let database = savefile_metadata.open_rw().context("Opening savefile database")?;
        let shared_state = Arc::new(DatabaseThreadSharedState::default());

        let thread_state = DatabaseThreadState {
            metadata: savefile_metadata.clone(),
            db_connection: database,
            shared_state: Arc::clone(&shared_state),
            generator_provider: Arc::clone(&generator_provider),
        };
        let db_thread = std::thread::Builder::new()
            .name(String::from("GS Savefile Thread"))
            .spawn(move || {
                savefile_persistence_thread_main(thread_state);
            })
            .context("Creating savefile thread")?;
        Ok(Self {
            db_thread: Some(db_thread),
            shared_state,
            extra_data: PhantomData,
            generator_provider,
            load_response_queue: VecDeque::with_capacity(4096),
        })
    }

    fn lock_requests_queue(&self) -> MutexGuard<Vec<DatabaseThreadCommand<ExtraData>>> {
        self.shared_state
            .command_queue
            .lock()
            .expect("Panic in database IO thread")
    }
}

impl<ExtraData: GsExtraData> Drop for SavefilePersistenceLayer<ExtraData> {
    fn drop(&mut self) {
        {
            let mut requests = self.lock_requests_queue();
            // Save remaining data but do not load anything new
            requests.retain(|req| !matches!(req, DatabaseThreadCommand::Load(..)));
            requests.push(DatabaseThreadCommand::Shutdown);
        }
        let thread = self.db_thread.take();
        if let Some(thread) = thread {
            thread.join().expect("Couldn't join on the database IO thread");
        }
    }
}

fn savefile_persistence_thread_main<ExtraData: GsExtraData>(mut state: DatabaseThreadState<ExtraData>) {
    let _span = info_span!("savefile-thread", savefile = &state.metadata.name, side = %ExtraData::SIDE).entered();
    let mut scratch_space = AlignedBytesMut::new(2 * 1024 * 1024);
    let mut scratch_space_alloc = ScratchSpaceHeapAllocator::new(scratch_space.as_bytes_mut());
    loop {
        let commands: SmallVec<[DatabaseThreadCommand<ExtraData>; 32]> = {
            let Ok(mut lock) = state.shared_state.command_queue.lock() else {
                return;
            };
            lock.drain(..).collect()
        };
        for command in commands {
            if let DatabaseThreadCommand::Shutdown = command {
                if let Ok(mut responses) = state.shared_state.response_queue.lock() {
                    responses.push(DatabaseThreadResponse::ShutdownDone);
                }
                return;
            }
            let response = savefile_persistence_thread_handle_command(command, &mut state, &mut scratch_space_alloc)
                .unwrap_or_else(|err| {
                    error!("Savefile IO error: {err}");
                    DatabaseThreadResponse::Error(err)
                });
            if let Ok(mut responses) = state.shared_state.response_queue.lock() {
                responses.push(response);
            } else {
                return;
            }
        }
    }
}

fn savefile_persistence_thread_handle_command<ExtraData: GsExtraData>(
    command: DatabaseThreadCommand<ExtraData>,
    state: &mut DatabaseThreadState<ExtraData>,
    scratch_space_alloc: &mut ScratchSpaceHeapAllocator,
) -> Result<DatabaseThreadResponse<ExtraData>> {
    match command {
        DatabaseThreadCommand::Shutdown => unreachable!(),
        DatabaseThreadCommand::Load(coordinates) => {
            let chunks = try_read_chunks(&state.db_connection, &coordinates[..])?;
            let mut output = Vec::new();
            let mut generate_requests = Vec::new();
            for read_result in chunks.into_iter() {
                match read_result {
                    ReadChunkResult::Missing(position) => {
                        generate_requests.push(position);
                    }
                    ReadChunkResult::Present { position, data } => {
                        let mut data = data.as_bytes();
                        let reader =
                            capnp::serialize::read_message_from_flat_slice(&mut data, SAVEFILE_CAPNP_READER_OPTIONS)
                                .map_err(anyhow::Error::from);
                        let reader = reader.map(capnp::message::TypedReader::<_, full_chunk_data::Owned>::new);
                        let reader = match reader {
                            Ok(reader) => reader,
                            Err(err) => {
                                output.push((position, Err(err)));
                                continue;
                            }
                        };
                        let inner_reader = reader.get().map_err(anyhow::Error::from);
                        let chunk = inner_reader.and_then(|reader| {
                            Chunk::read_full(&reader, ExtraData::ChunkData::default()).map_err(anyhow::Error::from)
                        });

                        output.push((position, chunk));
                    }
                }
            }
            if !generate_requests.is_empty() {
                state
                    .generator_provider
                    .lock()
                    .expect("Panic in game thread")
                    .request_load(&generate_requests);
            }
            Ok(DatabaseThreadResponse::LoadDone(output.into_boxed_slice()))
        }
        DatabaseThreadCommand::Save(mut chunks) => {
            let mut db_param = Vec::with_capacity(chunks.len());
            for (position, chunk) in chunks.iter_mut() {
                chunk.mutate_without_revision().blocks.optimize();
                let builder = capnp::message::Builder::new(&mut *scratch_space_alloc);
                let mut builder = capnp::message::TypedBuilder::<full_chunk_data::Owned, _>::new(builder);
                let mut inner_builder = builder.init_root();
                chunk.write_full(chunk.local_revision(), &mut inner_builder);

                let serialized = capnp::serialize::write_message_to_words(builder.borrow_inner());
                db_param.push(OverwriteChunkRequest {
                    position: *position,
                    data: serialized,
                });
            }

            let mut tx = state
                .db_connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            overwrite_chunks(&mut tx, db_param.into_iter())?;
            tx.commit()?;

            Ok(DatabaseThreadResponse::SaveDone)
        }
    }
}

impl<ExtraData: GsExtraData> ChunkPersistenceLayer<ExtraData> for SavefilePersistenceLayer<ExtraData> {
    fn request_load(&mut self, coordinates: &[AbsChunkPos]) {
        if coordinates.is_empty() {
            return;
        }
        let mut queue = self.lock_requests_queue();
        if let Some(DatabaseThreadCommand::Load(loads)) = queue.last_mut() {
            loads.extend_from_slice(coordinates);
        } else {
            queue.push(DatabaseThreadCommand::Load(coordinates.into()));
        }
    }

    fn cancel_load(&mut self, coordinates: &[AbsChunkPos]) {
        if coordinates.is_empty() {
            return;
        }
        let set: HashSet<AbsChunkPos> = HashSet::from_iter(coordinates.iter().copied());
        for cmd in self.lock_requests_queue().iter_mut() {
            let DatabaseThreadCommand::Load(loads) = cmd else {
                continue;
            };
            loads.retain(|pos| !set.contains(pos));
        }
    }

    fn request_save(&mut self, chunks: Box<[(AbsChunkPos, MutWatcher<Chunk<ExtraData>>)]>) {
        if chunks.is_empty() {
            return;
        }
        let mut queue = self.lock_requests_queue();
        if let Some(DatabaseThreadCommand::Save(saves)) = queue.last_mut() {
            saves.extend(chunks);
        } else {
            queue.push(DatabaseThreadCommand::Save(chunks.into_vec()));
        }
    }

    fn try_dequeue_responses(&mut self, max_count: usize) -> Vec<ChunkProviderResult<ExtraData>> {
        if max_count == 0 {
            return Vec::new();
        }
        {
            let mut underlying = self.generator_provider.lock().expect("Panic in database IO thread");
            let underlying_responses = underlying.try_dequeue_responses(max_count);
            drop(underlying);
            let mut save_requests = Vec::with_capacity(underlying_responses.len());
            for (position, response) in underlying_responses.iter() {
                let Ok(response) = response else { continue };
                save_requests.push((*position, response.clone()));
            }
            self.request_save(save_requests.into_boxed_slice());
            self.load_response_queue.extend(underlying_responses);
        }
        {
            let mut responses = self
                .shared_state
                .response_queue
                .lock()
                .expect("Panic in database IO thread");
            for response in responses.drain(..) {
                match response {
                    DatabaseThreadResponse::ShutdownDone => {
                        info!("Database thread shut down.");
                    }
                    DatabaseThreadResponse::Error(_err) => {
                        // no-op for now, already logged
                    }
                    DatabaseThreadResponse::LoadDone(results) => {
                        self.load_response_queue.extend(results);
                    }
                    DatabaseThreadResponse::SaveDone => {
                        // no-op
                    }
                }
            }
        }
        let dequeue_count = max_count.min(self.load_response_queue.len());
        self.load_response_queue.drain(0..dequeue_count).collect()
    }

    fn stats(&self) -> ChunkPersistenceLayerStats {
        let underlying = self
            .generator_provider
            .lock()
            .expect("Panic in database IO thread")
            .stats();
        // TODO
        ChunkPersistenceLayerStats {
            loads_queued: underlying.loads_queued,
            saves_queued: underlying.saves_queued,
            responses_queued: underlying.responses_queued,
        }
    }
}
