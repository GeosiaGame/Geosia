//! The builtin biome types.
//! Most of this will be moved to a "base" mod at some point in the future.

use bevy_math::DVec2;
use noise::NoiseFn;
use ocg_schemas::{voxel::{biome::{BiomeRegistry, BiomeDefinition, PLAINS_BIOME_NAME}, generation::Context, voxeltypes::{BlockRegistry, BlockEntry}}, registry::RegistryName, dependencies::rgb::RGBA8, range::range};

use super::blocks::{DIRT_BLOCK_NAME, GRASS_BLOCK_NAME, SAND_BLOCK_NAME, SNOWY_GRASS_BLOCK_NAME, STONE_BLOCK_NAME, WATER_BLOCK_NAME};


/// Registry name for ocean.
pub const OCEAN_BIOME_NAME: RegistryName = RegistryName::ocg_const("ocean");
/// Registry name for hills.
pub const HILLS_BIOME_NAME: RegistryName = RegistryName::ocg_const("hills");
/// Registry name for mountains.
pub const MOUNTAINS_BIOME_NAME: RegistryName = RegistryName::ocg_const("mountains");
pub const BEACH_BIOME_NAME: RegistryName = RegistryName::ocg_const("beach");
pub const RIVER_BIOME_NAME: RegistryName = RegistryName::ocg_const("river");

pub fn setup_basic_biomes(biome_registry: &mut BiomeRegistry) {
    biome_registry
        .push_object(BiomeDefinition {
            name: PLAINS_BIOME_NAME,
            representative_color: RGBA8::new(20, 180, 10, 255),
            elevation: range(1.0..2.5),
            temperature: range(..),
            moisture: range(..2.5),
            rule_source: |pos: &bevy_math::IVec3, context: &Context, block_registry: &BlockRegistry| {
                let (i_grass, _) = block_registry.lookup_name_to_object(GRASS_BLOCK_NAME.as_ref()).unwrap();
                let (i_dirt, _) = block_registry.lookup_name_to_object(DIRT_BLOCK_NAME.as_ref()).unwrap();
                let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
                let (i_snow_grass, _) = block_registry.lookup_name_to_object(SNOWY_GRASS_BLOCK_NAME.as_ref()).unwrap();

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
                return None;
            },
            surface_noise: |point: DVec2, noise: &mut Box<dyn NoiseFn<f64, 2>>| {
                let new_point = point * 1.5;

                let mut value = noise.get(new_point.to_array()) * 0.75;
                value += noise.get((new_point * 2.0).to_array()) * 0.25;
                value *= 5.0;
                return value;
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
            elevation: range(2.5..3.5),
            temperature: range(..),
            moisture: range(..2.5),
            rule_source: |pos: &bevy_math::IVec3, context: &Context, block_registry: &BlockRegistry| {
                let (i_grass, _) = block_registry.lookup_name_to_object(GRASS_BLOCK_NAME.as_ref()).unwrap();
                let (i_dirt, _) = block_registry.lookup_name_to_object(DIRT_BLOCK_NAME.as_ref()).unwrap();
                let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
                let (i_snow_grass, _) = block_registry.lookup_name_to_object(SNOWY_GRASS_BLOCK_NAME.as_ref()).unwrap();

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
                return None;
            },
            surface_noise: |point: DVec2, noise: &mut Box<dyn NoiseFn<f64, 2>>| {
                let new_point = point * 3.0;
                let new_point_arr = new_point.to_array();

                let mut value = noise.get(new_point_arr) * 0.6;
                value += noise.get((new_point * 1.5).to_array()) * 0.25;
                value += noise.get((new_point * 3.0).to_array()) * 0.15;
                value *= 4.0;
                value += 15.0;
                return value;
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
            elevation: range(3.5..),
            temperature: range(..),
            moisture: range(..2.5),
            rule_source: |pos: &bevy_math::IVec3, context: &Context, block_registry: &BlockRegistry| {
                let (i_grass, _) = block_registry.lookup_name_to_object(GRASS_BLOCK_NAME.as_ref()).unwrap();
                let (i_dirt, _) = block_registry.lookup_name_to_object(DIRT_BLOCK_NAME.as_ref()).unwrap();
                let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
                let (i_snow_grass, _) = block_registry.lookup_name_to_object(SNOWY_GRASS_BLOCK_NAME.as_ref()).unwrap();

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
                return None;
            },
            surface_noise: |point: DVec2, noise: &mut Box<dyn NoiseFn<f64, 2>>| {
                let new_point = point / 4.0;
                let new_point_arr = new_point.to_array();
                let h_n = |p| (noise.get(p) + 1.0) / 2.0;
                let h_rn = |p| (0.5 - (0.5 - h_n(p)).abs()) * 2.0;

                let h0 = 0.50 * h_rn(new_point_arr);
                let h01 = 0.25 * h_rn((new_point * 2.0).to_array()) + h0;
        
                (h01 + (h01 / 0.75) * 0.15 * h_n((new_point * 5.0).to_array())
                    + (h01 / 0.75) * 0.05 * h_rn((new_point * 9.0).to_array())).abs()
                    * 100.0
                    + 40.0
            },
            blend_influence: 0.75,
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
                    if context.ground_y > pos.y {
                        return Some(BlockEntry::new(i_stone, 0));
                    } else {
                        return Some(BlockEntry::new(i_water, 0));
                    }
                }
                return None;
            },
            surface_noise: |point: DVec2, noise: &mut Box<dyn NoiseFn<f64, 2>>| {
                noise.get(point.to_array()) * -7.5 + 1.0
            },
            blend_influence: 1.0,
            block_influence: 1.0,
            can_generate: true,
        })
        .unwrap();

        biome_registry
            .push_object(BiomeDefinition {
                name: BEACH_BIOME_NAME,
                representative_color: RGBA8::new(224, 200, 130, 255),
                elevation: range(1.0..1.5),
                temperature: range(..),
                moisture: range(..3.0),
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
                surface_noise: |point: DVec2, noise: &mut Box<dyn NoiseFn<f64, 2>>| {
                    noise.get(point.to_array()) * -7.5 + 1.0
                },
                blend_influence: 1.0,
                block_influence: 1.0,
                can_generate: false,
            })
            .unwrap();

            biome_registry
                .push_object(BiomeDefinition {
                    name: RIVER_BIOME_NAME,
                    representative_color: RGBA8::new(224, 200, 130, 255),
                    elevation: range(..),
                    temperature: range(..),
                    moisture: range(..),
                    rule_source: |pos: &bevy_math::IVec3, context: &Context, block_registry: &BlockRegistry| {
                        let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
                        let (i_sand, _) = block_registry.lookup_name_to_object(SAND_BLOCK_NAME.as_ref()).unwrap();
                        let (i_water, _) = block_registry.lookup_name_to_object(WATER_BLOCK_NAME.as_ref()).unwrap();
        
                        if context.ground_y == pos.y {
                            return Some(BlockEntry::new(i_sand, 0));
                        } else if context.ground_y > pos.y - 3 {
                            return Some(BlockEntry::new(i_stone, 0));
                        } else {
                            return Some(BlockEntry::new(i_water, 0));
                        }
                    },
                    surface_noise: |point: DVec2, noise: &mut Box<dyn NoiseFn<f64, 2>>| {
                        noise.get(point.to_array()) * -1.5 + 1.0
                    },
                    blend_influence: 1.0,
                    block_influence: 1.0,
                    can_generate: false,
                })
                .unwrap();
}