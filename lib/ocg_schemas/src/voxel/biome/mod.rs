//! All Biome-related types

use std::{fmt::Debug, rc::Rc};

use lazy_static::lazy_static;
use dyn_clone::DynClone;
use noise::{NoiseFn, Constant, SuperSimplex, Perlin, Multiply, Add, Max, Min, Power};
use rgb::RGBA8;
use serde::{Serialize, Deserialize};
use smallvec::{SmallVec, smallvec};

use crate::{registry::{Registry, RegistryName, RegistryObject, RegistryId}, voxel::generation::rule_sources::EmptyRuleSource, coordinates::{CHUNK_DIM2Z, CHUNK_DIMZ}};

use super::generation::{RuleSource, ConditionSource, fbm_noise::Fbm};


pub mod biome_map;
pub mod biome_picker;

/// A biome entry stored in the per-planet biome map.
#[derive(Clone, Debug, PartialOrd, PartialEq, Serialize, Deserialize)]
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
        Self {
            id: id,
            weight: 0.0,
        }
    }

    /// Helper to look up the biome definition corresponding to this ID
    pub fn lookup<'a>(&'a self, registry: &'a BiomeRegistry) -> Option<&BiomeDefinition> {
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
pub trait NoiseFn2Trait: NoiseFn<f64, 2> + DynClone + Sync + Send {}
dyn_clone::clone_trait_object!(NoiseFn2Trait);
impl Default for dyn NoiseFn2Trait where (dyn NoiseFn2Trait): Default + Sized {
    fn default() -> Self {
        Self::default()
    }
}

/// A named registry of block definitions.
pub type BiomeRegistry = Registry<BiomeDefinition>;

/// A definition of a biome type, specifying properties such as registry name, shape, textures.
#[derive(Clone)]
pub struct BiomeDefinition {
    /// The unique registry name
    pub name: RegistryName,
    /// A color that can represent the biome on maps, debug views, etc.
    pub representative_color: RGBA8,
    /// Size of this biome, in blocks.
    pub size_chunks: u32,
    /// Elevation of this biome.
    pub elevation: f64,
    /// Temperature of this biome.
    pub temperature: f64,
    /// Moisture of this biome.
    pub moisture: f64,
    /// The block placement rule source for this biome.
    pub rule_source: Box<RuleSrc>,
    /// The noise function for this biome.
    pub surface_noise: Box<NoiseFn2>,
    /// The strength of this biome in the blending step.
    pub influence: f64,
}

impl PartialEq for BiomeDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl BiomeDefinition {}

impl RegistryObject for BiomeDefinition {
    fn registry_name(&self) -> crate::registry::RegistryNameRef {
        self.name.as_ref()
    }
}

/// Height variance "areas".
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum VPElevation {
    Underground(f64),
    Ocean(f64),
    LowLand(f64),
    Hill(f64),
    Mountain(f64),
    Sky(f64),
}

/// Temperature variance "areas".
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum VPTemperature {
    Freezing(f64),
    LowTemp(f64),
    MedTemp(f64),
    HiTemp(f64),
    Desert(f64),
}

/// Moisture variation "areas".
#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum VPMoisture {
    Deadland(f64),
    Desert(f64),
    LowMoist(f64),
    MedMoist(f64),
    HiMoist(f64),
}

impl Default for VPElevation {
    fn default() -> Self {
        VPElevation::LowLand(0.4)
    }
}

impl Default for VPMoisture {
    fn default() -> Self {
        VPMoisture::MedMoist(0.8)
    }
}

impl Default for VPTemperature {
    fn default() -> Self {
        VPTemperature::MedTemp(0.4)
    }
}

/// Different noise layers for biome generation.
pub struct Noises {
    /// Height noise
    pub elevation_noise: Box<dyn NoiseFn<f64, 2> + Send + Sync>, 
    /// Temperature noise
    pub temperature_noise: Box<dyn NoiseFn<f64, 2> + Send + Sync>, 
    /// Moisture noise
    pub moisture_noise: Box<dyn NoiseFn<f64, 2> + Send + Sync>,
}

/// Name of the default void biome.
pub const VOID_BIOME_NAME: RegistryName = RegistryName::ocg_const("void");

lazy_static! {
    /// Registration for said biome.
    pub static ref VOID_BIOME: BiomeDefinition = BiomeDefinition {
        name: VOID_BIOME_NAME,
        representative_color: RGBA8::new(0, 0, 0, 0),
        size_chunks: 0,
        elevation: -1.0,
        temperature: -1.0,
        moisture: -1.0,
        rule_source: Box::new(EmptyRuleSource()),
        surface_noise: Box::new(Constant::new(0.0)),
        influence: 0.0,
    };
}

impl NoiseFn2Trait for Constant {}
impl<T> NoiseFn2Trait for Fbm<T> where T: NoiseFn<f64, 2> + Clone + Send + Sync {}
impl NoiseFn2Trait for SuperSimplex {}
impl NoiseFn2Trait for Perlin {}

/// newtype wrapper
pub struct Mul2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub Multiply<f64, S1, S2, 2>);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Mul2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point)
    }
}
impl<S1, S2> NoiseFn2Trait for Mul2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {}
impl<S1, S2> Clone for Mul2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(Multiply::new(self.0.source1.clone(), self.0.source2.clone()))
    }
}
/// newtype wrapper
pub struct Add2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub Add<f64, S1, S2, 2>);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Add2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point)
    }
}
impl<S1, S2> NoiseFn2Trait for Add2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {}
impl<S1, S2> Clone for Add2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(Add::new(self.0.source1.clone(), self.0.source2.clone()))
    }
}
/// newtype wrapper
pub struct Max2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub Max<f64, S1, S2, 2>);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Max2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point)
    }
}
impl<S1, S2> NoiseFn2Trait for Max2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {}
impl<S1, S2> Clone for Max2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(Max::new(self.0.source1.clone(), self.0.source2.clone()))
    }
}
/// newtype wrapper
pub struct Min2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub Min<f64, S1, S2, 2>);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Min2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point)
    }
}
impl<S1, S2> NoiseFn2Trait for Min2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {}
impl<S1, S2> Clone for Min2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(Min::new(self.0.source1.clone(), self.0.source2.clone()))
    }
}
/// newtype wrapper
pub struct Pow2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub Power<f64, S1, S2, 2>);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Pow2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point)
    }
}
impl<S1, S2> NoiseFn2Trait for Pow2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {}
impl<S1, S2> Clone for Pow2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(Power::new(self.0.source1.clone(), self.0.source2.clone()))
    }
}
