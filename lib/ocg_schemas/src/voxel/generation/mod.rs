//! World generation related methods.

use std::fmt::Debug;

use dyn_clone::DynClone;
use bevy_math::IVec3;
use noise::Seedable;

use crate::voxel::{biome::biome_picker::BiomeGenerator, chunk::Chunk};

use self::positional_random::PositionalRandomFactory;

use super::{voxeltypes::{BlockEntry, BlockRegistry}, chunk_storage::PaletteStorage};

pub mod fbm_noise;
pub mod positional_random;
pub mod rule_sources;
pub mod condition_sources;

/// Worldgen Chunk type.
pub type GenerationChunk = Chunk<GenerationChunkData>;

/// Context data for world generation.
pub struct Context<'a> {
    /// The biome generator. Unmodifiable.
    pub biome_generator: &'a BiomeGenerator,
    /// The chunk. Unmodifiable through here.
    pub chunk: &'a PaletteStorage<BlockEntry>,
    /// A positional random factory.
    pub random: PositionalRandomFactory<rand_xoshiro::Xoshiro512StarStar>,
    /// The ground Y level in this block position.
    pub ground_y: i32,
    /// The sea level for this planet.
    pub sea_level: i32,
}

/// Block placement rule source.
#[typetag::serde(tag = "rule_source")]
pub trait RuleSource: Sync + Send + Debug + DynClone {
    /// Placement function
    fn place(self: &Self, pos: &IVec3, context: &Context, block_registry: &BlockRegistry) -> Option<BlockEntry>;
}

dyn_clone::clone_trait_object!(RuleSource);

/// Block placement condition. Used for testing if a certain position is valid etc.
#[typetag::serde(tag = "condition_source")]
pub trait ConditionSource: Sync + Send + Debug + DynClone {
    /// Wether a block is valid.
    fn test(self: &Self, pos: &IVec3, context: &Context) -> bool;
}

dyn_clone::clone_trait_object!(ConditionSource);


/// Worldgen-only per-chunk data storage
#[derive(Clone, Default)]
pub struct GenerationChunkData {
    //
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
