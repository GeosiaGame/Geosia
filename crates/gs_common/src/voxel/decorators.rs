//! World decorator data

//! The builtin biome decorator types.
//! Most of this will be moved to a "base" mod at some point in the future.

use bevy::prelude::FloatExt;
use noise::NoiseFn;

use gs_schemas::coordinates::{InChunkPos, RelBlockPos, CHUNK_DIM};
use gs_schemas::dependencies::itertools::iproduct;
use gs_schemas::registry::{RegistryDataSet, RegistryName};
use gs_schemas::voxel::chunk_storage::ChunkStorage;
use gs_schemas::voxel::generation::decorator::{DecoratorDefinition, DecoratorRegistry};
use gs_schemas::voxel::voxeltypes::{BlockEntry, EMPTY_BLOCK_NAME};

use crate::voxel::biomes::PLAINS_BIOME_NAME;
use crate::voxel::blocks::{LEAVES_BLOCK_NAME, LOG_BLOCK_NAME};

/// Registry name for tree.
pub const TREE_DECORATOR_NAME: RegistryName = RegistryName::gs_const("tree");

/// Installs the base set of biome decorators into the given block registry.
pub fn setup_basic_decorators(registry: &mut DecoratorRegistry) {
    registry
        .push_object(DecoratorDefinition {
            name: TREE_DECORATOR_NAME,
            biomes: RegistryDataSet::new([PLAINS_BIOME_NAME].into_iter().collect()),
            salt: 124567,
            placement_check: |_def, weird_noise, pos, height, elevation, _temperature, moisture| {
                let noise_valid = weird_noise.get([pos.x as f64 / 128.0 * 4684.26, pos.z as f64 / 128.0 * 2317.15]) > 8.0;
                noise_valid && pos.y == height && elevation <= 4.0 && moisture > 1.0
            },
            placer: |_def, chunk, weird_noise, in_chunk_pos, chunk_pos, block_registry| {
                let (i_log, _) = block_registry.lookup_name_to_object(LOG_BLOCK_NAME.as_ref()).unwrap();
                let (i_leaves, _) = block_registry.lookup_name_to_object(LEAVES_BLOCK_NAME.as_ref()).unwrap();
                let (i_empty, _) = block_registry.lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref()).unwrap();

                let g_pos = in_chunk_pos + chunk_pos.block_pos(InChunkPos::ZERO);
                let tree_height = weird_noise.get([g_pos.x as f64 / 128.0 * 8357.71, g_pos.z as f64 / 128.0 * 1767.14]);
                let tree_height = tree_height.remap(-4.0, 4.0, 7.0, 10.0).round() as i32;

                for y in 0..tree_height {
                    let new_pos = in_chunk_pos + RelBlockPos::new(0, y, 0);
                    if new_pos.x < 0
                        || new_pos.x >= CHUNK_DIM
                        || new_pos.y < 0
                        || new_pos.y >= CHUNK_DIM
                        || new_pos.z < 0
                        || new_pos.z >= CHUNK_DIM
                    {
                        continue;
                    }
                    chunk.put(
                        InChunkPos::try_from_ivec3(*new_pos).expect("modulo failed???"),
                        BlockEntry::new(i_log, 0),
                    );
                }
                for (x, y, z) in iproduct!(-4..=4, -1..=4, -4..=4) {
                    // check if it's outside a sphere
                    if x * x + y * y + z * z > 5 * 5 {
                        continue;
                    }
                    let new_pos = in_chunk_pos + RelBlockPos::new(x, y + tree_height - 3, z);
                    if new_pos.x < 0
                        || new_pos.x >= CHUNK_DIM
                        || new_pos.y < 0
                        || new_pos.y >= CHUNK_DIM
                        || new_pos.z < 0
                        || new_pos.z >= CHUNK_DIM
                    {
                        continue;
                    }
                    let new_pos = InChunkPos::try_from_ivec3(*new_pos).expect("modulo failed???");
                    if chunk.get(new_pos).id != i_empty {
                        continue;
                    }
                    chunk.put(new_pos, BlockEntry::new(i_leaves, 0));
                }
            },
        })
        .unwrap();
}
