//! The builtin biome types.
//! Most of this will be moved to a "base" mod at some point in the future.

use noise::{SuperSimplex, NoiseFn};
use ocg_schemas::{voxel::{biome::{BiomeRegistry, BiomeDefinition, Mul2, Add2, PLAINS_BIOME_NAME, NoiseOffsetMul2, Subs2, Abs2, biome_map::GLOBAL_SCALE_MOD, SurfaceGenerator, SeedableGetter}, generation::{rule_sources::{ChainRuleSource, ConditionRuleSource, BlockRuleSource}, fbm_noise::Fbm, condition_sources::{YLevelCondition, OffsetGroundLevelCondition, GroundLevelCondition, UnderGroundLevelCondition, UnderSeaLevelCondition}}, voxeltypes::{BlockRegistry, BlockEntry}}, registry::RegistryName, dependencies::rgb::RGBA8, range::range};

use super::blocks::{SNOWY_GRASS_BLOCK_NAME, DIRT_BLOCK_NAME, GRASS_BLOCK_NAME, STONE_BLOCK_NAME, WATER_BLOCK_NAME};


/// Registry name for ocean.
pub const OCEAN_BIOME_NAME: RegistryName = RegistryName::ocg_const("ocean");
/// Registry name for hills.
pub const HILLS_BIOME_NAME: RegistryName = RegistryName::ocg_const("hills");
/// Registry name for mountains.
pub const MOUNTAINS_BIOME_NAME: RegistryName = RegistryName::ocg_const("mountains");

