//! Client-side voxel world rendering

use ocg_schemas::voxel::chunk::Chunk;
use ocg_schemas::voxel::chunk_group::ChunkGroup;

pub mod meshgen;

/// Client Chunk type
pub type ClientChunk = Chunk<ClientChunkData>;
/// Client ChunkGroup type
pub type ClientChunkGroup = ChunkGroup<ClientChunkData, ClientChunkGroupData>;

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
