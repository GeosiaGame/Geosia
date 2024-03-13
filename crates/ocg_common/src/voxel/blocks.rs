//! The builtin block types.
//! Most of this will be moved to a "base" mod at some point in the future.

use ocg_schemas::dependencies::rgb::RGBA8;
use ocg_schemas::registry::RegistryName;
use ocg_schemas::voxel::voxeltypes::BlockShapeSet::StandardShapedMaterial;
use ocg_schemas::voxel::voxeltypes::{BlockDefinition, BlockRegistry, EMPTY_BLOCK};

/// Registry name for stone.
pub const STONE_BLOCK_NAME: RegistryName = RegistryName::ocg_const("stone");
/// Registry name for dirt.
pub const DIRT_BLOCK_NAME: RegistryName = RegistryName::ocg_const("dirt");
/// Registry name for grass.
pub const GRASS_BLOCK_NAME: RegistryName = RegistryName::ocg_const("grass");
/// Registry name for snowy grass.
pub const SNOWY_GRASS_BLOCK_NAME: RegistryName = RegistryName::ocg_const("snowy_grass");
/// Registry name for water.
pub const WATER_BLOCK_NAME: RegistryName = RegistryName::ocg_const("water");
/// Registry name for sand.
pub const SAND_BLOCK_NAME: RegistryName = RegistryName::ocg_const("sand");
/// Registry name for log.
pub const LOG_BLOCK_NAME: RegistryName = RegistryName::ocg_const("log");
/// Registry name for leaves.
pub const LEAVES_BLOCK_NAME: RegistryName = RegistryName::ocg_const("leaves");

/// Installs the base set of blocks into the given block registry.
pub fn setup_basic_blocks(registry: &mut BlockRegistry) {
    registry.push_object(EMPTY_BLOCK.clone()).unwrap();
    registry
        .push_object(BlockDefinition {
            name: STONE_BLOCK_NAME,
            shape_set: StandardShapedMaterial,
            representative_color: RGBA8::new(64, 64, 64, 255),
            has_collision_box: true,
            has_drawable_mesh: true,
        })
        .unwrap();
    registry
        .push_object(BlockDefinition {
            name: DIRT_BLOCK_NAME,
            shape_set: StandardShapedMaterial,
            representative_color: RGBA8::new(110, 81, 0, 255),
            has_collision_box: true,
            has_drawable_mesh: true,
        })
        .unwrap();
    registry
        .push_object(BlockDefinition {
            name: GRASS_BLOCK_NAME,
            shape_set: StandardShapedMaterial,
            representative_color: RGBA8::new(30, 230, 30, 255),
            has_collision_box: true,
            has_drawable_mesh: true,
        })
        .unwrap();
    registry
        .push_object(BlockDefinition {
            name: SNOWY_GRASS_BLOCK_NAME,
            shape_set: StandardShapedMaterial,
            representative_color: RGBA8::new(200, 200, 200, 255),
            has_collision_box: true,
            has_drawable_mesh: true,
        })
        .unwrap();
    registry
        .push_object(BlockDefinition {
            name: WATER_BLOCK_NAME,
            shape_set: StandardShapedMaterial,
            representative_color: RGBA8::new(0, 0, 200, 100),
            has_collision_box: false,
            has_drawable_mesh: true,
        })
        .unwrap();
    registry
        .push_object(BlockDefinition {
            name: SAND_BLOCK_NAME,
            shape_set: StandardShapedMaterial,
            representative_color: RGBA8::new(224, 200, 130, 255),
            has_collision_box: true,
            has_drawable_mesh: true,
        })
        .unwrap();
    registry
        .push_object(BlockDefinition {
            name: LOG_BLOCK_NAME,
            shape_set: StandardShapedMaterial,
            representative_color: RGBA8::new(110, 65, 10, 255),
            has_collision_box: true,
            has_drawable_mesh: true,
        })
        .unwrap();
    registry
        .push_object(BlockDefinition {
            name: LEAVES_BLOCK_NAME,
            shape_set: StandardShapedMaterial,
            representative_color: RGBA8::new(24, 110, 21, 255),
            has_collision_box: true,
            has_drawable_mesh: true,
        })
        .unwrap();
}
