//! All Biome-related types

use std::fmt::{Debug, Display};

use lazy_static::lazy_static;
use dyn_clone::DynClone;
use noise::{NoiseFn, Constant, SuperSimplex, Perlin, Seedable};
use rgb::RGBA8;
use serde::{Serialize, Deserialize};

use crate::{registry::{Registry, RegistryName, RegistryObject, RegistryId}, range::{Range, range}};

use super::{generation::{fbm_noise::Fbm, Context}, voxeltypes::{BlockRegistry, BlockEntry}};


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
// TODO fix serialization of `BiomeDefinition`
#[derive(Clone)]
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
    pub rule_source: Box<dyn BlockPlacer>,
    /// The noise function for this biome.
    pub surface_noise: Box<dyn HeightGenerator>,
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

/// A block placer (wrapper).
pub trait BlockPlacer: Sync + Send {
    /// Clone this in a box.
    fn box_clone(&self) -> Box<dyn BlockPlacer>;
    /// Call the inner function.
    fn call(&self, pos: &bevy_math::IVec3, ctx: &Context, registry: &BlockRegistry) -> Option<BlockEntry>;
}
impl Clone for Box<dyn BlockPlacer> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}
impl BlockPlacer for Box<dyn BlockPlacer> {
    fn box_clone(&self) -> Box<dyn BlockPlacer> {
        (**self).box_clone()
    }

    fn call(&self, pos: &bevy_math::IVec3, ctx: &Context, registry: &BlockRegistry) -> Option<BlockEntry> {
        (**self).call(pos, ctx, registry)
    }
}
impl<F: Fn(&bevy_math::IVec3, &Context, &BlockRegistry) -> Option<BlockEntry> + Clone + Sync + Send + 'static> BlockPlacer for F {
    fn box_clone(&self) -> Box<dyn BlockPlacer> {
        Box::new(self.clone())
    }
    fn call(&self, pos: &bevy_math::IVec3, ctx: &Context, registry: &BlockRegistry) -> Option<BlockEntry> {
        self(pos, ctx, registry)
    }
}


/// A surface noise generator (wrapper)
pub trait HeightGenerator: Sync + Send {
    /// Clone this in a box.
    fn box_clone(&self) -> Box<dyn HeightGenerator>;
    /// Call the inner function.
    fn call(&self, pos: [f64; 2], seed: u32) -> f64;
}
impl Clone for Box<dyn HeightGenerator> {
    fn clone(&self) -> Self {
        (**self).box_clone()
    }
}
impl HeightGenerator for Box<dyn HeightGenerator> {
    fn box_clone(&self) -> Box<dyn HeightGenerator> {
        (**self).box_clone()
    }

    fn call(&self, pos: [f64; 2], seed: u32) -> f64 {
        (**self).call(pos, seed)
    }
}
impl<F: Fn([f64; 2], u32) -> f64 + Clone + Sync + Send + 'static> HeightGenerator for F {
    fn box_clone(&self) -> Box<dyn HeightGenerator> {
        Box::new(self.clone())
    }
    fn call(&self, pos: [f64; 2], seed: u32) -> f64 {
        self(pos, seed)
    }
}

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
    pub elevation_noise: Box<dyn NoiseFn2Trait>, 
    /// Temperature noise (0~5)
    pub temperature_noise: Box<dyn NoiseFn2Trait>, 
    /// Moisture noise (0~5)
    pub moisture_noise: Box<dyn NoiseFn2Trait>,
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
        rule_source: Box::new(|_pos: &bevy_math::IVec3, _ctx: &Context, _reg: &BlockRegistry| None),
        surface_noise: Box::new(|_point, _seed| 0.0),
        blend_influence: 0.0,
        block_influence: 0.0,
        can_generate: false,
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

/// Divides the position of the noise by the 2nd value.
pub struct NoiseOffsetDiv2<S1: NoiseFn<f64, 2>>(pub S1, pub f64);
impl<S1: NoiseFn<f64, 2>> NoiseFn<f64, 2> for NoiseOffsetDiv2<S1> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get([point[0] / self.1, point[1] / self.1])
    }
}
impl<S1> NoiseFn2Trait for NoiseOffsetDiv2<S1> where S1: NoiseFn2Trait + Send + Sync + Clone {}
impl<S1> SeedableGetter for NoiseOffsetDiv2<S1> where S1: NoiseFn2Trait + Send + Sync + Clone {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        None
    }
}
impl<S1> Clone for NoiseOffsetDiv2<S1> where S1: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}
/// Multiplies the position of the noise by the 2nd value.
pub struct NoiseOffsetMul2<S1: NoiseFn<f64, 2>>(pub S1, pub f64);
impl<S1: NoiseFn<f64, 2>> NoiseFn<f64, 2> for NoiseOffsetMul2<S1> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get([point[0] * self.1, point[1] * self.1])
    }
}
impl<S1> NoiseFn2Trait for NoiseOffsetMul2<S1> where S1: NoiseFn2Trait + Send + Sync + Clone {}
impl<S1> SeedableGetter for NoiseOffsetMul2<S1> where S1: NoiseFn2Trait + Send + Sync + Clone {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        None
    }
}
impl<S1> Clone for NoiseOffsetMul2<S1> where S1: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}

