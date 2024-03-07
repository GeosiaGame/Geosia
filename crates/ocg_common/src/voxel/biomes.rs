//! The builtin biome types.
//! Most of this will be moved to a "base" mod at some point in the future.

use ocg_schemas::{
    dependencies::rgb::RGBA8,
    range::range,
    registry::RegistryName,
    voxel::{
        biome::{BiomeDefinition, BiomeRegistry, VOID_BIOME_NAME},
        generation::{Context, Noise4DTo2D},
        voxeltypes::{BlockEntry, BlockRegistry},
    },
};

use super::blocks::{
    DIRT_BLOCK_NAME, GRASS_BLOCK_NAME, SAND_BLOCK_NAME, SNOWY_GRASS_BLOCK_NAME, STONE_BLOCK_NAME, WATER_BLOCK_NAME,
};

/// Registry name for plains.
pub const PLAINS_BIOME_NAME: RegistryName = RegistryName::ocg_const("plains");
/// Registry name for ocean.
pub const OCEAN_BIOME_NAME: RegistryName = RegistryName::ocg_const("ocean");
/// Registry name for hills.
pub const HILLS_BIOME_NAME: RegistryName = RegistryName::ocg_const("hills");
/// Registry name for mountains.
pub const MOUNTAINS_BIOME_NAME: RegistryName = RegistryName::ocg_const("mountains");
/// Registry name for beach.
pub const BEACH_BIOME_NAME: RegistryName = RegistryName::ocg_const("beach");
/// Registry name for river.
pub const RIVER_BIOME_NAME: RegistryName = RegistryName::ocg_const("river");

