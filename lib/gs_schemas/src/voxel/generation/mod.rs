//! World generation related methods.

use std::f64::consts::TAU;

use noise::{NoiseFn, Seedable};

use super::{chunk_storage::PaletteStorage, voxeltypes::BlockEntry};

pub mod decorator;
pub mod fbm_noise;
pub mod positional_random;

/// Context data for world generation.
pub struct Context<'a> {
    /// The world seed.
    pub seed: u64,
    /// The chunk. Unmodifiable through here.
    pub chunk: &'a PaletteStorage<BlockEntry>,
    /// The ground Y level in this block position.
    pub ground_y: i32,
    /// The sea level for this planet.
    pub sea_level: i32,
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

const CONVERT_NOISE_SCALE: f64 = 1.0;

/// Get a point of 4D "torus" noise as if it were a plane of 2D noise
pub trait NoiseNDTo2D<const T: usize> {
    /// get the noise value as a 2D point.
    fn get_2d(&self, point: [f64; 2]) -> f64;
}

impl<T> NoiseNDTo2D<4> for T
where
    T: NoiseFn<f64, 4> + ?Sized,
{
    fn get_2d(&self, point: [f64; 2]) -> f64 {
        let angle_x = TAU * point[0];
        let angle_y = TAU * point[1];
        self.get([
            angle_x.cos() / TAU * CONVERT_NOISE_SCALE,
            angle_x.sin() / TAU * CONVERT_NOISE_SCALE,
            angle_y.cos() / TAU * CONVERT_NOISE_SCALE,
            angle_y.sin() / TAU * CONVERT_NOISE_SCALE,
        ]) * 1.5
    }
}

impl<T> NoiseNDTo2D<3> for T
where
    T: NoiseFn<f64, 3> + ?Sized,
{
    fn get_2d(&self, point: [f64; 2]) -> f64 {
        let angle_x = TAU * point[0];
        let y = point[1];
        self.get([angle_x.cos() / TAU, angle_x.sin() / TAU, y])
    }
}

impl<T> NoiseNDTo2D<2> for T
where
    T: NoiseFn<f64, 2> + ?Sized,
{
    fn get_2d(&self, point: [f64; 2]) -> f64 {
        self.get(point)
    }
}