/// newtype wrapper
pub struct Mul2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub S1, pub S2);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Mul2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point) * self.1.get(point)
    }
}
impl<S1, S2> NoiseFn2Trait for Mul2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {}
impl<S1, S2> SeedableGetter for Mul2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        Some(Box::new(self.to_owned()))
    }
}
impl<S1, S2> SeedableWrapper for Mul2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn set_seed(&mut self, seed: u32) -> Box<dyn SeedableWrapper> {
        if let Some(seedable) = self.0.get_seedable().as_mut() {
            seedable.set_seed(seed);
        }
        if let Some(seedable) = self.1.get_seedable().as_mut() {
            seedable.set_seed(seed.wrapping_add(1));
        }
        Box::new(self.to_owned())
    }

    fn seed(self: &Self) -> u32 {
        let mut seed = 0;
        if let Some(seedable) = self.0.get_seedable().as_ref() {
            seed = seedable.seed();
        }
        seed
    }
}
impl<S1, S2> Clone for Mul2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}
/// newtype wrapper
pub struct Add2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub S1, pub S2);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Add2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point) + self.1.get(point)
    }
}
impl<S1, S2> NoiseFn2Trait for Add2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {}
impl<S1, S2> SeedableGetter for Add2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        Some(Box::new(self.to_owned()))
    }
}
impl<S1, S2> SeedableWrapper for Add2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn set_seed(&mut self, seed: u32) -> Box<dyn SeedableWrapper> {
        if let Some(seedable) = self.0.get_seedable().as_mut() {
            seedable.set_seed(seed);
        }
        if let Some(seedable) = self.1.get_seedable().as_mut() {
            seedable.set_seed(seed.wrapping_add(1));
        }
        Box::new(self.to_owned())
    }

    fn seed(self: &Self) -> u32 {
        let mut seed = 0;
        if let Some(seedable) = self.0.get_seedable().as_ref() {
            seed = seedable.seed();
        }
        seed
    }
}
impl<S1, S2> Clone for Add2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}
/// newtype wrapper
pub struct Subs2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub S1, pub S2);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Subs2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point) - self.1.get(point)
    }
}
impl<S1, S2> NoiseFn2Trait for Subs2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {}
impl<S1, S2> SeedableGetter for Subs2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        Some(Box::new(self.to_owned()))
    }
}
impl<S1, S2> SeedableWrapper for Subs2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn set_seed(&mut self, seed: u32) -> Box<dyn SeedableWrapper> {
        if let Some(seedable) = self.0.get_seedable().as_mut() {
            seedable.set_seed(seed);
        }
        if let Some(seedable) = self.1.get_seedable().as_mut() {
            seedable.set_seed(seed.wrapping_add(1));
        }
        Box::new(self.to_owned())
    }

    fn seed(self: &Self) -> u32 {
        let mut seed = 0;
        if let Some(seedable) = self.0.get_seedable().as_ref() {
            seed = seedable.seed();
        }
        seed
    }
}
impl<S1, S2> Clone for Subs2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}
/// newtype wrapper
pub struct Div2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub S1, pub S2);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Div2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point) / self.1.get(point)
    }
}
impl<S1, S2> NoiseFn2Trait for Div2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {}
impl<S1, S2> SeedableGetter for Div2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        Some(Box::new(self.to_owned()))
    }
}
impl<S1, S2> SeedableWrapper for Div2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn set_seed(&mut self, seed: u32) -> Box<dyn SeedableWrapper> {
        if let Some(seedable) = self.0.get_seedable().as_mut() {
            seedable.set_seed(seed);
        }
        if let Some(seedable) = self.1.get_seedable().as_mut() {
            seedable.set_seed(seed.wrapping_add(1));
        }
        Box::new(self.to_owned())
    }

    fn seed(self: &Self) -> u32 {
        let mut seed = 0;
        if let Some(seedable) = self.0.get_seedable().as_ref() {
            seed = seedable.seed();
        }
        seed
    }
}
impl<S1, S2> Clone for Div2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}
/// newtype wrapper
pub struct Max2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub S1, pub S2);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Max2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point).max(self.1.get(point))
    }
}
impl<S1, S2> NoiseFn2Trait for Max2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {}
impl<S1, S2> SeedableGetter for Max2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        Some(Box::new(self.to_owned()))
    }
}
impl<S1, S2> SeedableWrapper for Max2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn set_seed(&mut self, seed: u32) -> Box<dyn SeedableWrapper> {
        if let Some(seedable) = self.0.get_seedable().as_mut() {
            seedable.set_seed(seed);
        }
        if let Some(seedable) = self.1.get_seedable().as_mut() {
            seedable.set_seed(seed.wrapping_add(1));
        }
        Box::new(self.to_owned())
    }

    fn seed(self: &Self) -> u32 {
        let mut seed = 0;
        if let Some(seedable) = self.0.get_seedable().as_ref() {
            seed = seedable.seed();
        }
        seed
    }
}
impl<S1, S2> Clone for Max2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}
/// newtype wrapper
pub struct Min2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub S1, pub S2);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Min2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point).min(self.1.get(point))
    }
}
impl<S1, S2> NoiseFn2Trait for Min2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {}
impl<S1, S2> SeedableGetter for Min2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        Some(Box::new(self.to_owned()))
    }
}
impl<S1, S2> SeedableWrapper for Min2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn set_seed(&mut self, seed: u32) -> Box<dyn SeedableWrapper> {
        if let Some(seedable) = self.0.get_seedable().as_mut() {
            seedable.set_seed(seed);
        }
        if let Some(seedable) = self.1.get_seedable().as_mut() {
            seedable.set_seed(seed.wrapping_add(1));
        }
        Box::new(self.to_owned())
    }

    fn seed(self: &Self) -> u32 {
        let mut seed = 0;
        if let Some(seedable) = self.0.get_seedable().as_ref() {
            seed = seedable.seed();
        }
        seed
    }
}
impl<S1, S2> Clone for Min2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}
/// newtype wrapper
pub struct Pow2<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>>(pub S1, pub S2);
impl<S1: NoiseFn<f64, 2>, S2: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Pow2<S1, S2> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point).powf(self.1.get(point))
    }
}
impl<S1, S2> NoiseFn2Trait for Pow2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {}
impl<S1, S2> SeedableGetter for Pow2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        Some(Box::new(self.to_owned()))
    }
}
impl<S1, S2> SeedableWrapper for Pow2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static, S2: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn set_seed(&mut self, seed: u32) -> Box<dyn SeedableWrapper> {
        if let Some(seedable) = self.0.get_seedable().as_mut() {
            seedable.set_seed(seed);
        }
        if let Some(seedable) = self.1.get_seedable().as_mut() {
            seedable.set_seed(seed.wrapping_add(1));
        }
        Box::new(self.to_owned())
    }

    fn seed(self: &Self) -> u32 {
        let mut seed = 0;
        if let Some(seedable) = self.0.get_seedable().as_ref() {
            seed = seedable.seed();
        }
        seed
    }
}
impl<S1, S2> Clone for Pow2<S1, S2> where S1: NoiseFn2Trait + Send + Sync + Clone, S2: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}
/// Newtype wrapper
pub struct Abs2<S1: NoiseFn<f64, 2>>(pub S1);
impl<S1: NoiseFn<f64, 2>> NoiseFn<f64, 2> for Abs2<S1> {
    fn get(&self, point: [f64; 2]) -> f64 {
        self.0.get(point).abs()
    }
}
impl<S1> NoiseFn2Trait for Abs2<S1> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static {}
impl<S1> SeedableGetter for Abs2<S1> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn get_seedable(self: &Self) -> Option<Box<dyn SeedableWrapper>> {
        None
    }
}
impl<S1> SeedableWrapper for Abs2<S1> where S1: NoiseFn2Trait + Send + Sync + Clone + 'static {
    fn set_seed(self: &mut Self, seed: u32) -> Box<dyn SeedableWrapper> {
        if let Some(seedable) = self.0.get_seedable().as_mut() {
            seedable.set_seed(seed);
        }
        Box::new(self.to_owned())
    }

    fn seed(self: &Self) -> u32 {
        let mut seed = 0;
        if let Some(seedable) = self.0.get_seedable().as_ref() {
            seed = seedable.seed();
        }
        seed
    }
}
impl<S1> Clone for Abs2<S1> where S1: NoiseFn2Trait + Send + Sync + Clone {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}