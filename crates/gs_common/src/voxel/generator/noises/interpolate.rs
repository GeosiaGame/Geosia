use core::marker::PhantomData;
use std::ops::{Add, Mul, Sub};
use noise::NoiseFn;
use gs_schemas::math::interpolate;

/// Noise function that blends the output value from the source function with cubic interpolation.
#[derive(Clone)]
pub struct Interpolate<T, Source, const DIM: usize>
where
    Source: NoiseFn<T, DIM>,
{
    /// Outputs a value.
    pub source: Source,
    sample_offset: f64,

    phantom: PhantomData<T>,
}

impl<T, Source, const DIM: usize> Interpolate<T, Source, DIM>
where
    Source: NoiseFn<T, DIM>,
{

    pub fn new(source: Source, sample_granularity: f64) -> Self {
        Self {
            source,
            sample_offset: sample_granularity,
            phantom: PhantomData,
        }
    }
}

impl<T, Source> NoiseFn<T, 1> for Interpolate<T, Source, 1>
where
    T: From<f64> + Into<f64> + Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
    Source: NoiseFn<T, 1>,
{
    fn get(&self, point: [T; 1]) -> f64 {
        let source_values = {
            let mut source_values = [0.0_f64; 4];
            for x in 0..4 {
                let x_o = T::from((x as f64 - 2.0) * self.sample_offset);
                source_values[x] = self.source.get([point[0] + x_o]);
            }
            source_values
        };

        // Now perform the cubic interpolation and return.
        interpolate::cubic(source_values, T::into(point[0]))
    }
}

impl<T, Source> NoiseFn<T, 2> for Interpolate<T, Source, 2>
where
    T: From<f64> + Into<f64> + Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
    Source: NoiseFn<T, 2>,
{
    fn get(&self, point: [T; 2]) -> f64 {
        let source_values = {
            let mut source_values = [[0.0_f64; 4]; 4];
            for x in 0..4 {
                let x_o = T::from((x as f64 - 2.0) * self.sample_offset);
                for y in 0..4 {
                    let y_o = T::from((y as f64 - 2.0) * self.sample_offset);
                    source_values[x][y] = self.source.get([point[0] + x_o, point[1] + y_o]);
                }
            }
            source_values
        };

        // Now perform the cubic interpolation and return.
        interpolate::bicubic(source_values, convert_to_f64(point))
    }
}

impl<T, Source> NoiseFn<T, 3> for Interpolate<T, Source, 3>
where
    T: From<f64> + Into<f64> + Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
    Source: NoiseFn<T, 3>,
{
    fn get(&self, point: [T; 3]) -> f64 {
        let source_values = {
            let mut source_values = [[[0.0_f64; 4]; 4]; 4];
            for x in 0..4 {
                let x_o = T::from((x as f64 - 2.0) * self.sample_offset);
                for y in 0..4 {
                    let y_o = T::from((y as f64 - 2.0) * self.sample_offset);
                    for z in 0..4 {
                        let z_o = T::from((z as f64 - 2.0) * self.sample_offset);
                        source_values[x][y][z] = self.source.get([point[0] + x_o, point[1] + y_o, point[2] + z_o]);
                    }
                }
            }
            source_values
        };

        // Now perform the cubic interpolation and return.
        interpolate::tricubic(source_values, convert_to_f64(point))
    }
}

impl<T, Source> NoiseFn<T, 4> for Interpolate<T, Source, 4>
where
    T: From<f64> + Into<f64> + Add<Output = T> + Mul<Output = T> + Sub<Output = T> + Copy,
    Source: NoiseFn<T, 4>,
{
    fn get(&self, point: [T; 4]) -> f64 {
        let source_values = {
            let mut source_values = [[[[0.0_f64; 4]; 4]; 4]; 4];
            for x in 0..4 {
                let x_o = T::from((x as f64 - 2.0) * self.sample_offset);
                for y in 0..4 {
                    let y_o = T::from((y as f64 - 2.0) * self.sample_offset);
                    for z in 0..4 {
                        let z_o = T::from((z as f64 - 2.0) * self.sample_offset);
                        for w in 0..4 {
                            let w_o = T::from((w as f64 - 2.0) * self.sample_offset);
                            source_values[x][y][z][w] = self.source.get([point[0] + x_o, point[1] + y_o, point[2] + z_o, point[3] + w_o]);
                        }
                    }
                }
            }
            source_values
        };

        // Now perform the cubic interpolation and return.
        interpolate::pentacubic(source_values, convert_to_f64(point))
    }
}

fn convert_to_f64<T, const N: usize>(array: [T; N]) -> [f64; N]
where
    T: Into<f64> + Copy,
{
    let mut result = [0.0; N];
    for i in 0..N {
        result[i] = T::into(array[i]);
    }
    result
}
