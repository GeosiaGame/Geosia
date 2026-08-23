//! Client-side voxel world rendering

use std::collections::VecDeque;

use gs_common::InGameSystemSet;
use gs_common::network::server::QueuedPacket;
use gs_common::network::transport::RPC_CLIENT_READER_OPTIONS;
use gs_common::prelude::rpc::chunk_data_stream_packet;
use gs_common::voxel::plugin::{BlockRegistryHolder, CHUNK_PACKET_QUEUE_LENGTH, VoxelUniverse, VoxelUniverseBuilder};
use gs_schemas::coordinates::{AbsBlockPos, AbsChunkPos};
use gs_schemas::dependencies::itertools::Itertools;
use gs_schemas::mutwatcher::MutWatcher;
use gs_schemas::schemas::CapnpExt;
use gs_schemas::voxel::chunk::Chunk;
use gs_schemas::voxel::chunk_group::ChunkGroup;
use meshgen::mesh_from_chunk;
use smallvec::SmallVec;

use crate::ClientData;
use crate::prelude::*;
use crate::voxel::meshgen::{ChunkMeshMaterial, default_chunk_material};

pub mod client_plugin;
pub mod meshgen;

/// Client [`Chunk`] type
pub type ClientChunk = Chunk<ClientData>;
/// Client [`ChunkGroup`] type
pub type ClientChunkGroup = ChunkGroup<ClientData>;
/// Client [`VoxelUniverse`]
pub type ClientVoxelUniverse = VoxelUniverse<ClientData>;

/// Keeps track of the render entities associated with a chunk
#[derive(Clone, Default, Debug)]
struct ChunkMeshState {
    entities: SmallVec<[Entity; 4]>,
}

/// Client-only per-chunk data storage
#[derive(Clone, Default, Debug)]
pub struct ClientChunkData {
    mesh: Option<MutWatcher<ChunkMeshState>>,
}

/// Client-only per-chunk-group data storage
#[derive(Clone, Default, Debug)]
pub struct ClientChunkGroupData {
    //
}

/// Network chunk streaming client, exists alongside [`VoxelUniverse`] on clients.
#[derive(Component)]
pub struct NetworkVoxelClient {
    /// Public for `gs_client` usage, to allow receiving&processing chunk packets.
    pub chunk_packet_queue: VecDeque<QueuedPacket>,
}

/// Extensions to the [`VoxelUniverseBuilder`]
pub trait ClientVoxelUniverseBuilder: Sized {
    /// Attaches the client-specific parts of the chunk streaming system.
    #[must_use]
    fn with_client_chunk_system(self) -> Self;
}

/// Registers the systems for the client voxel universe.
pub struct ClientVoxelUniversePlugin;

impl Plugin for ClientVoxelUniversePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Assets<ChunkMeshMaterial>>();
        app.add_systems(
            FixedPreUpdate,
            (client_chunk_packet_receiver_system).in_set(InGameSystemSet),
        );
        app.add_systems(FixedUpdate, (client_chunk_mesher_system).in_set(InGameSystemSet));
    }
}

impl ClientVoxelUniverseBuilder for VoxelUniverseBuilder<'_, ClientData> {
    fn with_client_chunk_system(mut self) -> Self {
        self.bundle.insert(NetworkVoxelClient {
            chunk_packet_queue: VecDeque::with_capacity(1024),
        });
        self
    }
}

fn client_chunk_packet_receiver_system(
    mut nvc_q: Single<&mut NetworkVoxelClient>,
    mut voxel_q: Single<&mut ClientVoxelUniverse>,
) {
    let voxels = &mut *voxel_q;
    let nvc = &mut *nvc_q;

    let to_drain = CHUNK_PACKET_QUEUE_LENGTH.min(nvc.chunk_packet_queue.len());
    for packet in nvc.chunk_packet_queue.drain(0..to_drain) {
        if let Err(e) = handle_chunk_packet(packet, voxels) {
            error!("Error while processing received chunk packet: {e}");
        }
    }
}

