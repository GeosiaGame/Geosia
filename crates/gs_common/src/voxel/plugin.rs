//! The Bevy plugin for voxel universe handling.

use std::collections::BTreeSet;
use std::marker::PhantomData;

use gs_schemas::coordinates::{AbsBlockPos, AbsChunkPos, AbsChunkRange, RelChunkPos};
use gs_schemas::dependencies::itertools::Itertools;
use gs_schemas::mutwatcher::{MutWatcher, RevisionNumber};
use gs_schemas::schemas::network_capnp::{PacketId, chunk_data_stream_packet};
use gs_schemas::schemas::new_packet_builder;
use gs_schemas::voxel::biome::BiomeRegistry;
use gs_schemas::voxel::chunk::Chunk;
use gs_schemas::voxel::chunk_group::ChunkGroup;
use gs_schemas::voxel::generation::decorator::DecoratorRegistry;
use gs_schemas::voxel::voxeltypes::BlockRegistry;
use gs_schemas::{GameSide, GsExtraData};
use smallvec::SmallVec;

use crate::network::server::{ConnectedPlayer, NetworkThreadServerCommand, PacketStreamKey};
use crate::network::server_packet_handler::BootstrappedGameDataTag;
use crate::network::transport::{PacketStream, PacketWrapper};
use crate::prelude::*;
use crate::voxel::persistence::ChunkPersistenceLayer;
use crate::{GameServer, GameServerResource};
use crate::{InGameSystemSet, ServerData};

/// The maximum number of stored chunk packets before applying stream backpressure.
pub const CHUNK_PACKET_QUEUE_LENGTH: usize = 20;

/// public for client crate use only
pub const CHUNK_LOAD_RADIUS: i32 = 6;

/// Initializes the settings related to the voxel universe.
#[derive(Default)]
pub struct VoxelUniversePlugin<ExtraData: GsExtraData> {
    _extra_data: PhantomData<ExtraData>,
}

impl<ExtraData: GsExtraData> Plugin for VoxelUniversePlugin<ExtraData> {
    fn build(&self, app: &mut App) {
        if ExtraData::SIDE == GameSide::Server {
            app.add_systems(
                FixedPreUpdate,
                (server_system_process_chunk_loading).in_set(InGameSystemSet),
            )
            .add_systems(
                FixedPreUpdate,
                (server_system_open_chunk_stream).in_set(InGameSystemSet),
            )
            .add_systems(
                FixedPostUpdate,
                (server_system_process_chunk_sending).in_set(InGameSystemSet),
            );
        }
    }

    fn name(&self) -> &'static str {
        "common::VoxelUniversePlugin"
    }

    fn is_unique(&self) -> bool {
        true
    }
}

impl<ExtraData: GsExtraData> VoxelUniversePlugin<ExtraData> {
    /// Constructor.
    pub fn new() -> Self {
        Self { _extra_data: default() }
    }
}

/// The extra data associated with each chunk on the server
#[derive(Default, Clone)]
pub struct ServerChunkMetadata {
    /// Map holding which revision was provided to each connected player
    player_held_revisions: HashMap<Entity, RevisionNumber>,
}

/// A tag component marking voxel universes regardless of the generic type.
#[derive(Clone, Copy, Component)]
pub struct VoxelUniverseTag;

/// The chunk-storing component of a voxel universe, it's spawned along with other components that will manage its storage.
#[derive(Component)]
pub struct VoxelUniverse<ExtraData: GsExtraData> {
    loaded_chunks: ChunkGroup<ExtraData>,
    _extra_data: PhantomData<ExtraData>,
}

/// Persistent storage for chunks, exists alongside [`VoxelUniverse`] on servers.
#[derive(Component)]
pub struct PersistentVoxelStorage<ExtraData: GsExtraData> {
    pub(crate) persistence_layer: Box<dyn ChunkPersistenceLayer<ExtraData>>,
    live_requests: BTreeSet<AbsChunkPos>,
}

