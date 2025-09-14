//! All Biome-related types

use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};

use bevy_color::Srgba;
use bevy_math::DVec2;
use noise::OpenSimplex;
use serde::{Deserialize, Serialize};

use super::{
    generation::Context,
    voxeltypes::{BlockEntry, BlockRegistry},
};
use crate::voxel::generation::fbm_noise::Fbm;
use crate::{
    range::Range,
    registry::{Registry, RegistryId, RegistryName, RegistryObject},
};

pub mod biome_map;

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
    pub fn lookup<'r>(&self, registry: &'r BiomeRegistry) -> Option<&'r BiomeDefinition> {
        registry.lookup_id_to_object(self.id)
    }
}

/// A block placement function.
pub type BlockRuleSourceFunction = fn(pos: &bevy_math::IVec3, ctx: &Context, registry: &BlockRegistry) -> Option<BlockEntry>;
/// A surface noise function.
/// Return
pub type SurfaceNoiseFunction = fn(pos: DVec2, noise: &Fbm<OpenSimplex>) -> f64;

/// A named registry of biome definitions.
pub type BiomeRegistry = Registry<BiomeDefinition>;

/// A definition of a biome type, specifying properties such as registry name, shape, textures.
#[derive(Clone, Serialize, Deserialize)]
pub struct BiomeDefinition {
    /// The unique registry name
    pub name: RegistryName,
    /// A color that can represent the biome on maps, debug views, etc.
    pub representative_color: Srgba,
    /// Can this biome generate in the world?
    pub can_generate: bool,
    /// Elevation of this biome.
    pub elevation: Range<f64>,
    /// Temperature of this biome.
    pub temperature: Range<f64>,
    /// Moisture of this biome.
    pub moisture: Range<f64>,
    /// The block placement rule source for this biome.
    #[serde(default = "get_empty_rule_source", skip)]
    pub rule_source: BlockRuleSourceFunction,
    /// The noise function for this biome.
    #[serde(default = "get_empty_surface_noise", skip)]
    pub surface_noise: SurfaceNoiseFunction,
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
        self.name.hash(state);
    }
}

impl RegistryObject for BiomeDefinition {
    fn registry_name(&self) -> crate::registry::RegistryNameRef {
        self.name.as_ref()
    }
}

/// Different noise layers for biome generation.
#[derive(Clone)]
pub struct Noises {
    /// Base noise from which all other noises are derived from
    pub base_terrain_noise: Fbm<OpenSimplex>,
    /// Height noise (0~5)
    pub elevation_noise: Fbm<OpenSimplex>,
    /// Temperature noise (0~5)
    pub temperature_noise: Fbm<OpenSimplex>,
    /// Moisture noise (0~5)
    pub moisture_noise: Fbm<OpenSimplex>,
    /// "Weird" noise (-1.5~1.5)
    /// use for seemingly random values that need to be deterministic, e.g. decorators
    pub weird_noise: Fbm<OpenSimplex>,
}

/// Name of the default void biome.
pub const VOID_BIOME_NAME: RegistryName = RegistryName::gs_const("void");
/// No-op rule source
pub const EMPTY_RULE_SOURCE: BlockRuleSourceFunction = |_pos, _ctx, _reg| None;
/// Empty surface noise function
pub const EMPTY_SURFACE_NOISE: SurfaceNoiseFunction = |_pos, _noise| 0.0;

const fn get_empty_rule_source() -> BlockRuleSourceFunction {
    EMPTY_RULE_SOURCE
}

const fn get_empty_surface_noise() -> SurfaceNoiseFunction {
    EMPTY_SURFACE_NOISE
}
