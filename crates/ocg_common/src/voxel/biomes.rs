//! The builtin biome types.
//! Most of this will be moved to a "base" mod at some point in the future.

use ocg_schemas::{voxel::biome::{BiomeRegistry, EMPTY_BIOME, BiomeDefinition, VPElevation, VPTemperature, VPMoisture}, registry::RegistryName, dependencies::rgb::RGBA8};


/// Registry name for plains.
pub const PLAINS_BIOME_NAME: RegistryName = RegistryName::ocg_const("plains");
/// Registry name for ocean.
pub const OCEAN_BIOME_NAME: RegistryName = RegistryName::ocg_const("ocean");
/// Registry name for hills.
pub const HILLS_BIOME_NAME: RegistryName = RegistryName::ocg_const("hills");
/// Registry name for mountains.
pub const MOUNTAINS_BIOME_NAME: RegistryName = RegistryName::ocg_const("mountains");

pub fn setup_basic_biomes(registry: &mut BiomeRegistry) {
    registry.push_object(EMPTY_BIOME.clone()).unwrap();
    registry
        .push_object(BiomeDefinition {
            name: PLAINS_BIOME_NAME,
            representative_color: RGBA8::new(20, 180, 10, 255),
            size_chunks: 4,
            elevation: VPElevation::LowLand,
            temperature: VPTemperature::MedTemp,
            moisture: VPMoisture::MedMoist,
            //rule_source: 
        })
        .unwrap();
    registry
        .push_object(BiomeDefinition {
            name: OCEAN_BIOME_NAME,
            representative_color: RGBA8::new(10, 120, 180, 255),
            size_chunks: 6,
            elevation: VPElevation::Ocean,
            temperature: VPTemperature::MedTemp,
            moisture: VPMoisture::HiMoist,
        })
        .unwrap();
    registry
        .push_object(BiomeDefinition {
            name: HILLS_BIOME_NAME,
            representative_color: RGBA8::new(15, 110, 10, 255),
            size_chunks: 3,
            elevation: VPElevation::Hill,
            temperature: VPTemperature::MedTemp,
            moisture: VPMoisture::MedMoist,
        })
        .unwrap();
    registry
        .push_object(BiomeDefinition {
            name: MOUNTAINS_BIOME_NAME,
            representative_color: RGBA8::new(220, 220, 220, 255),
            size_chunks: 2,
            elevation: VPElevation::Mountain,
            temperature: VPTemperature::Freezing,
            moisture: VPMoisture::LowMoist,
        })
        .unwrap();
}