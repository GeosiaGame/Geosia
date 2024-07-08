//! The Bevy plugin for voxel universe handling.

use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::Arc;

use bevy::prelude::*;
use capnp::message::TypedBuilder;
use gs_schemas::coordinates::{AbsBlockPos, AbsChunkPos, AbsChunkRange, RelChunkPos};
use gs_schemas::dependencies::itertools::Itertools;
use gs_schemas::mutwatcher::{MutWatcher, RevisionNumber};
use gs_schemas::schemas::network_capnp::stream_header::StandardTypes;
use gs_schemas::schemas::NetworkStreamHeader;
use gs_schemas::voxel::biome::BiomeRegistry;
use gs_schemas::voxel::chunk::Chunk;
use gs_schemas::voxel::chunk_group::ChunkGroup;
use gs_schemas::voxel::voxeltypes::BlockRegistry;
use gs_schemas::{GameSide, GsExtraData};
use smallvec::SmallVec;
use tokio_util::bytes::Bytes;

use crate::network::server::{ConnectedPlayer, ConnectedPlayersTable};
use crate::network::thread::{NetworkThread, NetworkThreadState};
use crate::network::transport::InProcessStream;
use crate::network::PeerAddress;
use crate::voxel::persistence::ChunkPersistenceLayer;
use crate::{prelude::*, GameServer, GameServerResource};
use crate::{InGameSystemSet, ServerData};

/// The maximum number of stored chunk packets before applying stream backpressure.
pub const CHUNK_PACKET_QUEUE_LENGTH: usize = 64;

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
                FixedPostUpdate,
                (server_system_process_chunk_sending).in_set(InGameSystemSet),
            );
        }
    }

    fn name(&self) -> &str {
        "common::VoxelUniversePlugin"
    }

    fn is_unique(&self) -> bool {
        true
    }
}

impl<ExtraData: GsExtraData> VoxelUniversePlugin<ExtraData> {
    /// Constructor.
    pub fn new() -> Self {
        Self {
            _extra_data: Default::default(),
        }
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

/// Persistent storage for chunks, exists alongside VoxelUniverse on servers.
#[derive(Component)]
pub struct PersistentVoxelStorage<ExtraData: GsExtraData> {
    persistence_layer: Box<dyn ChunkPersistenceLayer<ExtraData>>,
    live_requests: BTreeSet<AbsChunkPos>,
}

/// Network chunk streaming client, exists alongside VoxelUniverse on clients.
#[derive(Component)]
pub struct NetworkVoxelClient<ExtraData: GsExtraData> {
    _extra_data: PhantomData<ExtraData>,
    /// Public for gs_client usage, to allow receiving&processing chunk packets.
    pub chunk_packet_receiver: AsyncBoundedReceiver<Bytes>,
}

/// The bevy [`Resource`] for shared voxel registry access from systems.
#[derive(Resource, Clone, Deref)]
pub struct BlockRegistryHolder(pub Arc<BlockRegistry>);

/// The bevy [`Resource`] for shared biome registry access from systems.
#[derive(Resource, Clone, Deref)]
pub struct BiomeRegistryHolder(pub Arc<BiomeRegistry>);

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
    ) -> Result<Self> {
        let mut old_worlds = world.query::<&VoxelUniverseTag>();
        if old_worlds.iter(world).next().is_some() {
            bail!("Existing voxel worlds still in the bevy app");
        }

        world.insert_resource(BlockRegistryHolder(Arc::clone(&block_registry)));
        world.insert_resource(BiomeRegistryHolder(Arc::clone(&biome_registry)));
        let bundle = world.spawn((VoxelUniverseTag, VoxelUniverse::<ED>::new(default())));

        Ok(Self {
            _block_registry: block_registry,
            _biome_registry: biome_registry,
            bundle,
            _extra_data: default(),
        })
    }

    /// Adds persistent storage support to the universe.
    pub fn with_persistent_storage(mut self, persistence_layer: Box<dyn ChunkPersistenceLayer<ED>>) -> Result<Self> {
        if self.bundle.contains::<NetworkVoxelClient<ED>>() {
            bail!("Universe already has a network client, cannot add persistent storage");
        }

        // TODO: make the player load the chunks
        self.bundle.world_scope(|w| {
            w.spawn((VoxelPosition(AbsBlockPos::ZERO), ChunkLoader { radius: 4 }));
        });

        self.bundle.insert(PersistentVoxelStorage::<ED> {
            persistence_layer,
            live_requests: default(),
        });
        Ok(self)
    }

