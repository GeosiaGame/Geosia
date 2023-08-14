//! All Biome-related types

use std::fmt::Debug;

use noise::NoiseFn;
use rgb::RGBA8;
use serde::{Serialize, Deserialize};

use crate::registry::{Registry, RegistryName, RegistryObject, RegistryId};

use super::{voxeltypes::BlockEntry, generation::{RuleSource, ConditionSource, rule_sources::EmptyRuleSource}};


pub mod biome_map;
pub mod biome_picker;

/// A biome entry stored in the per-planet biome map.
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[repr(C)]
pub struct BiomeEntry {
    /// The biome ID in registry.
    pub id: RegistryId,
}

/// God save my soul from the hell that is Rust generic types.
/// You NEED to use this type alias everywhere where one is required, by the way. FUN.
pub type RuleSrc = dyn RuleSource;
/// Holy shit another one
pub type ConditionSrc = dyn ConditionSource;
pub type NoiseFn2 = dyn NoiseFn<f64, 2> + Sync;

/// A named registry of block definitions.
pub type BiomeRegistry = Registry<BiomeDefinition>;

impl BiomeEntry {
    /// Helper to construct a new biome entry.
    pub fn new(id: RegistryId) -> Self {
        Self {
            id: id,
        }
    }
}

impl Debug for BiomeEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BiomeEntry").field("id", &self.id).finish()
    }
}

/// A definition of a biome type, specifying properties such as registry name, shape, textures.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BiomeDefinition {
    /// The unique registry name
    pub name: RegistryName,
    /// A color that can represent the biome on maps, debug views, etc.
    pub representative_color: RGBA8,
    /// Size of this biome, in blocks.
    pub size_chunks: u32,
    /// Elevation of this biome.
    pub elevation: VPElevation,
    /// Temperature of this biome.
    pub temperature: VPTemperature,
    /// Moisture of this biome.
    pub moisture: VPMoisture,
    /// The block placement rule source for this biome.
    #[serde(skip)]
    pub rule_source: &'static RuleSrc,
    #[serde(skip)]
    pub surface_noise: &'static NoiseFn2,
}

impl BiomeDefinition {}

impl RegistryObject for BiomeDefinition {
    fn registry_name(&self) -> crate::registry::RegistryNameRef {
        self.name.as_ref()
    }
}

/// Height variance "areas".
#[derive(Copy, Clone, Hash, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum VPElevation {
    Underground,
    Ocean,
    LowLand,
    Hill,
    Mountain,
    Sky,
}

/// Temperature variance "areas".
#[derive(Copy, Clone, Hash, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum VPTemperature {
    Freezing,
    LowTemp,
    MedTemp,
    HiTemp,
    Desert,
}

/// Moisture variation "areas".
#[derive(Copy, Clone, Hash, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum VPMoisture {
    Deadland,
    Desert,
    LowMoist,
    MedMoist,
    HiMoist,
}

impl Default for VPElevation {
    fn default() -> Self {
        VPElevation::LowLand
    }
}

impl Default for VPMoisture {
    fn default() -> Self {
        VPMoisture::MedMoist
    }
}

impl Default for VPTemperature {
    fn default() -> Self {
        VPTemperature::MedTemp
    }
}

/// Always-true condition.
#[derive(Clone, Serialize, Deserialize)]
pub struct AlwaysTrueCondition();

impl ConditionSource for AlwaysTrueCondition {
    fn test(&mut self, pos: bevy_math::IVec3, context: &super::generation::Context) -> bool {
        true
    }
}

impl Debug for AlwaysTrueCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AlwaysTrueCondition").finish()
    }
}

/// The registry name of [`EMPTY_BIOME`]
pub const EMPTY_BIOME_NAME: RegistryName = RegistryName::ocg_const("empty");

/// The empty biome definition, used when no specific biomes have been generated
pub static EMPTY_BIOME: BiomeDefinition = BiomeDefinition {
    name: EMPTY_BIOME_NAME,
    representative_color: RGBA8::new(0, 0, 0, 0),
    size_chunks: 0,
    elevation: VPElevation::LowLand,
    temperature: VPTemperature::MedTemp,
    moisture: VPMoisture::MedMoist,
    rule_source: &EmptyRuleSource(),
    surface_noise: &noise::Constant {
        value: 0.0
    },
};