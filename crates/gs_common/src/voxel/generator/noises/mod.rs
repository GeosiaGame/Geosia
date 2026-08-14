//! Random noise utilities.

use noise::NoiseFn;
use std::f64::consts::TAU;

mod fbm;
mod interpolate;
mod cache;
mod convert;

pub use fbm::*;
pub use interpolate::*;
pub use cache::*;
pub use convert::*;

/// Get a point of [`N`]-dimensional noise as if it were a plane of 2D noise
pub trait NoiseNDTo2D<T, U, const N: usize>: NoiseFn<U, N> {
    /// get the noise value as a 2D point.
    fn get_2d(&self, point: [T; 2]) -> f64;
}

impl<T> NoiseNDTo2D<f64, f64, 4> for T
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

impl<T> NoiseNDTo2D<f64, f64, 3> for T
where
    T: NoiseFn<f64, 3> + ?Sized,
{
    fn get_2d(&self, point: [f64; 2]) -> f64 {
        let angle_x = TAU * point[0];
        let y = point[1];
        self.get([angle_x.cos() / TAU, angle_x.sin() / TAU, y])
    }
}

impl<T> NoiseNDTo2D<f64, f64, 2> for T
where
    T: NoiseFn<f64, 2> + ?Sized,
{
    fn get_2d(&self, point: [f64; 2]) -> f64 {
        self.get(point)
    }
}

impl<T> NoiseNDTo2D<i32, i32, 4> for T
where
    T: NoiseFn<i32, 4> + ?Sized,
{
    fn get_2d(&self, point: [i32; 2]) -> f64 {
        let angle_x = TAU * point[0] as f64;
        let angle_y = TAU * point[1] as f64;
        self.get([
            (angle_x.cos() / TAU * CONVERT_NOISE_SCALE) as i32,
            (angle_x.sin() / TAU * CONVERT_NOISE_SCALE) as i32,
            (angle_y.cos() / TAU * CONVERT_NOISE_SCALE) as i32,
            (angle_y.sin() / TAU * CONVERT_NOISE_SCALE) as i32,
        ]) * 1.5
    }
}

impl<T> NoiseNDTo2D<i32, i32, 3> for T
where
    T: NoiseFn<i32, 3> + ?Sized,
{
    fn get_2d(&self, point: [i32; 2]) -> f64 {
        let angle_x = TAU * point[0] as f64;
        let y = point[1];
        self.get([(angle_x.cos() / TAU) as i32, (angle_x.sin() / TAU) as i32, y])
    }
}

impl<T> NoiseNDTo2D<i32, i32, 2> for T
where
    T: NoiseFn<i32, 2> + ?Sized,
{
    fn get_2d(&self, point: [i32; 2]) -> f64 {
        self.get(point)
    }
}

impl<T> NoiseNDTo2D<i32, f64, 4> for T
where
    T: NoiseFn<f64, 4> + ?Sized,
{
    fn get_2d(&self, point: [i32; 2]) -> f64 {
        let angle_x = TAU * point[0] as f64;
        let angle_y = TAU * point[1] as f64;
        self.get([
            angle_x.cos() / TAU * CONVERT_NOISE_SCALE,
            angle_x.sin() / TAU * CONVERT_NOISE_SCALE,
            angle_y.cos() / TAU * CONVERT_NOISE_SCALE,
            angle_y.sin() / TAU * CONVERT_NOISE_SCALE,
        ]) * 1.5
    }
}

impl<T> NoiseNDTo2D<i32, f64, 3> for T
where
    T: NoiseFn<f64, 3> + ?Sized,
{
    fn get_2d(&self, point: [i32; 2]) -> f64 {
        let angle_x = TAU * point[0] as f64;
        let y = point[1] as f64;
        self.get([angle_x.cos() / TAU, angle_x.sin() / TAU, y])
    }
}

impl<T> NoiseNDTo2D<i32, f64, 2> for T
where
    T: NoiseFn<f64, 2> + ?Sized,
{
    fn get_2d(&self, point: [i32; 2]) -> f64 {
        self.get([point[0] as f64, point[1] as f64])
    }
}

const CONVERT_NOISE_SCALE: f64 = 1.0;
