//! World generation related methods.

use noise::Seedable;

use self::positional_random::PositionalRandomFactory;

use super::{voxeltypes::BlockEntry, chunk_storage::PaletteStorage};

pub mod fbm_noise;
pub mod positional_random;

/// Context data for world generation.
pub struct Context<'a> {
    /// The chunk. Unmodifiable through here.
    pub chunk: &'a PaletteStorage<BlockEntry>,
    /// A positional random factory.
    pub random: PositionalRandomFactory<rand_xoshiro::Xoshiro512StarStar>,
    /// The ground Y level in this block position.
    pub ground_y: i32,
    /// The sea level for this planet.
    pub sea_level: i32,
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
