//! The Bevy plugin for voxel universe handling.

use std::marker::PhantomData;
use std::sync::Arc;

use bevy::prelude::*;
use gs_schemas::coordinates::AbsChunkPos;
use gs_schemas::dependencies::itertools::Itertools;
use gs_schemas::schemas::network_capnp::stream_header::StandardTypes;
use gs_schemas::schemas::NetworkStreamHeader;
use gs_schemas::voxel::biome::BiomeRegistry;
use gs_schemas::voxel::chunk_group::ChunkGroup;
use gs_schemas::voxel::voxeltypes::BlockRegistry;
use gs_schemas::{GameSide, GsExtraData};
use tokio_util::bytes::Bytes;

use crate::network::thread::{NetworkThread, NetworkThreadState};
use crate::network::transport::InProcessStream;
use crate::prelude::*;
use crate::voxel::generator::{WORLD_SIZE_XZ, WORLD_SIZE_Y};
use crate::voxel::persistence::ChunkPersistenceLayer;
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

impl<ExtraData: GsExtraData> VoxelUniversePlugin<ExtraData> {
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
pub struct VoxelUniverse<ExtraData: GsExtraData> {
    loaded_chunks: ChunkGroup<ExtraData>,
    _extra_data: PhantomData<ExtraData>,
}

/// Persistent storage for chunks, exists alongside VoxelUniverse on servers.
#[derive(Component)]
pub struct PersistentVoxelStorage<ExtraData: GsExtraData> {
    _persistence_layer: Box<dyn ChunkPersistenceLayer<ExtraData>>,
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
    pub fn with_persistent_storage(
        mut self,
        mut persistence_layer: Box<dyn ChunkPersistenceLayer<ED>>,
    ) -> Result<Self> {
        if self.bundle.contains::<NetworkVoxelClient<ED>>() {
            bail!("Universe already has a network client, cannot add persistent storage");
        }

        // TODO: remove
        let mut universe = self.bundle.get_mut::<VoxelUniverse<ED>>().unwrap();

        let chunk_positions = (-WORLD_SIZE_XZ..=WORLD_SIZE_XZ)
            .cartesian_product(-WORLD_SIZE_Y..=WORLD_SIZE_Y)
            .cartesian_product(-WORLD_SIZE_XZ..=WORLD_SIZE_XZ)
            .map(|((x, y), z)| AbsChunkPos::new(x, y, z))
            .collect_vec();
        persistence_layer.request_load(&*chunk_positions);
        for chunk in persistence_layer
            .try_dequeue_responses(chunk_positions.len())
            .into_iter()
        {
            let (pos, chunk) = chunk.unwrap();
            universe.loaded_chunks.chunks.insert(pos, chunk);
        }

        self.bundle.insert(PersistentVoxelStorage::<ED> {
            _persistence_layer: persistence_layer,
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

fn system_process_chunk_loading(
    _registry: Res<BlockRegistryHolder>,
    mut voxels: Query<&mut VoxelUniverse<ServerData>>,
) {
    let _voxels: &mut VoxelUniverse<_> = voxels.single_mut().as_mut();
    // TODO: actually load chunks
}
