//! All Biome-related types

use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};

use bevy_math::DVec2;
use noise::NoiseFn;
use rgb::RGBA8;
use serde::{Deserialize, Serialize};

use super::{
    generation::Context,
    voxeltypes::{BlockEntry, BlockRegistry},
};
use crate::{
    range::Range,
    registry::{Registry, RegistryId, RegistryName, RegistryObject},
};

pub mod biome_map;
pub mod decorator;

/// A biome entry stored in the per-planet biome map.
#[derive(Clone, Copy, Debug, PartialOrd, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct BiomeEntry {
    /// The biome ID in registry.
    pub id: RegistryId,
    /// Weight map
    pub weight: f64,
}

impl BiomeEntry {
    /// Helper to construct a new biome entry.
    pub fn new(id: RegistryId) -> Self {
        Self { id, weight: 0.0 }
    }

    /// Helper to look up the biome definition corresponding to this ID
    pub fn lookup<'a>(&'a self, registry: &'a BiomeRegistry) -> Option<&BiomeDefinition> {
        registry.lookup_id_to_object(self.id)
    }
}

/// A named registry of block definitions.
pub type BiomeRegistry = Registry<BiomeDefinition>;

/// A definition of a biome type, specifying properties such as registry name, shape, textures.
// TODO fix serialization of `BiomeDefinition`
#[derive(Clone, Serialize, Deserialize)]
pub struct BiomeDefinition {
    /// The unique registry name
    pub name: RegistryName,
    /// A color that can represent the biome on maps, debug views, etc.
    pub representative_color: RGBA8,
    /// Can this biome generate in the world?
    pub can_generate: bool,
    /// Elevation of this biome.
    pub elevation: Range<f64>,
    /// Temperature of this biome.
    pub temperature: Range<f64>,
    /// Moisture of this biome.
    pub moisture: Range<f64>,
    /// The block placement rule source for this biome.
    #[serde(skip)]
    pub rule_source: Option<fn(pos: &bevy_math::IVec3, ctx: &Context, registry: &BlockRegistry) -> Option<BlockEntry>>,
    /// The noise function for this biome.
    #[serde(skip)]
    pub surface_noise: Option<fn(pos: DVec2, noise: &mut Box<dyn NoiseFn<f64, 4>>) -> f64>,
    /// The strength of this biome in the blending step.
    pub blend_influence: f64,
    /// The strength of this biome in the block placement step.
    pub block_influence: f64,
}

impl Debug for BiomeDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BiomeDefinition").field("id", &self.name).finish()
    }
}

impl Display for BiomeDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BiomeDefinition").field("id", &self.name).finish()
    }
}

impl PartialEq for BiomeDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Hash for BiomeDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

impl RegistryObject for BiomeDefinition {
    fn registry_name(&self) -> crate::registry::RegistryNameRef {
        self.name.as_ref()
    }
}

/// Different noise layers for biome generation.
pub struct Noises {
    /// Base noise from which all other noises are derived from
    pub base_terrain_noise: Box<dyn NoiseFn<f64, 4>>,
    /// Height noise (0~5)
    pub elevation_noise: Box<dyn NoiseFn<f64, 4>>,
    /// Temperature noise (0~5)
    pub temperature_noise: Box<dyn NoiseFn<f64, 4>>,
    /// Moisture noise (0~5)
    pub moisture_noise: Box<dyn NoiseFn<f64, 4>>,
}

/// Name of the default void biome.
pub const VOID_BIOME_NAME: RegistryName = RegistryName::ocg_const("void");
