//! Client-side voxel world rendering

use bevy::color::palettes::tailwind;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use capnp::message::TypedReader;
use gs_common::network::transport::RPC_LOCAL_READER_OPTIONS;
use gs_common::prelude::*;
use gs_common::voxel::plugin::{
    BlockRegistryHolder, NetworkVoxelClient, VoxelUniverse, VoxelUniverseBuilder, CHUNK_PACKET_QUEUE_LENGTH,
};
use gs_common::InGameSystemSet;
use gs_schemas::coordinates::{AbsBlockPos, AbsChunkPos};
use gs_schemas::mutwatcher::{MutWatcher, RevisionNumber};
use gs_schemas::schemas::network_capnp as rpc;
use gs_schemas::voxel::chunk::Chunk;
use gs_schemas::voxel::chunk_group::ChunkGroup;
use meshgen::mesh_from_chunk;
use smallvec::{smallvec, SmallVec};
use tokio_util::bytes::Bytes;

use crate::ClientData;

pub mod meshgen;

/// Client Chunk type
pub type ClientChunk = Chunk<ClientData>;
/// Client ChunkGroup type
pub type ClientChunkGroup = ChunkGroup<ClientData>;
/// Client VoxelUniverse
pub type ClientVoxelUniverse = VoxelUniverse<ClientData>;

/// Keeps track of the render entities associated with a chunk
#[derive(Clone, Default)]
struct ChunkMeshState {
    meshes: SmallVec<[Handle<Mesh>; 4]>,
    entities: SmallVec<[Entity; 4]>,
}

/// Client-only per-chunk data storage
#[derive(Clone, Default)]
pub struct ClientChunkData {
    mesh: Option<MutWatcher<ChunkMeshState>>,
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

impl ClientVoxelUniverseBuilder for VoxelUniverseBuilder<'_, ClientData> {
    fn with_client_chunk_system(mut self) -> Self {
        self.bundle.world_scope(|world| {
            let fixed_pre_update = FixedPreUpdate.intern();
            let fixed_update = FixedUpdate.intern();
            let mut schedules = world.resource_mut::<Schedules>();
            schedules
                .get_mut(fixed_pre_update)
                .unwrap()
                .add_systems((client_chunk_packet_receiver_system).in_set(InGameSystemSet));
            schedules
                .get_mut(fixed_update)
                .unwrap()
                .add_systems((client_chunk_mesher_system).in_set(InGameSystemSet));
        });
        self
    }
}

fn client_chunk_packet_receiver_system(
    mut nvc_q: Query<&mut NetworkVoxelClient<ClientData>>,
    mut voxel_q: Query<&mut ClientVoxelUniverse>,
) {
    let mut voxels = voxel_q
        .get_single_mut()
        .context("Missing universe while handling chunk packet, did the game already shut down?")
        .unwrap();
    let mut nvc = nvc_q.single_mut();
    let mut batch: SmallVec<[Bytes; CHUNK_PACKET_QUEUE_LENGTH]> = SmallVec::new();
    for _ in 0..CHUNK_PACKET_QUEUE_LENGTH {
        if let Ok(packet) = nvc.chunk_packet_receiver.try_recv() {
            batch.push(packet);
        } else {
            break;
        }
    }

    let voxels = &mut *voxels;
    for raw_packet in batch {
        if let Err(e) = handle_chunk_packet(raw_packet, voxels) {
            error!("Error while processing received chunk packet: {e}");
        }
    }
}

fn handle_chunk_packet(raw_packet: Bytes, voxels: &mut ClientVoxelUniverse) -> Result<()> {
    let mut slice = &raw_packet as &[u8];
    let msg = capnp::serialize::read_message_from_flat_slice(&mut slice, RPC_LOCAL_READER_OPTIONS)?;
    let typed_reader = TypedReader::<_, rpc::chunk_data_stream_packet::Owned>::new(msg);
    let root = typed_reader.get()?;
    let cpos_r = root.reborrow().get_position()?;
    let pos = AbsChunkPos::new(cpos_r.get_x(), cpos_r.get_y(), cpos_r.get_z());
    let data_r = root.reborrow().get_data()?;
    let revision: RevisionNumber = root.get_revision().try_into()?;
    let chunk = ClientChunk::read_full(&data_r, default())?;

    voxels
        .loaded_chunks_mut()
        .chunks
        .insert(pos, MutWatcher::new_saved(chunk, revision));

    Ok(())
}

fn client_chunk_mesher_system(
    mut voxel_q: Query<&mut ClientVoxelUniverse>,
    block_registry: Res<BlockRegistryHolder>,
    mut voxel_material: Local<Option<Handle<StandardMaterial>>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    let Ok(mut voxels) = voxel_q.get_single_mut() else {
        return;
    };
    let voxels = &mut *voxels;

    let voxel_material = voxel_material.get_or_insert_with(|| {
        materials.add(StandardMaterial {
            base_color: tailwind::GRAY_500.into(),
            ..default()
        })
    });

    // Schedule new meshes for all outdated chunks
    let mut new_entries = Vec::new();
    let loaded_chunks = voxels.loaded_chunks();
    for (&pos, chunk) in loaded_chunks.chunks.iter() {
        let old_mesh = chunk.extra_data.mesh.as_ref();
        let needs_mesh = if let Some(old_mesh) = old_mesh {
            old_mesh.is_older_than(chunk)
        } else {
            true
        };
        if !needs_mesh {
            continue;
        }
        let Some(neighbors) = loaded_chunks.get_neighborhood_around(pos).transpose_option() else {
            continue;
        };
        let chunk_mesh = match mesh_from_chunk(&block_registry, &neighbors) {
            Ok(mesh) => mesh,
            Err(e) => {
                error!(position = %pos, error = %e, "Could not mesh chunk");
                continue;
            }
        };
        let mesh = meshes.add(chunk_mesh);
        trace!(position = %pos, "Spawning new chunk mesh");

        let entity = commands
            .spawn(PbrBundle {
                mesh: mesh.clone(),
                material: voxel_material.clone(),
                transform: Transform::from_translation(AbsBlockPos::from(pos).as_vec3()),
                ..default()
            })
            .id();

        let mesh = chunk.new_with_same_revision(ChunkMeshState {
            meshes: smallvec![mesh],
            entities: smallvec![entity],
        });

        new_entries.push((pos, mesh));
    }
    let loaded_chunks = voxels.loaded_chunks_mut();
    for (pos, mesh) in new_entries.into_iter() {
        let old_mesh = loaded_chunks
            .chunks
            .get_mut(&pos)
            .unwrap()
            .mutate_without_revision()
            .extra_data
            .mesh
            .replace(mesh);
        if let Some(old_mesh) = old_mesh {
            let old_mesh = old_mesh.into_inner();
            for mesh in old_mesh.meshes.iter() {
                meshes.remove(mesh);
            }
            for &entity in old_mesh.entities.iter() {
                if let Some(entity) = commands.get_entity(entity) {
                    entity.despawn_recursive();
                }
            }
        }
    }
}
