//! World generation related methods.

use std::fmt::Debug;

use dyn_clone::DynClone;
use bevy_math::IVec3;
use hashbrown::HashMap;
use noise::{NoiseFn, Seedable};

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
    ground_y: i32,
}

/// Block placement rule source.
#[typetag::serde(tag = "rule_source")]
pub trait RuleSource: Sync + Send + Debug + DynClone {
    /// Placement function
    fn place(self: &mut Self, pos: &IVec3, context: &Context, block_registry: &BlockRegistry) -> Option<BlockEntry>;
}

dyn_clone::clone_trait_object!(RuleSource);

/// Block placement condition. Used for testing if a certain position is valid etc.
#[typetag::serde(tag = "condition_source")]
pub trait ConditionSource: Sync + Send + Debug + DynClone {
    /// Wether a block is valid.
    fn test(self: &mut Self, pos: IVec3, context: &Context) -> bool;
}

dyn_clone::clone_trait_object!(ConditionSource);


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
