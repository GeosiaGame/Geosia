//! All Biome-related types

use std::fmt::Debug;

use dyn_clone::DynClone;
use noise::{NoiseFn, SuperSimplex, Perlin, Constant};
use rgb::RGBA8;
use serde::{Serialize, Deserialize};

use crate::registry::{Registry, RegistryName, RegistryObject, RegistryId};

use super::generation::{RuleSource, ConditionSource, fbm_noise::Fbm};


pub mod biome_map;
pub mod biome_picker;

/// A biome entry stored in the per-planet biome map.
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[repr(C)]
pub struct BiomeEntry {
    /// The biome ID in registry.
    pub id: RegistryId,
}

impl BiomeEntry {
    /// Helper to construct a new biome entry.
    pub fn new(id: RegistryId) -> Self {
        Self {
            id: id,
        }
    }

    /// Helper to look up the biome definition corresponding to this ID
    pub fn lookup(self, registry: &BiomeRegistry) -> Option<&BiomeDefinition> {
        registry.lookup_id_to_object(self.id)
    }
}


/// God save my soul from the hell that is Rust generic types.
/// You NEED to use this type alias everywhere where one is required, by the way. FUN.
pub type RuleSrc = dyn RuleSource;
/// Holy shit another one
pub type ConditionSrc = dyn ConditionSource;
/// WHERE DO THESE KEEP APPEARING FROM
pub type NoiseFn2 = dyn NoiseFn2Trait;

/// Helper trait for NoiseFn<f64, 2> + required extras
pub trait NoiseFn2Trait: NoiseFn<f64, 2> + DynClone + Debug + Sync + Send {}
dyn_clone::clone_trait_object!(NoiseFn2Trait);


/// A named registry of block definitions.
pub type BiomeRegistry = Registry<BiomeDefinition>;

/// A definition of a biome type, specifying properties such as registry name, shape, textures.
#[derive(Clone, Debug)]
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
    pub rule_source: Box<RuleSrc>,
    /// The noise function for this biome.
    pub surface_noise: Box<NoiseFn2>,
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

/// The registry name of [`EMPTY_BIOME`]
pub const EMPTY_BIOME_NAME: RegistryName = RegistryName::ocg_const("empty");

impl NoiseFn2Trait for Constant {}
impl NoiseFn2Trait for Fbm<SuperSimplex> {}
impl NoiseFn2Trait for SuperSimplex {}
impl NoiseFn2Trait for Perlin {}
