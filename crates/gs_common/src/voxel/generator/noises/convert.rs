use std::marker::PhantomData;
use noise::NoiseFn;

/// Noise function converts a f64-based noise function to a [`T`]-based one.
#[derive(Clone)]
pub struct Convert<T, U, Source, Converter, const DIM: usize>
where
    Source: NoiseFn<U, DIM>,
    Converter: Fn(T) -> U,
{
    /// Outputs a value.
    pub source: Source,
    converter: Converter,

    phantom: PhantomData<T>,
}

impl<T, U, Source, Converter, const DIM: usize> Convert<T, U, Source, Converter, DIM>
where
    Source: NoiseFn<U, DIM>,
    Converter: Fn(T) -> U,
{

    pub fn new(source: Source, converter: Converter) -> Self {
        Self {
            source,
            converter,
            phantom: PhantomData,
        }
    }
}

impl<T, U, Source, Converter, const DIM: usize> NoiseFn<T, DIM> for Convert<T, U, Source, Converter, DIM>
where
    T: Copy + Default,
    U: Copy + Default,
    Source: NoiseFn<U, DIM>,
    Converter: Fn(T) -> U,
{
    fn get(&self, point: [T; DIM]) -> f64 {
        let mut real_point = [U::default(); DIM];
        for i in 0..DIM {
            real_point[i] = (self.converter)(point[i]);
        }
        self.source.get(real_point)
    }
}
