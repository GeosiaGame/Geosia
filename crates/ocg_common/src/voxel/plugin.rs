//! The Bevy plugin for voxel universe handling.

use std::marker::PhantomData;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;
use ocg_schemas::coordinates::AbsChunkPos;
use ocg_schemas::schemas::network_capnp::stream_header::StandardTypes;
use ocg_schemas::schemas::NetworkStreamHeader;
use ocg_schemas::voxel::chunk_group::ChunkGroup;
use ocg_schemas::voxel::voxeltypes::BlockRegistry;
use ocg_schemas::{GameSide, OcgExtraData};
use tokio_util::bytes::Bytes;

use crate::network::thread::{NetworkThread, NetworkThreadState};
use crate::network::transport::InProcessStream;
use crate::prelude::*;
use crate::voxel::persistence::ChunkPersistenceLayer;
use crate::{InGameSystemSet, ServerData};

/// The maximum number of stored chunk packets before applying stream backpressure.
pub const CHUNK_PACKET_QUEUE_LENGTH: usize = 64;

/// Initializes the settings related to the voxel universe.
#[derive(Default)]
pub struct VoxelUniversePlugin<ExtraData: OcgExtraData> {
    _extra_data: PhantomData<ExtraData>,
}

impl<ExtraData: OcgExtraData> Plugin for VoxelUniversePlugin<ExtraData> {
    fn build(&self, app: &mut App) {
        if ExtraData::side() == GameSide::Server {
            app.add_systems(PreUpdate, (system_process_chunk_loading).in_set(InGameSystemSet));
        }
    }

    fn name(&self) -> &str {
        "common::VoxelUniversePlugin"
    }

    fn is_unique(&self) -> bool {
        true
    }
}

impl<ExtraData: OcgExtraData> VoxelUniversePlugin<ExtraData> {
    /// Constructor.
    pub fn new() -> Self {
        Self {
            _extra_data: Default::default(),
        }
    }
}

/// A tag component marking voxel universes regardless of the generic type.
#[derive(Clone, Copy, Component)]
pub struct VoxelUniverseTag;

/// The chunk-storing component of a voxel universe, it's spawned along with other components that will manage its storage.
#[derive(Component)]
pub struct VoxelUniverse<ExtraData: OcgExtraData> {
    loaded_chunks: ChunkGroup<ExtraData>,
    _extra_data: PhantomData<ExtraData>,
}

/// Persistent storage for chunks, exists alongside VoxelUniverse on servers.
#[derive(Component)]
pub struct PersistentVoxelStorage<ExtraData: OcgExtraData> {
    persistence_layer: Box<dyn ChunkPersistenceLayer<ExtraData>>,
}

/// Network chunk streaming client, exists alongside VoxelUniverse on clients.
#[derive(Component)]
pub struct NetworkVoxelClient<ExtraData: OcgExtraData> {
    _extra_data: PhantomData<ExtraData>,
    /// Public for ocg_client usage, to allow receiving&processing chunk packets.
    pub chunk_packet_receiver: AsyncBoundedReceiver<Bytes>,
}

/// The bevy [`Resource`] for shared voxel registry access from systems.
#[derive(Resource, ExtractResource, Clone, Deref)]
pub struct BlockRegistryHolder(pub Arc<BlockRegistry>);

/// Builder for voxel universe initialization
pub struct VoxelUniverseBuilder<'world, ExtraData: OcgExtraData> {
    _registry: Arc<BlockRegistry>,
    /// The bundle being spawned
    pub bundle: EntityWorldMut<'world>,
    _extra_data: PhantomData<ExtraData>,
}

impl<'world, ED: OcgExtraData> VoxelUniverseBuilder<'world, ED> {
    /// Starts initializing a new voxel universe in a bevy World. Cannot be used on a World without cleaning up the previous universe first.
    pub fn new(world: &'world mut World, registry: Arc<BlockRegistry>) -> Result<Self> {
        let mut old_worlds = world.query::<&VoxelUniverseTag>();
        if old_worlds.iter(world).next().is_some() {
            bail!("Existing voxel worlds still in the bevy app");
        }

        world.insert_resource(BlockRegistryHolder(Arc::clone(&registry)));
        let bundle = world.spawn((VoxelUniverseTag, VoxelUniverse::<ED>::new(default())));

        Ok(Self {
            _registry: registry,
            bundle,
            _extra_data: default(),
        })
    }

    /// Adds persistent storage support to the universe.
    pub fn with_persistent_storage(
        mut self,
        mut persistence_layer: Box<dyn ChunkPersistenceLayer<ED>>,
    ) -> Result<Self> {
        if self.bundle.contains::<NetworkVoxelClient<ED>>() {
            bail!("Universe already has a network client, cannot add persistent storage");
        }

        // TODO: remove
        let mut universe = self.bundle.get_mut::<VoxelUniverse<ED>>().unwrap();
        persistence_layer.request_load(&[AbsChunkPos::ZERO]);
        let resp = persistence_layer
            .try_dequeue_responses(1)
            .into_iter()
            .next()
            .unwrap()
            .unwrap();
        universe.loaded_chunks.chunks.insert(resp.0, resp.1);

        self.bundle.insert(PersistentVoxelStorage::<ED> { persistence_layer });
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

impl<ExtraData: OcgExtraData> VoxelUniverse<ExtraData> {
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
}

impl<ED: OcgExtraData> NetworkVoxelClient<ED> {
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

fn system_process_chunk_loading(
    _registry: Res<BlockRegistryHolder>,
    mut voxels: Query<&mut VoxelUniverse<ServerData>>,
) {
    let _voxels: &mut VoxelUniverse<_> = voxels.single_mut().as_mut();
    // TODO: actually load chunks
}