/// The bevy [`Resource`] for shared voxel registry access from systems.
#[derive(Resource, Clone, Deref)]
pub struct BlockRegistryHolder(pub Arc<BlockRegistry>);

/// The bevy [`Resource`] for shared biome registry access from systems.
#[derive(Resource, Clone, Deref)]
pub struct BiomeRegistryHolder(pub Arc<BiomeRegistry>);

/// The bevy [`Resource`] for shared decorator registry access from systems.
#[derive(Resource, Clone, Deref)]
pub struct DecoratorRegistryHolder(pub Arc<DecoratorRegistry>);

/// Component for entities anchored in the voxel grid.
#[derive(Component, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Deref, DerefMut)]
pub struct VoxelPosition(pub AbsBlockPos);

impl VoxelPosition {
    /// Returns the chunk corresponding to the stored block position.
    pub fn chunk_pos(&self) -> AbsChunkPos {
        self.0.into()
    }
}

/// Component that triggers chunk loading in a given radius around itself.
#[derive(Component, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Deref, DerefMut)]
pub struct ChunkLoader {
    /// The radius of the loading area, in chunk units.
    /// If zero or less, does not load anything.
    pub radius: i32,
}

/// Builder for voxel universe initialization
pub struct VoxelUniverseBuilder<'world, ExtraData: GsExtraData> {
    _block_registry: Arc<BlockRegistry>,
    _biome_registry: Arc<BiomeRegistry>,
    _decorator_registry: Arc<DecoratorRegistry>,
    /// The bundle being spawned
    pub bundle: EntityWorldMut<'world>,
    _extra_data: PhantomData<ExtraData>,
}

impl<'world, ED: GsExtraData> VoxelUniverseBuilder<'world, ED> {
    /// Starts initializing a new voxel universe in a bevy World. Cannot be used on a World without cleaning up the previous universe first.
    pub fn new(
        world: &'world mut World,
        block_registry: Arc<BlockRegistry>,
        biome_registry: Arc<BiomeRegistry>,
        decorator_registry: Arc<DecoratorRegistry>,
    ) -> Result<Self> {
        let mut old_worlds = world.query::<&VoxelUniverseTag>();
        if old_worlds.iter(world).next().is_some() {
            bail!("Existing voxel worlds still in the bevy app");
        }

        world.insert_resource(BlockRegistryHolder(Arc::clone(&block_registry)));
        world.insert_resource(BiomeRegistryHolder(Arc::clone(&biome_registry)));
        world.insert_resource(DecoratorRegistryHolder(Arc::clone(&decorator_registry)));
        let bundle = world.spawn((VoxelUniverseTag, VoxelUniverse::<ED>::new(default())));

        Ok(Self {
            _block_registry: block_registry,
            _biome_registry: biome_registry,
            _decorator_registry: decorator_registry,
            bundle,
            _extra_data: default(),
        })
    }

    /// Adds persistent storage support to the universe.
    pub fn with_persistent_storage(mut self, persistence_layer: Box<dyn ChunkPersistenceLayer<ED>>) -> Result<Self> {
        // TODO: make the player load the chunks
        self.bundle.world_scope(|w| {
            w.spawn((VoxelPosition(AbsBlockPos::ZERO), ChunkLoader { radius: CHUNK_LOAD_RADIUS }));
        });

        self.bundle.insert(PersistentVoxelStorage::<ED> {
            persistence_layer,
            live_requests: default(),
        });
        Ok(self)
    }

    /// Finishes the setup, returns the entity ID holding the [`VoxelUniverse`] component.
    pub fn build(self) -> EntityWorldMut<'world> {
        self.bundle
    }
}

impl<ExtraData: GsExtraData> VoxelUniverse<ExtraData> {
    /// Constructor.
    pub fn new(group_data: ExtraData::GroupData) -> Self {
        Self {
            loaded_chunks: ChunkGroup::with_data(group_data),
            _extra_data: default(),
        }
    }