fn handle_chunk_packet(packet: QueuedPacket, voxels: &mut ClientVoxelUniverse) -> Result<()> {
    let typed_reader = packet
        .data
        .parse_typed::<chunk_data_stream_packet::Owned>(RPC_CLIENT_READER_OPTIONS)?;
    let root = typed_reader.get()?.get_payload()?;
    let pos = AbsChunkPos::from(IVec3::read_from_message(&root.reborrow().get_position()?)?);
    let data_r = root.reborrow().get_data()?;

    let mut chunk_entry = voxels.loaded_chunks_mut().chunks.entry(pos);
    let extra_data = if let std::collections::btree_map::Entry::Occupied(ref mut occupied) = chunk_entry {
        std::mem::take(&mut occupied.get_mut().mutate_without_revision().extra_data)
    } else {
        default()
    };
    let chunk_with_revision = ClientChunk::read_full(&data_r, extra_data)?;
    let revision = chunk_with_revision.local_revision();
    let chunk = chunk_with_revision.into_inner();

    match chunk_entry {
        std::collections::btree_map::Entry::Occupied(occupied) => {
            if let Some(e) = occupied.into_mut().mutate_from_server_revision(revision) {
                *e = chunk;
            }
        }
        std::collections::btree_map::Entry::Vacant(vacant) => {
            vacant.insert(MutWatcher::new_saved(chunk, revision));
        }
    }

    Ok(())
}

fn client_chunk_mesher_system(
    mut voxel_q: Query<&mut ClientVoxelUniverse>,
    block_registry: Res<BlockRegistryHolder>,
    mut voxel_material: Local<Option<Handle<ChunkMeshMaterial>>>,
    mut materials: ResMut<Assets<ChunkMeshMaterial>>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut commands: Commands,
) {
    let Ok(mut voxels) = voxel_q.single_mut() else {
        return;
    };
    let voxels = &mut *voxels;

    let voxel_material = voxel_material.get_or_insert_with(|| materials.add(default_chunk_material()));

    // Schedule new meshes for all outdated chunks
    let loaded_chunks = voxels.loaded_chunks();
    let mut chunk_mutations: Vec<(AbsChunkPos, MutWatcher<ChunkMeshState>)> = Vec::new();

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
        let mut chunk_meshes: SmallVec<[_; 4]> = SmallVec::new();
        if let Some(mesh) = chunk_mesh {
            chunk_meshes.push(mesh_assets.add(mesh));
        }
        let mut mesh_entities: SmallVec<[_; 4]> = SmallVec::new();
        if let Some(old_mesh) = old_mesh {
            mesh_entities.extend_from_slice(&old_mesh.entities[..]);
        }

        if mesh_entities.len() < chunk_meshes.len() {
            let to_spawn = chunk_meshes.len() - mesh_entities.len();
            let bundle = (
                Mesh3d::default(),
                MeshMaterial3d(voxel_material.clone()),
                Transform::from_translation(AbsBlockPos::from(pos).as_vec3()),
            );
            for _ in 0..to_spawn {
                mesh_entities.push(commands.spawn(bundle.clone()).id());
            }
        } else if mesh_entities.len() > chunk_meshes.len() {
            let to_despawn = mesh_entities.len() - chunk_meshes.len();
            for _ in 0..to_despawn {
                let Some(e) = mesh_entities.pop() else { continue };
                commands.entity(e).try_despawn();
            }
        }

        for (e, new_mesh) in mesh_entities.iter().copied().zip_eq(chunk_meshes) {
            commands.entity(e).insert(Mesh3d(new_mesh));
        }

        chunk_mutations.push((
            pos,
            chunk.new_with_same_revision(ChunkMeshState {
                entities: mesh_entities,
            }),
        ));
    }
    let loaded_chunks = voxels.loaded_chunks_mut();
    for (cpos, mutation) in chunk_mutations {
        let Some(chunk) = loaded_chunks.get_chunk_mut(cpos) else {
            unreachable!();
        };
        chunk.mutate_without_revision().extra_data.mesh = Some(mutation);
    }
}
