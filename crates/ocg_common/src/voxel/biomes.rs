//! The builtin biome types.
//! Most of this will be moved to a "base" mod at some point in the future.

use noise::SuperSimplex;
use ocg_schemas::{voxel::{biome::{BiomeRegistry, BiomeDefinition, Mul2, Add2, PLAINS_BIOME_NAME}, generation::{rule_sources::{ChainRuleSource, ConditionRuleSource, BlockRuleSource}, fbm_noise::Fbm, condition_sources::{YLevelCondition, OffsetGroundLevelCondition, GroundLevelCondition, UnderGroundLevelCondition, UnderSeaLevelCondition, AlwaysTrueCondition}}, voxeltypes::{BlockRegistry, BlockEntry, EMPTY_BLOCK_NAME}}, registry::RegistryName, dependencies::rgb::RGBA8, range::range};

use super::blocks::{SNOWY_GRASS_BLOCK_NAME, DIRT_BLOCK_NAME, GRASS_BLOCK_NAME, STONE_BLOCK_NAME, WATER_BLOCK_NAME};


/// Registry name for ocean.
pub const OCEAN_BIOME_NAME: RegistryName = RegistryName::ocg_const("ocean");
/// Registry name for hills.
pub const HILLS_BIOME_NAME: RegistryName = RegistryName::ocg_const("hills");
/// Registry name for mountains.
pub const MOUNTAINS_BIOME_NAME: RegistryName = RegistryName::ocg_const("mountains");

pub fn setup_basic_biomes(block_registry: &BlockRegistry, biome_registry: &mut BiomeRegistry) {
    let (i_air, _) = block_registry.lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref()).unwrap();
    let (i_grass, _) = block_registry.lookup_name_to_object(GRASS_BLOCK_NAME.as_ref()).unwrap();
    let (i_dirt, _) = block_registry.lookup_name_to_object(DIRT_BLOCK_NAME.as_ref()).unwrap();
    let (i_stone, _) = block_registry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
    let (i_snow_grass, _) = block_registry.lookup_name_to_object(SNOWY_GRASS_BLOCK_NAME.as_ref()).unwrap();
    let (i_water, _) = block_registry.lookup_name_to_object(WATER_BLOCK_NAME.as_ref()).unwrap();

    let under_surface_5 = OffsetGroundLevelCondition::new_boxed(5);
    let on_surface = GroundLevelCondition::new_boxed();
    let under_surface = UnderGroundLevelCondition::new_boxed();

    //biome_registry.push_object(VOID_BIOME.clone()).unwrap();

    let plains_rule_source = ChainRuleSource::new_boxed(vec![
        ConditionRuleSource::new_boxed(on_surface, 
            ChainRuleSource::new_boxed(vec![
                    ConditionRuleSource::new_boxed(YLevelCondition::new_boxed(80), BlockRuleSource::new_boxed(BlockEntry::new(i_snow_grass, 0))),
                    BlockRuleSource::new_boxed(BlockEntry::new(i_grass, 0)),
                ])
            ),
        ConditionRuleSource::new_boxed(under_surface_5, 
            BlockRuleSource::new_boxed(BlockEntry::new(i_dirt, 0))
        ),
        ConditionRuleSource::new_boxed(under_surface.clone(), 
            BlockRuleSource::new_boxed(BlockEntry::new(i_stone, 0))
        ),
        BlockRuleSource::new_boxed(BlockEntry::new(i_air, 0)),
    ]);

    let noise_func = Fbm::<SuperSimplex>::new(0);
    let noise_func = noise_func.set_octaves(vec![1.0, 2.0, 2.0, 1.0]);
    biome_registry
        .push_object(BiomeDefinition {
            name: PLAINS_BIOME_NAME,
            representative_color: RGBA8::new(20, 180, 10, 255),
            size_chunks: 6,
            elevation: range(0.4..),
            temperature: range(0.4..),
            moisture: range(0.8..),
            rule_source: plains_rule_source.clone(),
            surface_noise: Box::new(noise_func.clone()),
            influence: 1.0,
        })
        .unwrap();

    let noise_func = Fbm::<SuperSimplex>::new(1);
    let noise_func = noise_func.set_octaves(vec![0.1, 0.5, 0.0, 1.5]);
    biome_registry
        .push_object(BiomeDefinition {
            name: OCEAN_BIOME_NAME,
            representative_color: RGBA8::new(10, 120, 180, 255),
            size_chunks: 6,
            elevation: range(0.0..),
            temperature: range(0.4..),
            moisture: range(0.8..),
            rule_source: ChainRuleSource::new_boxed(vec![
                ConditionRuleSource::new_boxed(under_surface.clone(), BlockRuleSource::new_boxed(BlockEntry::new(i_stone, 0))),
                ConditionRuleSource::new_boxed(UnderSeaLevelCondition::new_boxed(), BlockRuleSource::new_boxed(BlockEntry::new(i_water, 0)))
            ]),
            surface_noise: Box::new(Add2(noise::Add::new(noise_func.clone(), noise::Constant {value: -10.0}))),
            influence: 1.0,
        })
        .unwrap();

    let noise_func = Fbm::<SuperSimplex>::new(2);
    let noise_func = noise_func.set_octaves(vec![1.0, 2.5, 2.5, 1.5]).set_persistence(0.75);
    biome_registry
        .push_object(BiomeDefinition {
            name: HILLS_BIOME_NAME,
            representative_color: RGBA8::new(15, 110, 10, 255),
            size_chunks: 5,
            elevation: range(0.2..),
            temperature: range(0.05..),
            moisture: range(0.2..),
            rule_source: plains_rule_source.clone(),
            surface_noise: Box::new(Mul2(noise::Multiply::new(noise_func.clone(), noise::Constant { value: 10.0 }))),
            influence: 0.85,
        })
        .unwrap();

    let noise_func = Fbm::<SuperSimplex>::new(3);
    let noise_func = noise_func.set_octaves(vec![1.0, 4.5, 2.0, 1.5]).set_persistence(0.9);
    biome_registry
        .push_object(BiomeDefinition {
            name: MOUNTAINS_BIOME_NAME,
            representative_color: RGBA8::new(220, 220, 220, 255),
            size_chunks: 4,
            elevation: range(0.4..),
            temperature: range(0.0..),
            moisture: range(0.1..),
            rule_source: ChainRuleSource::new_boxed(vec![
                ConditionRuleSource::new_boxed(AlwaysTrueCondition::new_boxed() /*NotCondition::new_boxed(UnderSeaLevelCondition::new_boxed())*/, plains_rule_source.clone()),
                //BlockRuleSource::new_boxed(BlockEntry::new(i_stone, 0)),
            ]),
            surface_noise: Box::new(Add2(noise::Add::new(Mul2(noise::Multiply::new(noise_func.clone(), noise::Constant::new(25.0))), noise::Constant::new(40.0)))),
            influence: 1.5,
        })
        .unwrap();
}