    /// Adds a network client to stream chunks from a server.
    pub fn with_network_client<NS: NetworkThreadState>(mut self, net_thread: &NetworkThread<NS>) -> Result<Self> {
        if self.bundle.contains::<PersistentVoxelStorage<ED>>() {
            bail!("Universe already has a network client, cannot add persistent storage");
        }
        let (tx, rx) = async_bounded_channel(CHUNK_PACKET_QUEUE_LENGTH);
        self.bundle.insert(NetworkVoxelClient::<ED> {
            _extra_data: default(),
            chunk_packet_receiver: rx,
        });
        net_thread.insert_stream_handler(
            NetworkStreamHeader::Standard(StandardTypes::ChunkData),
            Box::new(move |_state, stream| {
                Box::pin(NetworkVoxelClient::<ED>::chunk_stream_handler(stream, tx.clone()))
            }),
        );
        Ok(self)
    }

    /// Finishes the setup, returns the entity ID holding the VoxelUniverse component.
    pub fn build(self) -> EntityWorldMut<'world> {
        self.bundle
    }
}

impl<ExtraData: GsExtraData> VoxelUniverse<ExtraData> {
    /// Constructor.
    pub fn new(group_data: ExtraData::GroupData) -> Self {
        Self {
            loaded_chunks: ChunkGroup::with_data(group_data),
            _extra_data: Default::default(),
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

impl<ED: GsExtraData> NetworkVoxelClient<ED> {
    async fn chunk_stream_handler(stream: InProcessStream, packet_queue: AsyncBoundedSender<Bytes>) {
        let InProcessStream { mut rx, .. } = stream;
        while let Some(raw_packet) = rx.recv().await {
            if let Err(e) = packet_queue.send(raw_packet).await {
                error!("Error while queueing chunk data packet: {e}");
                break;
            }
        }
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
    let Ok((mut voxels, mut persistence, _)) = voxel_q.get_single_mut() else {
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

fn server_system_process_chunk_sending(
    engine: Res<GameServerResource>,
    mut voxel_q: Query<&mut VoxelUniverse<ServerData>>,
    connected_players_table_q: Query<&ConnectedPlayersTable>,
    connected_players_q: Query<&ConnectedPlayer>,
) {
    // TODO: send only nearby chunks, not everything. Also don't iterate every chunk every tick.
    let Ok(player_list) = connected_players_table_q.get_single() else {
        return;
    };
    let player_list = &player_list.players_by_address;
    if player_list.is_empty() {
        return;
    }

    let Ok(mut voxels) = voxel_q.get_single_mut() else {
        return;
    };
    let voxels = &mut *voxels;

    let engine = &engine.0 as &GameServer;

    let mut send_list: SmallVec<[PeerAddress; 8]> = SmallVec::new();

    for (&position, loaded_chunk) in voxels.loaded_chunks_mut().chunks.iter_mut() {
        send_list.clear();
        let chunk_rev = loaded_chunk.local_revision();
        let chunk_player_list = &mut loaded_chunk.mutate_without_revision().extra_data.player_held_revisions;
        // remove disconnected players
        chunk_player_list.retain(|&player, _rev| connected_players_q.contains(player));
        // find players with outdated revisions
        for (&peer, &player) in player_list.iter() {
            let entry = chunk_player_list.entry(player).or_insert_with(|| {
                send_list.push(peer);
                chunk_rev
            });
            if *entry < chunk_rev {
                send_list.push(peer);
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
    peers: &[PeerAddress],
) {
    let mut builder = TypedBuilder::<rpc::chunk_data_stream_packet::Owned>::new_default();
    let mut root = builder.init_root();
    root.set_tick(tick);
    root.set_revision(chunk.local_revision().into());
    let mut position = root.reborrow().init_position();
    position.set_x(pos.x);
    position.set_y(pos.y);
    position.set_z(pos.z);
    chunk.write_full(&mut root.reborrow().init_data());
    let mut buffer = Vec::new();
    capnp::serialize::write_message(&mut buffer, builder.borrow_inner()).unwrap();
    let buffer = Bytes::from(buffer);

    // TODO: error handling, throttling
    let peers: SmallVec<[_; 8]> = peers.into();
    let _ = engine.network_thread.schedule_task(move |rstate| {
        Box::pin(async move {
            let mut state = rstate.borrow_mut();
            for addr in peers {
                let my_buffer = buffer.clone();
                let Some(peer) = state.find_connected_client_mut(addr) else {
                    bail!("Cannot find connected client {addr:?} anymore");
                };
                if peer.chunk_stream.is_none() {
                    peer.chunk_stream = Some(
                        peer.open_stream(NetworkStreamHeader::Standard(StandardTypes::ChunkData))
                            .unwrap(),
                    );
                }
                let chunk_stream = peer.chunk_stream.as_mut().unwrap();
                chunk_stream.tx.send(my_buffer)?;
            }
            Ok(())
        })
    });
}
