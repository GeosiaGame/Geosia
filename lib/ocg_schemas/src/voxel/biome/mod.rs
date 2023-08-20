//! All Biome-related types

use std::fmt::{Debug, Display};

use lazy_static::lazy_static;
use dyn_clone::DynClone;
use noise::{NoiseFn, Constant, SuperSimplex, Perlin, Multiply, Add, Max, Min, Power, Seedable};
use rgb::RGBA8;
use serde::{Serialize, Deserialize};

use crate::{registry::{Registry, RegistryName, RegistryObject, RegistryId}, voxel::generation::rule_sources::EmptyRuleSource, range::{Range, range}};

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

/// A named registry of block definitions.
pub type BiomeRegistry = Registry<BiomeDefinition>;

/// A definition of a biome type, specifying properties such as registry name, shape, textures.
#[derive(Clone)]
// TODO fix serialization of `BiomeDefinition`
pub struct BiomeDefinition {
    /// The unique registry name
    pub name: RegistryName,
    /// A color that can represent the biome on maps, debug views, etc.
    pub representative_color: RGBA8,
    /// Elevation of this biome.
    pub elevation: Range<f64>,
    /// Temperature of this biome.
    pub temperature: Range<f64>,
    /// Moisture of this biome.
    pub moisture: Range<f64>,
    /// The block placement rule source for this biome.
    pub rule_source: Box<RuleSrc>,
    /// The noise function for this biome.
    pub surface_noise: Box<NoiseFn2>,
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

impl RegistryObject for BiomeDefinition {
    fn registry_name(&self) -> crate::registry::RegistryNameRef {
        self.name.as_ref()
    }
}

impl BiomeDefinition {}

/// God save my soul from the hell that is Rust generic types.
/// You NEED to use this type alias everywhere where one is required, by the way. FUN.
pub type RuleSrc = dyn RuleSource;
/// Holy shit another one
pub type ConditionSrc = dyn ConditionSource;
/// WHERE DO THESE KEEP APPEARING FROM
pub type NoiseFn2 = dyn NoiseFn2Trait;

/// Helper trait for NoiseFn<f64, 2> + required extras
pub trait NoiseFn2Trait: NoiseFn<f64, 2> + SeedableGetter + DynClone + Sync + Send {}
dyn_clone::clone_trait_object!(NoiseFn2Trait);
#[allow(trivial_bounds)]
impl Default for dyn NoiseFn2Trait where (dyn NoiseFn2Trait): Default + Sized {
    fn default() -> Self {
        Self::default()
    }
}

/// A getter for `Seedable` for setting a new seed for a trait NoiseFn.
pub trait SeedableGetter: Sync + Send {
    /// Gets a boxed `SeedableWrapper` from this object, if it's `Seedable`.
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        None
    }
}

/// Wraps a `Seedable` in a trait object-safe way.
pub trait SeedableWrapper: Sync + Send {
    /// Set the seed for the function implementing the `Seedable` trait
    fn set_seed(self: &mut Self, seed: u32) -> Box<dyn SeedableWrapper>;

    /// Getter to retrieve the seed from the function
    fn seed(self: &Self) -> u32;
}

#[allow(trivial_bounds)]
impl Seedable for (dyn SeedableWrapper) where (dyn SeedableWrapper): Sized {
    fn set_seed(mut self, seed: u32) -> Self {
        *SeedableWrapper::set_seed(&mut self, seed)
    }

    fn seed(&self) -> u32 {
        SeedableWrapper::seed(self)
    }
}

/// Different noise layers for biome generation.
pub struct Noises {
    /// Height noise (0~5)
    pub elevation_noise: Box<NoiseFn2>, 
    /// Temperature noise (0~5)
    pub temperature_noise: Box<NoiseFn2>, 
    /// Moisture noise (0~5)
    pub moisture_noise: Box<NoiseFn2>,
}

/// Name of the default-er plains biome.
pub const PLAINS_BIOME_NAME: RegistryName = RegistryName::ocg_const("plains");
/// Name of the default void biome.
pub const VOID_BIOME_NAME: RegistryName = RegistryName::ocg_const("void");

lazy_static! {
    /// Registration for said biome.
    pub static ref VOID_BIOME: BiomeDefinition = BiomeDefinition {
        name: VOID_BIOME_NAME,
        representative_color: RGBA8::new(0, 0, 0, 0),
        elevation: range(-1.0..-1.0),
        temperature: range(-1.0..-1.0),
        moisture: range(-1.0..-1.0),
        rule_source: Box::new(EmptyRuleSource()),
        surface_noise: Box::new(Constant::new(0.0)),
        blend_influence: 0.0,
        block_influence: 0.0,
    };
}

impl NoiseFn2Trait for Constant {}
impl SeedableGetter for Constant {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        None
    }
}
impl<T> NoiseFn2Trait for Fbm<T> where T: NoiseFn<f64, 2> + Clone + Send + Sync + Default + Seedable + 'static {}
impl<'a, T> SeedableWrapper for Fbm<T> where T: NoiseFn<f64, 2> + Clone + Send + Sync + Default + Seedable + 'static {
    fn set_seed(&mut self, seed: u32) -> Box<dyn SeedableWrapper> {
        let result = Seedable::set_seed(self.to_owned(), seed);
        Box::new(result)
    }

    fn seed(self: &Self) -> u32 {
        Seedable::seed(self)
    }
}
impl<T> SeedableGetter for Fbm<T> where T: NoiseFn<f64, 2> + Clone + Send + Sync + Default + Seedable + 'static {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        Some(Box::new(self.to_owned()))
    }
}
impl NoiseFn2Trait for SuperSimplex {}
impl SeedableWrapper for SuperSimplex {
    fn set_seed(&mut self, seed: u32) -> Box<dyn SeedableWrapper> {
        Box::new(Seedable::set_seed(*self, seed))
    }

    fn seed(self: &Self) -> u32 {
        Seedable::seed(self)
    }
}
impl SeedableGetter for SuperSimplex {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        Some(Box::new(*self))
    }
}
impl NoiseFn2Trait for Perlin {}
impl SeedableWrapper for Perlin {
    fn set_seed(&mut self, seed: u32) -> Box<dyn SeedableWrapper> {
        Box::new(Seedable::set_seed(*self, seed))
    }

    fn seed(self: &Self) -> u32 {
        Seedable::seed(self)
    }
}
impl SeedableGetter for Perlin {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        Some(Box::new(*self))
    }
}

/// newtype wrapper
pub struct Mul2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub Multiply<f64, S1, S2, 2>);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Mul2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point)
    }
}
impl<S1, S2> NoiseFn2Trait for Mul2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {}
impl<S1, S2> SeedableGetter for Mul2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone  {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        None
    }
}
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
impl<S1, S2> SeedableGetter for Add2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone  {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        None
    }
}
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
impl<S1, S2> SeedableGetter for Max2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone  {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        None
    }
}
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
impl<S1, S2> SeedableGetter for Min2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone  {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        None
    }
}
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
impl<S1, S2> SeedableGetter for Pow2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone  {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        None
    }
}
impl<S1, S2> Clone for Pow2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(Power::new(self.0.source1.clone(), self.0.source2.clone()))
    }
}
