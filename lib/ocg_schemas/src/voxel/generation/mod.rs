//! World generation related methods.

use std::{f64::consts::TAU, fmt::Debug};

use noise::{NoiseFn, Seedable};
use serde::{Deserialize, Serialize};

use self::positional_random::PositionalRandomFactory;
use super::{biome::BiomeDefinition, chunk_storage::PaletteStorage, voxeltypes::BlockEntry};

pub mod blur;
pub mod fbm_noise;
pub mod positional_random;

/// Context data for world generation.
pub struct Context<'a> {
    /// The world seed.
    pub seed: u64,
    /// The chunk. Unmodifiable through here.
    pub chunk: &'a PaletteStorage<BlockEntry>,
    /// the current biome at this position.
    pub biome: &'a BiomeDefinition,
    /// A positional random factory.
    pub random: PositionalRandomFactory<rand_xoshiro::Xoshiro512StarStar>,
    /// The ground Y level in this block position.
    pub ground_y: i32,
    /// The sea level for this planet.
    pub sea_level: i32,
    /// height of the world above y=0
    pub height: i32,
    /// depth of the world below y=0
    pub depth: i32,
}

/// Number provider.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NumberProvider<Idx> {
    /// Constant value.
    Constant(Idx),
    /// Uniform range of `min..max` values.
    UniformRange(Idx, Idx),
    /// Uniform range of `min..=max` values.
    UniformRangeInclusive(Idx, Idx),
    /// Weighted range of values.
    WeightedRange(Vec<(Idx, Idx)>),
}

impl<Idx> NumberProvider<Idx>
where
    Idx: rand::distributions::uniform::SampleUniform
        + Copy
        + Debug
        + PartialOrd
        + Default
        + for<'a> ::core::ops::AddAssign<&'a Idx>,
{
    /// Sample this number provider for a value, given the RNG provided.
    pub fn sample<T>(&self, rand: &mut T) -> Idx
    where
        T: rand::Rng,
    {
        match self {
            NumberProvider::Constant(x) => *x,
            NumberProvider::UniformRange(min, max) => rand.sample(rand::distributions::Uniform::new(min, max)),
            NumberProvider::UniformRangeInclusive(min, max) => {
                rand.sample(rand::distributions::Uniform::new_inclusive(min, max))
            }
            NumberProvider::WeightedRange(weights) => {
                weights[rand.sample(
                    rand::distributions::WeightedIndex::new(weights.iter().map(|w| w.1))
                        .unwrap_or_else(|_| panic!("failed to generate weighted distribution from {:?}", weights)),
                )]
                .0
            }
            #[allow(unreachable_patterns)]
            x => panic!("failed to sample the range for {:?}", x),
        }
    }
}

fn build_sources<Source>(seed: u32, octaves: &[f64]) -> Vec<Source>
where
    Source: Default + Seedable,
{
    let mut sources = Vec::with_capacity(octaves.len());
    for &x in octaves {
        let source = Source::default();
        sources.push(source.set_seed(seed ^ (x * 4037543.0) as u32));
    }
    sources
}

/// Get a point of 4D "torus" noise as if it were a plane of 2D noise
pub trait Noise4DTo2D<const T: usize> {
    /// get the noise value as a 2D point.
    fn get_2d(&self, point: [f64; 2]) -> f64;
}

impl<T> Noise4DTo2D<4> for T
where
    T: NoiseFn<f64, 4> + ?Sized,
{
    fn get_2d(&self, point: [f64; 2]) -> f64 {
        let angle_x = TAU * point[0];
        let angle_y = TAU * point[1];
        self.get([
            angle_x.cos() / TAU,
            angle_x.sin() / TAU,
            angle_y.cos() / TAU,
            angle_y.sin() / TAU,
        ]) * 1.5
    }
}

impl<T> Noise4DTo2D<3> for T
where
    T: NoiseFn<f64, 3> + ?Sized,
{
    fn get_2d(&self, point: [f64; 2]) -> f64 {
        let angle_x = TAU * point[0];
        let y = point[1];
        self.get([angle_x.cos() / TAU, angle_x.sin() / TAU, y])
    }
}

impl<T> Noise4DTo2D<2> for T
where
    T: NoiseFn<f64, 2> + ?Sized,
{
    fn get_2d(&self, point: [f64; 2]) -> f64 {
        self.get(point)
    }
}