/// Installs the base set of biomes into the given block registry.
pub fn setup_basic_biomes(biome_registry: &mut BiomeRegistry) {
    biome_registry
        .push_object(BiomeDefinition {
            name: VOID_BIOME_NAME,
            representative_color: RGBA8::new(0, 0, 0, 0),
            elevation: range(-1.0..-1.0),
            temperature: range(-1.0..-1.0),
            moisture: range(-1.0..-1.0),
            rule_source: |_pos: &bevy_math::IVec3, _ctx: &Context, _reg: &BlockRegistry| None,
            surface_noise: |_point, _noise| 0.0,
            blend_influence: 0.0,
            block_influence: 0.0,
            can_generate: false,
        })
        .unwrap();

    biome_registry
        .push_object(BiomeDefinition {
            name: PLAINS_BIOME_NAME,
            representative_color: RGBA8::new(20, 180, 10, 255),
            elevation: range(0.5..1.0),
            temperature: range(..),
            moisture: range(..2.5),
            rule_source: |pos: &bevy_math::IVec3, context: &Context, block_registry: &BlockRegistry| {
                let (i_grass, _) = block_registry.lookup_name_to_object(GRASS_BLOCK_NAME.as_ref()).unwrap();
                let (i_dirt, _) = block_registry.lookup_name_to_object(DIRT_BLOCK_NAME.as_ref()).unwrap();
                let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
                let (i_snow_grass, _) = block_registry
                    .lookup_name_to_object(SNOWY_GRASS_BLOCK_NAME.as_ref())
                    .unwrap();

                if context.ground_y == pos.y {
                    if pos.y >= 80 {
                        return Some(BlockEntry::new(i_snow_grass, 0));
                    } else {
                        return Some(BlockEntry::new(i_grass, 0));
                    }
                } else if pos.y <= context.ground_y && pos.y > context.ground_y - 5 {
                    return Some(BlockEntry::new(i_dirt, 0));
                } else if context.ground_y > pos.y {
                    return Some(BlockEntry::new(i_stone, 0));
                }
                None
            },
            surface_noise: |point, noise| {
                let new_point = point * 1.5;

                let mut value = noise.get_2d(new_point.to_array()) * 0.75;
                value += noise.get_2d((new_point * 2.0).to_array()) * 0.25;
                value *= 5.0;
                value += 10.0;
                value
            },
            blend_influence: 0.5,
            block_influence: 1.0,
            can_generate: true,
        })
        .unwrap();

    biome_registry
        .push_object(BiomeDefinition {
            name: HILLS_BIOME_NAME,
            representative_color: RGBA8::new(15, 110, 10, 255),
            elevation: range(1.0..2.0),
            temperature: range(..),
            moisture: range(..2.5),
            rule_source: |pos: &bevy_math::IVec3, context: &Context, block_registry: &BlockRegistry| {
                let (i_grass, _) = block_registry.lookup_name_to_object(GRASS_BLOCK_NAME.as_ref()).unwrap();
                let (i_dirt, _) = block_registry.lookup_name_to_object(DIRT_BLOCK_NAME.as_ref()).unwrap();
                let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
                let (i_snow_grass, _) = block_registry
                    .lookup_name_to_object(SNOWY_GRASS_BLOCK_NAME.as_ref())
                    .unwrap();

                if context.ground_y == pos.y {
                    if pos.y >= 80 {
                        return Some(BlockEntry::new(i_snow_grass, 0));
                    } else {
                        return Some(BlockEntry::new(i_grass, 0));
                    }
                } else if pos.y <= context.ground_y && pos.y > context.ground_y - 5 {
                    return Some(BlockEntry::new(i_dirt, 0));
                } else if context.ground_y > pos.y {
                    return Some(BlockEntry::new(i_stone, 0));
                }
                None
            },
            surface_noise: |point, noise| {
                let new_point = point / 3.0;
                let new_point_arr = new_point.to_array();

                let mut value = noise.get_2d(new_point_arr) * 0.6;
                value += noise.get_2d((new_point * 1.5).to_array()) * 0.25;
                value += noise.get_2d((new_point * 3.0).to_array()) * 0.15;
                value *= 8.0;
                value += 15.0;
                value
            },
            blend_influence: 1.0,
            block_influence: 1.0,
            can_generate: true,
        })
        .unwrap();

    biome_registry
        .push_object(BiomeDefinition {
            name: MOUNTAINS_BIOME_NAME,
            representative_color: RGBA8::new(220, 220, 220, 255),
            elevation: range(3.0..),
            temperature: range(..),
            moisture: range(..2.5),
            rule_source: |pos: &bevy_math::IVec3, context: &Context, block_registry: &BlockRegistry| {
                let (i_grass, _) = block_registry.lookup_name_to_object(GRASS_BLOCK_NAME.as_ref()).unwrap();
                let (i_dirt, _) = block_registry.lookup_name_to_object(DIRT_BLOCK_NAME.as_ref()).unwrap();
                let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
                let (i_snow_grass, _) = block_registry
                    .lookup_name_to_object(SNOWY_GRASS_BLOCK_NAME.as_ref())
                    .unwrap();

                if context.ground_y == pos.y {
                    if pos.y >= 80 {
                        return Some(BlockEntry::new(i_snow_grass, 0));
                    } else {
                        return Some(BlockEntry::new(i_grass, 0));
                    }
                } else if context.ground_y >= pos.y && pos.y > context.ground_y - 5 {
                    return Some(BlockEntry::new(i_dirt, 0));
                } else if context.ground_y > pos.y {
                    return Some(BlockEntry::new(i_stone, 0));
                }
                None
            },
            surface_noise: |point, noise| {
                let new_point = point / 4.0;
                let new_point_arr = new_point.to_array();
                let h_n = |p| (noise.get_2d(p) + 1.0) / 2.0;
                let h_rn = |p| (0.5 - (0.5 - h_n(p)).abs()) * 2.0;

                let h0 = 0.50 * h_rn(new_point_arr);
                let h01 = 0.25 * h_rn((new_point * 2.0).to_array()) + h0;

                (h01 + (h01 / 0.75) * 0.15 * h_n((new_point * 5.0).to_array())
                    + (h01 / 0.75) * 0.05 * h_rn((new_point * 9.0).to_array()))
                .abs()
                    * 100.0
                    + 40.0
            },
            blend_influence: 1.0,
            block_influence: 1.0,
            can_generate: true,
        })
        .unwrap();

    biome_registry
        .push_object(BiomeDefinition {
            name: OCEAN_BIOME_NAME,
            representative_color: RGBA8::new(10, 120, 180, 255),
            elevation: range(..1.0),
            temperature: range(..),
            moisture: range(2.5..),
            rule_source: |pos: &bevy_math::IVec3, context: &Context, block_registry: &BlockRegistry| {
                let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
                let (i_water, _) = block_registry.lookup_name_to_object(WATER_BLOCK_NAME.as_ref()).unwrap();

                if context.sea_level > pos.y {
                    return if context.ground_y > pos.y {
                        Some(BlockEntry::new(i_stone, 0))
                    } else {
                        Some(BlockEntry::new(i_water, 0))
                    };
                }
                None
            },
            surface_noise: |point, noise| noise.get_2d(point.to_array()) * -7.5 + 1.0,
            blend_influence: 10.0,
            block_influence: 1.0,
            can_generate: true,
        })
        .unwrap();

    biome_registry
        .push_object(BiomeDefinition {
            name: BEACH_BIOME_NAME,
            representative_color: RGBA8::new(224, 200, 130, 255),
            elevation: range(..),
            temperature: range(..),
            moisture: range(2.5..),
            rule_source: |pos: &bevy_math::IVec3, context: &Context, block_registry: &BlockRegistry| {
                let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
                let (i_sand, _) = block_registry.lookup_name_to_object(SAND_BLOCK_NAME.as_ref()).unwrap();

                if context.ground_y > pos.y - 1 {
                    return Some(BlockEntry::new(i_stone, 0));
                } else if context.ground_y == pos.y {
                    return Some(BlockEntry::new(i_sand, 0));
                }
                None
            },
            surface_noise: |point, noise| noise.get_2d(point.to_array()) * 1.0 + 1.0,
            blend_influence: 1.0,
            block_influence: 1.0,
            can_generate: false,
        })
        .unwrap();

    biome_registry
        .push_object(BiomeDefinition {
            name: RIVER_BIOME_NAME,
            representative_color: RGBA8::new(10, 100, 200, 255),
            elevation: range(..),
            temperature: range(..),
            moisture: range(..),
            rule_source: |pos: &bevy_math::IVec3, context: &Context, block_registry: &BlockRegistry| {
                let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
                let (i_sand, _) = block_registry.lookup_name_to_object(SAND_BLOCK_NAME.as_ref()).unwrap();
                let (i_water, _) = block_registry.lookup_name_to_object(WATER_BLOCK_NAME.as_ref()).unwrap();

                if context.ground_y == pos.y {
                    return Some(BlockEntry::new(i_sand, 0));
                } else if pos.y <= context.ground_y && context.ground_y > pos.y - 3 {
                    return Some(BlockEntry::new(i_water, 0));
                } else if context.ground_y > pos.y {
                    return Some(BlockEntry::new(i_stone, 0));
                }
                None
            },
            surface_noise: |point, noise| noise.get_2d(point.to_array()) * -1.5 + 1.0,
            blend_influence: 1.0,
            block_influence: 1.0,
            can_generate: false,
        })
        .unwrap();
}
