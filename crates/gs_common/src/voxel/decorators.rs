//! World decorator data

//! The builtin biome decorator types.
//! Most of this will be moved to a "base" mod at some point in the future.

use bevy_math::IVec3;
use gs_schemas::coordinates::{InChunkPos, CHUNK_DIM};
use gs_schemas::dependencies::itertools::iproduct;
use gs_schemas::registry::{RegistryDataSet, RegistryName};
use gs_schemas::voxel::chunk_storage::ChunkStorage;
use gs_schemas::voxel::generation::decorator::{DecoratorDefinition, DecoratorRegistry};
use gs_schemas::voxel::voxeltypes::{BlockEntry, EMPTY_BLOCK_NAME};
use rand::{distributions::Uniform, Rng};

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
            placement_check_fn: Some(|_def, rand, pos, height, elevation, _temperature, moisture| {
                pos.y == height && elevation <= 4.0 && moisture > 1.0 && rand.gen::<f64>() > 0.99
            }),
            placer_fn: Some(|_def, chunk, rand, pos, chunk_pos, block_registry| {
                let log_id = block_registry.lookup_name_to_object(LOG_BLOCK_NAME.as_ref()).unwrap().0;
                let leaves_id = block_registry
                    .lookup_name_to_object(LEAVES_BLOCK_NAME.as_ref())
                    .unwrap()
                    .0;
                let empty_id = block_registry
                    .lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref())
                    .unwrap()
                    .0;

                let distribution = Uniform::new(4, 6);
                let tree_height = rand.sample(distribution);

                for y in 0..tree_height {
                    let new_pos = pos - *chunk_pos * CHUNK_DIM + IVec3::new(0, y, 0);
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
                        InChunkPos::try_from_ivec3(new_pos).expect("modulo failed???"),
                        BlockEntry::new(log_id, 0),
                    );
                }
                for (x, y, z) in iproduct!(-3..=3, 0..=3, -3..=3) {
                    // check if it's outside a sphere
                    if x * x + y * y + z * z > 3 * 3 {
                        continue;
                    }
                    let new_pos = pos - *chunk_pos * CHUNK_DIM + IVec3::new(x, y + tree_height - 2, z);
                    if new_pos.x < 0
                        || new_pos.x >= CHUNK_DIM
                        || new_pos.y < 0
                        || new_pos.y >= CHUNK_DIM
                        || new_pos.z < 0
                        || new_pos.z >= CHUNK_DIM
                    {
                        continue;
                    }
                    let new_pos = InChunkPos::try_from_ivec3(new_pos).expect("modulo failed???");
                    if chunk.get(new_pos).id != empty_id {
                        continue;
                    }
                    chunk.put(new_pos, BlockEntry::new(leaves_id, 0));
                }
            }),
        })
        .unwrap();
}
