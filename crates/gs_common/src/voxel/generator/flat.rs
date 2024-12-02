//! Simple flat world generator

use gs_schemas::{
    coordinates::{AbsBlockPos, InChunkPos, InChunkRange, CHUNK_DIM},
    voxel::{chunk::Chunk, chunk_storage::ChunkStorage, voxeltypes::BlockEntry},
    GsExtraData,
};

use super::VoxelGenerator;
use crate::prelude::*;

/// A layer of blocks to generate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FlatLayer {
    /// The type of block to fill the layer with.
    pub block_type: BlockEntry,
    /// The thickness of the layer in blocks, must be positive.
    pub thickness: i32,
}

/// A simple voxel generator that creates a world composed of many stacked flat layers.
/// The bottom layer is copied downwards, the top layer is copied upwards to fill the non-defined space.
pub struct FlatGenerator {
    start_y: i32,
    layers: Vec<FlatLayer>,
}

impl FlatGenerator {
    /// Constructs a generator from the given arguments, returning an error if invalid layers were given.
    pub fn new(start_y: i32, layers: Vec<FlatLayer>) -> Result<Self> {
        if layers.is_empty() {
            bail!("Empty layers given to the flat generator")
        }
        if layers.iter().any(|l| l.thickness <= 0) {
            bail!("Invalid non-positive thickness layer in {layers:?}");
        }
        Ok(Self { start_y, layers })
    }

    fn layer_for(&self, y: i32) -> FlatLayer {
        if y <= self.start_y {
            return self.layers[0];
        }
        let mut cur_y = self.start_y;
        for layer in self.layers.iter().copied() {
            let end_y = cur_y + layer.thickness;
            if (cur_y..end_y).contains(&y) {
                return layer;
            }
            cur_y = end_y;
        }
        *self.layers.last().unwrap()
    }
}

impl<ED: GsExtraData> VoxelGenerator<ED> for FlatGenerator {
    fn generate_chunk(
        &self,
        position: gs_schemas::coordinates::AbsChunkPos,
        extra_data: <ED as GsExtraData>::ChunkData,
    ) -> Chunk<ED> {
        let first_blockpos: AbsBlockPos = position.into();
        let first_layer = self.layer_for(first_blockpos.y).block_type;
        let mut chunk = Chunk::new(first_layer, extra_data);
        for in_y in 1..CHUNK_DIM {
            let global_y = first_blockpos.y + in_y;
            let layer = self.layer_for(global_y).block_type;
            let min = InChunkPos::try_new(0, in_y, 0).unwrap();
            let max = InChunkPos::try_new(CHUNK_DIM - 1, in_y, CHUNK_DIM - 1).unwrap();
            chunk.blocks.fill(InChunkRange::from_corners(min, max), layer);
        }
        chunk
    }
}
