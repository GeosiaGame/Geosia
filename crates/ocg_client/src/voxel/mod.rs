//! Client-side voxel world rendering

use ocg_schemas::voxel::chunk::Chunk;
use ocg_schemas::voxel::chunk_group::ChunkGroup;

use crate::ClientData;

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
