//! Random noise utilities.

use noise::NoiseFn;
use std::f64::consts::TAU;

mod fbm;
mod curve;
mod cache;

pub use fbm::*;
pub use curve::*;
pub use cache::*;

/// Get a point of [`N`]-dimensional noise as if it were a plane of 2D noise
pub trait NoiseNDTo2D<const N: usize>: NoiseFn<f64, N> {
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

const CONVERT_NOISE_SCALE: f64 = 1.0;