pub fn setup_basic_biomes(block_registry: &BlockRegistry, biome_registry: &mut BiomeRegistry) {
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
    ]);

    let noise_func = Fbm::<SuperSimplex>::new(0);
    let noise_func = Box::leak(Box::new(noise_func.set_octaves(vec![1.0, 1.0, 1.0, 1.0])));
    biome_registry
        .push_object(BiomeDefinition {
            name: PLAINS_BIOME_NAME,
            representative_color: RGBA8::new(20, 180, 10, 255),
            elevation: range(1.0..2.5),
            temperature: range(..),
            moisture: range(..2.5),
            rule_source: plains_rule_source.clone(),
            surface_noise: SurfaceGenerator::new(Box::leak(Box::new(|point: [f64; 2], seed: u32| {
                let noise_fn = noise_func.clone();
                noise_fn.get_seedable().unwrap().set_seed(seed);
                let new_point = [point[0] / GLOBAL_SCALE_MOD * 2.0, point[1] / GLOBAL_SCALE_MOD * 2.0];

                let mut value = noise_fn.get(new_point) * 0.75;
                value += noise_fn.get([new_point[0] * 2.0, new_point[1] * 2.0]) * 0.25;
                value *= 5.0;
                return value;
            }))),
            /*
            surface_noise: Box::new(NoiseOffsetDiv2(
                Mul2(noise::Multiply::new(
                Add2(noise::Add::new(
                    Mul2(noise::Multiply::new(noise_func.clone(), noise::Constant::new(0.75))),
                    Mul2(noise::Multiply::new(NoiseOffsetMul2(noise_func.clone(), 2.0), noise::Constant::new(0.25)))
                    )), 
                noise::Constant::new(5.0)
                )),
                GLOBAL_SCALE_MOD * 2.0
            )),
            */
            blend_influence: 0.5,
            block_influence: 1.0,
            can_generate: true,
        })
        .unwrap();

    let noise_func = Fbm::<SuperSimplex>::new(2);
    let noise_func = Box::leak(Box::new(noise_func.set_octaves(vec![1.0, 1.0, 1.0, 1.0]).set_persistence(0.75)));
    biome_registry
        .push_object(BiomeDefinition {
            name: HILLS_BIOME_NAME,
            representative_color: RGBA8::new(15, 110, 10, 255),
            elevation: range(2.5..3.5),
            temperature: range(..),
            moisture: range(..2.5),
            rule_source: plains_rule_source.clone(),
            surface_noise: SurfaceGenerator::new(Box::leak(Box::new(|point: [f64; 2], seed: u32| {
                let noise_fn = noise_func.clone();
                noise_fn.get_seedable().unwrap().set_seed(seed);

                let new_point = [point[0] / GLOBAL_SCALE_MOD * 40.0, point[1] / GLOBAL_SCALE_MOD * 40.0];

                let mut value = noise_fn.get(new_point) * 0.6;
                value += noise_fn.get([new_point[0] * 1.5, new_point[1] * 1.5]) * 0.25;
                value += noise_fn.get([new_point[0] * 3.0, new_point[1] * 3.0]) * 0.15;
                value *= 0.05;
                return value;
            }))),
            /*
            surface_noise: Box::new(NoiseOffsetDiv2(
                Add2(noise::Add::new(
                Mul2(noise::Multiply::new(
                    Add2(noise::Add::new(
                        Add2(noise::Add::new(
                            Mul2(noise::Multiply::new(noise_func.clone(), noise::Constant::new(0.6))),
                            Mul2(noise::Multiply::new(NoiseOffsetMul2(noise_func.clone(), 1.5), noise::Constant::new(0.25)))
                            )),
                            Mul2(noise::Multiply::new(NoiseOffsetMul2(noise_func.clone(), 3.0), noise::Constant::new(0.15)))
                        )),
                        noise::Constant::new(0.25)
                    )),
                    noise::Constant::new(60.0)
                )),
                GLOBAL_SCALE_MOD * 80.0
            )),
            */
            blend_influence: 1.0,
            block_influence: 1.0,
            can_generate: true,
        })
        .unwrap();

    let noise_func = Fbm::<SuperSimplex>::new(3);
    let noise_func = noise_func.set_octaves(vec![1.0, 1.5, 1.0, 1.5]).set_persistence(0.75);
    let ridge_noise_func = Box::leak(Box::new(Mul2(noise::Multiply::new(
        Subs2(
            noise::Constant::new(0.5),
            Abs2(noise::Abs::new(
                Subs2(
                    noise::Constant::new(0.5),
                    noise_func.clone()
                )
            ))
        ),
        noise::Constant::new(2.0)
    ))));
    let ridge_noise_func_2 = Box::leak(Box::new(Add2(noise::Add::new(
        Mul2(noise::Multiply::new(
            noise::Constant::new(0.5),
            ridge_noise_func.clone()
        )),
        NoiseOffsetMul2(
            Mul2(noise::Multiply::new(
                noise::Constant::new(0.25),
                ridge_noise_func.clone()
            )),
            2.0
        )
    ))));
    biome_registry
        .push_object(BiomeDefinition {
            name: MOUNTAINS_BIOME_NAME,
            representative_color: RGBA8::new(220, 220, 220, 255),
            elevation: range(3.5..),
            temperature: range(../*=3.0*/),
            moisture: range(..2.5),
            rule_source: plains_rule_source.clone(),
            surface_noise: SurfaceGenerator::new(Box::leak(Box::new(|point: [f64; 2], seed: u32| {
                let noise_fn = ridge_noise_func.clone();
                noise_fn.get_seedable().unwrap().set_seed(seed);
                let noise_fn_2 = ridge_noise_func_2.clone();
                noise_fn_2.get_seedable().unwrap().set_seed(seed);
                
                let new_point = [point[0] / GLOBAL_SCALE_MOD * 16.0, point[1] / GLOBAL_SCALE_MOD * 16.0];
                let val = noise_fn_2.get(new_point) / 0.75;
                let mut value = val * 0.15 * noise_fn.get([new_point[0] * 5.0, new_point[1] * 5.0]);
                value += noise_fn_2.get(new_point);

                value += val + 0.05 * noise_fn.get([new_point[0] * 9.0, new_point[1] * 9.0]);
                value *= 15.0;
                return value;
            }))),
            /*
            surface_noise: Box::new(NoiseOffsetDiv2(
                Add2(noise::Add::new(
                Mul2(noise::Multiply::new(
                    Add2(noise::Add::new(
                        Add2(noise::Add::new(
                            ridge_noise_func_2.clone(),
                            Mul2(noise::Multiply::new(
                                Div2(
                                        ridge_noise_func_2.clone(),
                                        noise::Constant::new(0.75)
                                    ),
                                Mul2(noise::Multiply::new(
                                    noise::Constant::new(0.15),
                                    NoiseOffsetMul2(noise_func.clone(), 5.0)
                                    ))
                                ))
                            )),
                        Add2(noise::Add::new(
                                Div2(
                                    ridge_noise_func_2.clone(),
                                    noise::Constant::new(0.75)
                                ),
                                Mul2(noise::Multiply::new(
                                    noise::Constant::new(0.05),
                                    NoiseOffsetMul2(ridge_noise_func.clone(), 9.0)
                                ))
                            ))
                        )),
                    noise::Constant::new(150.0)
                    )),
                noise::Constant::new(40.0)
                )),
                GLOBAL_SCALE_MOD * 160.0
            )),
            */
            blend_influence: 1.0,
            block_influence: 1.0,
            can_generate: true,
        })
        .unwrap();

    let noise_func = Fbm::<SuperSimplex>::new(1);
    let noise_func = Box::leak(Box::new(noise_func.set_octaves(vec![-2.0, 1.0])));
    biome_registry
        .push_object(BiomeDefinition {
            name: OCEAN_BIOME_NAME,
            representative_color: RGBA8::new(10, 120, 180, 255),
            elevation: range(..1.0),
            temperature: range(..),
            moisture: range(2.5..),
            rule_source: ConditionRuleSource::new_boxed(UnderSeaLevelCondition::new_boxed(), 
                ChainRuleSource::new_boxed(vec![
                    ConditionRuleSource::new_boxed(under_surface.clone(), BlockRuleSource::new_boxed(BlockEntry::new(i_stone, 0))),
                    BlockRuleSource::new_boxed(BlockEntry::new(i_water, 0)),
            ])),
            surface_noise: SurfaceGenerator::new(Box::leak(Box::new(|point: [f64; 2], seed: u32| {
                let noise_fn = noise_func.clone();
                noise_fn.get_seedable().unwrap().set_seed(seed);

                let new_point = [point[0] / GLOBAL_SCALE_MOD * 1.0, point[1] / GLOBAL_SCALE_MOD * 1.0];

                let mut value = noise_fn.get(new_point) * -7.5;
                value += 1.0;
                return value;
            }))),
            /*
            surface_noise: Box::new(
                NoiseOffsetDiv2(
                    Add2(noise::Add::new(Mul2(noise::Multiply::new(noise_func, noise::Constant::new(-7.5))), noise::Constant::new(0.0))),
                    GLOBAL_SCALE_MOD * 100.0
                )
            ),
            */
            blend_influence: 1.0,
            block_influence: 1.0,
            can_generate: true,
        })
        .unwrap();
}