    /// Read-only access to the currently loaded chunks
    #[inline]
    pub fn loaded_chunks(&self) -> &ChunkGroup<ExtraData> {
        &self.loaded_chunks
    }

    /// Writable access to the currently loaded chunks
    #[inline]
    pub fn loaded_chunks_mut(&mut self) -> &mut ChunkGroup<ExtraData> {
        &mut self.loaded_chunks
    }
}

fn server_system_process_chunk_loading(
    mut voxel_q: Query<(
        &mut VoxelUniverse<ServerData>,
        &mut PersistentVoxelStorage<ServerData>,
        &VoxelUniverseTag,
    )>,
    chunk_loaders: Query<(&ChunkLoader, &VoxelPosition)>,
) {
    let Ok((mut voxels, mut persistence, _)) = voxel_q.single_mut() else {
        return;
    };
    // TODO: do not fully scan every frame, this is really simple code to get it going right now

    let persistence = &mut *persistence;
    let chunk_map = &mut voxels.loaded_chunks.chunks;
    let layer = &mut persistence.persistence_layer;
    let live_requests = &mut persistence.live_requests;

    // Dequeue all processed requests
    {
        let _span = trace_span!("Dequeue chunk load responses").entered();
        for (loaded_pos, response) in layer.try_dequeue_responses(usize::MAX) {
            live_requests.remove(&loaded_pos);
            trace!(chunk_position = %loaded_pos, is_ok = response.is_ok(), "Chunk load request resolved");
            let loaded_chunk = match response {
                Ok(c) => c,
                Err(e) => {
                    error!("Could not load chunk at position {loaded_pos}: {e}");
                    continue;
                }
            };
            // Do not overwrite if the chunk was already loaded earlier.
            chunk_map.entry(loaded_pos).or_insert(loaded_chunk);
        }
    }

    // Find new requests to make
    let to_request = {
        let _span = trace_span!("Scan for new chunk load requests").entered();
        let mut to_request: BTreeSet<AbsChunkPos> = default();

        for (loader, lpos) in chunk_loaders.iter() {
            if loader.radius <= 0 {
                continue;
            }
            let r = loader.radius;
            let center: AbsChunkPos = lpos.chunk_pos();
            let range = AbsChunkRange::from_corners(center - RelChunkPos::splat(r), center + RelChunkPos::splat(r));
            for cpos in range.iter_xzy() {
                if chunk_map.contains_key(&cpos) {
                    continue;
                }
                if live_requests.contains(&cpos) {
                    continue;
                }
                to_request.insert(cpos);
            }
        }

        to_request.into_iter().collect_vec()
    };
    {
        let _span = trace_span!("Request chunks to load", n = to_request.len()).entered();
        layer.request_load(&to_request);
        live_requests.extend(to_request);
    }
}

#[derive(Component)]
struct ConnectedPlayerAwaitingChunkStream {
    result: AsyncResult<(Arc<PacketStream>, PacketStreamKey)>,
}

#[derive(Component)]
struct ConnectedPlayerChunkStream {
    s2c_chunk_stream: Arc<PacketStream>,
    #[allow(dead_code)]
    s2c_chunk_stream_key: PacketStreamKey,
}

