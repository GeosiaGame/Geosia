//! Client-side voxel world rendering

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use capnp::message::TypedReader;
use gs_common::network::transport::RPC_LOCAL_READER_OPTIONS;
use gs_common::prelude::*;
use gs_common::voxel::plugin::{
    BlockRegistryHolder, NetworkVoxelClient, VoxelUniverse, VoxelUniverseBuilder, CHUNK_PACKET_QUEUE_LENGTH,
};
use gs_common::InGameSystemSet;
use gs_schemas::coordinates::{AbsChunkPos, RelChunkPos};
use gs_schemas::dependencies::itertools::iproduct;
use gs_schemas::mutwatcher::MutWatcher;
use gs_schemas::schemas::network_capnp as rpc;
use gs_schemas::voxel::chunk::Chunk;
use gs_schemas::voxel::chunk_group::ChunkGroup;
use gs_schemas::voxel::voxeltypes::{BlockEntry, EMPTY_BLOCK_NAME};
use meshgen::mesh_from_chunk;
use smallvec::SmallVec;
use tokio_util::bytes::Bytes;

use crate::{ClientData, WhiteMaterialResource};

pub mod meshgen;

/// Client Chunk type
pub type ClientChunk = Chunk<ClientData>;
/// Client ChunkGroup type
pub type ClientChunkGroup = ChunkGroup<ClientData>;

/// Client-only per-chunk data storage
#[derive(Clone, Default)]
pub struct ClientChunkData {
    //
}

/// Client-only per-chunk-group data storage
#[derive(Clone, Default)]
pub struct ClientChunkGroupData {
    //
}

/// Extensions to the [`VoxelUniverseBuilder`]
pub trait ClientVoxelUniverseBuilder: Sized {
    /// Attaches the client-specific parts of the chunk streaming system.
    fn with_client_chunk_system(self) -> Self;
}

impl<'world> ClientVoxelUniverseBuilder for VoxelUniverseBuilder<'world, ClientData> {
    fn with_client_chunk_system(mut self) -> Self {
        self.bundle.world_scope(|world| {
            let fixed_pre_update = FixedPreUpdate.intern();
            let mut schedules = world.resource_mut::<Schedules>();
            schedules
                .get_mut(fixed_pre_update)
                .unwrap()
                .add_systems((chunk_packet_receiver_system).in_set(InGameSystemSet));
        });
        self
    }
}

fn chunk_packet_receiver_system(world: &mut World) {
    let white_material = world.get_resource::<WhiteMaterialResource>().unwrap().clone();

    let mut nvc = world.query::<&mut NetworkVoxelClient<ClientData>>();
    let mut nvc = nvc.get_single_mut(world).unwrap();
    let mut batch: SmallVec<[Bytes; CHUNK_PACKET_QUEUE_LENGTH]> = SmallVec::new();
    for _ in 0..CHUNK_PACKET_QUEUE_LENGTH {
        if let Ok(packet) = nvc.chunk_packet_receiver.try_recv() {
            batch.push(packet);
        } else {
            break;
        }
    }

    for raw_packet in batch {
        if let Err(e) = handle_chunk_packet(raw_packet, world, &white_material) {
            error!("Error while processing received chunk packet: {e}");
        }
    }
}

fn handle_chunk_packet(raw_packet: Bytes, world: &mut World, white_material: &WhiteMaterialResource) -> Result<()> {
    let mut slice = &raw_packet as &[u8];
    let msg = capnp::serialize::read_message_from_flat_slice(&mut slice, RPC_LOCAL_READER_OPTIONS)?;
    let typed_reader = TypedReader::<_, rpc::chunk_data_stream_packet::Owned>::new(msg);
    let root = typed_reader.get()?;
    let cpos_r = root.reborrow().get_position()?;
    let pos = AbsChunkPos::new(cpos_r.get_x(), cpos_r.get_y(), cpos_r.get_z());
    let data_r = root.reborrow().get_data()?;

    {
        let block_registry = Arc::clone(&world.resource::<BlockRegistryHolder>().0);
        let empty = block_registry
            .lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref())
            .context("no empty block")?
            .0;

        let mut universe = world.query::<&mut VoxelUniverse<ClientData>>();
        let Ok(mut universe) = universe.get_single_mut(world) else {
            warn!("Missing voxel universe while processing chunk data packet, did the game already shut down?");
            return Ok(());
        };

        let chunks = universe.loaded_chunks_mut();
        for (cx, cy, cz) in iproduct!(-1..=1, -1..=1, -1..=1) {
            let cpos = RelChunkPos::new(cx, cy, cz) + pos;
            if !chunks.chunks.contains_key(&cpos) {
                let chunk = ClientChunk::new(BlockEntry::new(empty, 0), Default::default());
                chunks.chunks.insert(cpos, MutWatcher::new(chunk));
            }
        }
        let mid_chunk = ClientChunk::read_full(&data_r, Default::default())?;
        chunks.chunks.insert(pos, MutWatcher::new(mid_chunk));

        for (pos, _) in chunks.chunks.iter() {
            let chunks = &chunks.get_neighborhood_around(*pos).transpose_option();
            if let Some(chunks) = chunks {
                let chunk_mesh = mesh_from_chunk(&block_registry, chunks).unwrap();

                let mesh = world.resource_mut::<Assets<Mesh>>().add(chunk_mesh);

                world.spawn(PbrBundle {
                    mesh,
                    material: white_material.mat.clone().unwrap(),
                    transform: Transform::from_xyz(0.0, 0.0, 0.0),
                    ..default()
                });
            }
        }
    }

    info!("Received chunk packet at position {pos}");
    Ok(())
}
