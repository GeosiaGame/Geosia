//! World generation related methods.

use std::fmt::Debug;

use bevy_math::IVec3;
use hashbrown::HashMap;
use noise::{NoiseFn, Seedable};
use serde::{Serializer, Deserializer, Serialize, Deserialize};

use crate::{voxel::{biome::biome_picker::BiomeGenerator, chunk::Chunk}, registry::RegistryName};

use self::positional_random::PositionalRandomFactory;

use super::voxeltypes::{BlockEntry, BlockRegistry};

pub mod fbm_noise;
pub mod positional_random;
pub mod rule_sources;
pub mod condition_sources;

/// Worldgen Chunk type.
pub type GenerationChunk = Chunk<GenerationChunkData>;

/// Context data for world generation.
pub struct Context {
    biome_generator: BiomeGenerator,
    chunk: GenerationChunk,
    random: PositionalRandomFactory<rand_xoshiro::Xoshiro512StarStar>,
}

/// Block placement rule source.
pub trait RuleSource: Sync + Debug {
    /// Placement function
    fn place(self: &mut Self, pos: &IVec3, context: &Context, block_registry: &BlockRegistry) -> Option<BlockEntry>;
}

/// Block placement condition. Used for testing if a certain position is valid etc.
pub trait ConditionSource: Sync + Debug {
    /// Wether a block is valid.
    fn test(self: &mut Self, pos: IVec3, context: &Context) -> bool;
}

/// Worldgen-only per-chunk data storage
#[derive(Clone, Default)]
pub struct GenerationChunkData {
    //
}

/// Manager for different noise functions.
pub struct NoiseManager {
    // just add a `noise_2d` etc. when need be.
    noise_3d: HashMap<RegistryName, &'static dyn NoiseFn<f64, 3>>,
}

impl NoiseManager {
    /// Gets a 3D noise from the noise manager.
    pub fn get_noise_3d(&self, id: &RegistryName) -> &'_ dyn NoiseFn<f64, 3> {
        self.noise_3d.get(id).unwrap()
    }

    /// Adds a noise to the noise manager. IF it already contains one with the same id, the old one is removed.
    pub fn add_noise_3d(&mut self, id: RegistryName, noise: &'static dyn NoiseFn<f64, 3>) {
        self.noise_3d.insert(id, noise);
    }
}

fn build_sources<Source>(seed: u32, octaves: &Vec<f64>) -> Vec<Source>
where
    Source: Default + Seedable,
{
    let mut sources = Vec::with_capacity(octaves.len());
    for x in 0..octaves.len() {
        let source = Source::default();
        sources.push(source.set_seed(seed + (octaves[x] * 100.0) as u32));
    }
    sources
}

/// Fake trait for adding Copy to RuleSource
pub trait RuleSourceClone {
    /// dumb helper function
    fn serialize_trait<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer, Self: Sized;
    fn deserialize_trait<'de, D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de>, Self: Sized;
    fn default_trait() -> Self where Self: Sized;
}

impl<T> RuleSourceClone for T where T: 'static + RuleSource + Clone + Default + Serialize + for<'a> Deserialize<'a> {
    fn serialize_trait<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer, Self: Sized {
        self.serialize_trait(serializer)
    }

    fn deserialize_trait<'de, D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de>, Self: Sized {
        Self::deserialize(deserializer)
    }

    fn default_trait() -> Self where Self: Sized {
        Self::default()
    }
}

impl Serialize for (dyn RuleSource) where (dyn RuleSource): Sized + Clone + Default {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
        self.serialize_trait(serializer)
    }
}

impl<'de> Deserialize<'de> for (dyn RuleSource) where (dyn RuleSource): Sized + Clone + Default {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de> {
        Self::deserialize_trait(deserializer)
    }
}