fn server_system_open_chunk_stream(
    engine: Res<GameServerResource>,
    mut commands: Commands,
    mut players_q: Populated<
        (
            Entity,
            &ConnectedPlayer,
            Option<&mut ConnectedPlayerAwaitingChunkStream>,
        ),
        Without<ConnectedPlayerChunkStream>,
    >,
) {
    let mut await_inserts: SmallVec<[_; 4]> = SmallVec::new();
    let mut ready_inserts: SmallVec<[_; 4]> = SmallVec::new();
    for (entity, player, awaiting) in players_q.iter_mut() {
        match awaiting {
            None => {
                let (result, tx) = AsyncResult::new_pair();
                engine
                    .0
                    .network_thread
                    .send_command(NetworkThreadServerCommand::OpenNewStream(player.connection_key, tx));
                await_inserts.push((entity, ConnectedPlayerAwaitingChunkStream { result }));
            }
            Some(mut awaiting) => match awaiting.result.poll() {
                None => continue,
                Some(Ok((stream, key))) => {
                    ready_inserts.push((
                        entity,
                        ConnectedPlayerChunkStream {
                            s2c_chunk_stream: Arc::clone(stream),
                            s2c_chunk_stream_key: *key,
                        },
                    ));
                    commands.entity(entity).remove::<ConnectedPlayerAwaitingChunkStream>();
                }
                Some(Err(e)) => {
                    error_once!(
                        "Player {} ({}) could not get a chunk stream: {}",
                        player.authenticated_info.player_character,
                        player.authenticated_info.address,
                        e
                    );
                    continue;
                }
            },
        }
    }
    commands.insert_batch(await_inserts);
    commands.insert_batch(ready_inserts);
}

fn server_system_process_chunk_sending(
    engine: Res<GameServerResource>,
    mut voxel_q: Single<&mut VoxelUniverse<ServerData>>,
    connected_players_q: Populated<
        (Entity, &ConnectedPlayer, &ConnectedPlayerChunkStream),
        With<BootstrappedGameDataTag>,
    >,
) {
    // TODO: send only nearby chunks, not everything. Also don't iterate every chunk every tick.
    let voxels = &mut *voxel_q;

    let engine = &engine.0 as &GameServer;

    let mut send_list: SmallVec<[&PacketStream; 8]> = SmallVec::new();

    for (&position, loaded_chunk) in voxels.loaded_chunks_mut().chunks.iter_mut() {
        send_list.clear();
        let chunk_rev = loaded_chunk.local_revision();
        let chunk_player_list = &mut loaded_chunk.mutate_without_revision().extra_data.player_held_revisions;
        // remove disconnected players
        chunk_player_list.retain(|&player, _rev| connected_players_q.contains(player));
        // find players with outdated revisions
        for (pid, _player, stream) in connected_players_q.iter() {
            let entry = chunk_player_list.entry(pid).or_insert_with(|| {
                send_list.push(&stream.s2c_chunk_stream);
                chunk_rev
            });
            if *entry < chunk_rev {
                send_list.push(&stream.s2c_chunk_stream);
                *entry = chunk_rev;
            }
        }
        if send_list.is_empty() {
            continue;
        }
        // serialize chunk once and send to all players
        // TODO: tick counter system
        send_chunk_to_players(0, engine, position, loaded_chunk, &send_list);
    }
}

fn send_chunk_to_players(
    tick: u64,
    engine: &GameServer,
    pos: AbsChunkPos,
    chunk: &MutWatcher<Chunk<ServerData>>,
    peers: &[&PacketStream],
) {
    let mut builder = new_packet_builder::<chunk_data_stream_packet::Owned>();
    let mut pkt_root = builder.init_root();
    pkt_root.set_id(PacketId::ChunkData);
    pkt_root.set_timestamp_ms(engine.network_thread.packet_timestamp());
    let mut root = pkt_root.init_payload();
    root.set_tick(tick);
    let mut position = root.reborrow().init_position();
    position.set_x(pos.x);
    position.set_y(pos.y);
    position.set_z(pos.z);
    chunk.write_full(chunk.local_revision(), &mut root.reborrow().init_data());
    let mut packet = PacketWrapper::from(builder);

    // TODO: error handling, throttling
    for stream in peers.iter().skip(1) {
        let _ = stream.send_packet(packet.clone_mut());
    }
    if let Some(first) = peers.first() {
        let _ = first.send_packet(packet);
    }
}
