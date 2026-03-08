//! bBm noise function with configurable per-octave strength.

use bevy_math::{DVec2, DVec3, DVec4};
use noise::{MultiFractal, NoiseFn, Seedable};
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec, ToSmallVec};

/// Noise function that outputs fBm (fractal Brownian motion) noise.
///
/// fBm is a _monofractal_ method. In essence, fBm has a _constant_ fractal
/// dimension. It is as close to statistically _homogeneous_ and _isotropic_
/// as possible. Homogeneous means "the same everywhere" and isotropic means
/// "the same in all directions" (note that the two do not mean the same
/// thing).
///
/// The main difference between fractal Brownian motion and regular Brownian
/// motion is that while the increments in Brownian motion are independent,
/// the increments in fractal Brownian motion depend on the previous increment.
///
/// fBm is the result of several noise functions of ever-increasing frequency
/// and ever-decreasing amplitude.
///
/// fBm is commonly referred to as Perlin noise.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fbm<T> {
    /// The amplitudes of each octave of the noise function.
    /// The size of the list is the total number of frequency octaves to generate the noise with.
    ///
    /// The number of octaves control the _amount of detail_ in the noise
    /// function. Adding more octaves increases the detail, with the drawback
    /// of increasing the calculation time.
    pub octaves_amplitudes: SmallVec<[f64; 6]>,

    /// The number of cycles per unit length that the noise function outputs.
    pub frequency: f64,

    /// A multiplier that determines how quickly the frequency increases for
    /// each successive octave in the noise function.
    ///
    /// The frequency of each successive octave is equal to the product of the
    /// previous octave's frequency and the lacunarity value.
    ///
    /// A lacunarity of 2.0 results in the frequency doubling every octave. For
    /// almost all cases, 2.0 is a good value to use.
    pub lacunarity: f64,

    seed: u32,
    sources: SmallVec<[Option<T>; 6]>,
    scale_factor: f64,
}

fn build_sources<Source>(seed: u32, octaves: &[f64]) -> SmallVec<[Option<Source>; 6]>
where
    Source: Default + Seedable,
{
    let mut sources = SmallVec::new();
    for &octave in octaves {
        if octave == 0.0 {
            sources.push(None);
        } else {
            let source = Source::default()
                .set_seed(seed ^ (octave * 4037543.0) as u32);
            sources.push(Some(source));
        }
    }
    sources
}

impl<T> Fbm<T>
where
    T: Default + Seedable,
{
    /// Default seed for fBm noise.
    pub const DEFAULT_SEED: u32 = 0;
    /// Default octaves for fBm noise.
    pub const DEFAULT_OCTAVES: [f64; 6] = [0.5, 0.25, 0.125, 0.0625, 0.03125, 0.015625];
    /// Default frequency for fBm noise.
    pub const DEFAULT_FREQUENCY: f64 = 1.0;
    /// Default lacynarity for fBm noise.
    pub const DEFAULT_LACUNARITY: f64 = core::f64::consts::PI * 2.0 / 3.0;
    /// Default persistence for fBm noise.
    pub const DEFAULT_PERSISTENCE: f64 = 0.5;
    /// Maximum amount of octaves for fBm noise.
    pub const MAX_OCTAVES: usize = 32;

    /// Creates a new instance of FBM noise.
    pub fn new(seed: u32) -> Self {
        let octaves_amplitudes = Self::DEFAULT_OCTAVES.to_smallvec();
        Self {
            seed,
            frequency: Self::DEFAULT_FREQUENCY,
            lacunarity: Self::DEFAULT_LACUNARITY,
            sources: build_sources(seed, &octaves_amplitudes),
            scale_factor: Self::calc_scale_factor(&octaves_amplitudes),
            octaves_amplitudes,
        }
    }

    /// Sets the octave list and returns a new fBm noise generator.
    #[must_use]
    pub fn set_octaves(&self, octaves_amplitudes: &[f64]) -> Self {
        // core::index::Clamp is unstable
        // let octaves_amplitudes = octaves_amplitudes[Clamp(..=Self::MAX_OCTAVES)].to_smallvec()
        let octaves_amplitudes = {
            // limit the amplitude list's length to at most `MAX_OCTAVES` entries.
            let end = usize::min(octaves_amplitudes.len(), Self::MAX_OCTAVES);
            octaves_amplitudes[..end].to_smallvec()
        };

        Self {
            sources: build_sources(self.seed, &octaves_amplitudes),
            scale_factor: Self::calc_scale_factor(&octaves_amplitudes),
            octaves_amplitudes,
            ..*self
        }
    }

    /// Sets the source noise generator for this instance of FBM noise.
    #[must_use]
    pub fn set_sources(self, sources: SmallVec<[Option<T>; 6]>) -> Self {
        Self { sources, ..self }
    }

    /// Sets the seed for this noise.
    #[must_use]
    pub fn set_seed(&mut self, seed: u32) {
        if self.seed == seed {
            return;
        }

        self.seed = seed;
        self.sources = build_sources(seed, &self.octaves_amplitudes);
    }

    fn calc_scale_factor(octaves: &[f64]) -> f64 {
        let denom = octaves.iter().enumerate().fold(0.0, |acc, (x, &amplitude)| acc + amplitude.powi(x as i32));

        1.0 / denom
    }
}

impl<T> Default for Fbm<T>
where
    T: Default + Seedable,
{
    fn default() -> Self {
        Self::new(Self::DEFAULT_SEED)
    }
}

impl<T> MultiFractal for Fbm<T>
where
    T: Default + Seedable,
{
    /// This should always be called _before_ [`self.set_persistence`], as this function assumes default persistence.
    fn set_octaves(self, mut octaves: usize) -> Self {
        if self.octaves_amplitudes.len() == octaves {
            return self;
        }

        octaves = octaves.clamp(1, Self::MAX_OCTAVES);

        let octaves = {
            let mut octaves_amplitudes: SmallVec<[f64; 6]> = smallvec![Self::DEFAULT_PERSISTENCE; octaves];
            for x in 0..octaves_amplitudes.len() {
                octaves_amplitudes[x] = Self::DEFAULT_PERSISTENCE.powi(x as i32);
            }
            octaves_amplitudes
        };

        Self {
            sources: build_sources(self.seed, &octaves),
            scale_factor: Self::calc_scale_factor(&octaves),
            octaves_amplitudes: octaves,
            ..self
        }
    }

    /// Sets the frequency and returns a new fBm noise generator.
    fn set_frequency(self, frequency: f64) -> Self {
        Self { frequency, ..self }
    }

    /// Sets the lacunarity and returns a new fBm noise generator.
    fn set_lacunarity(self, lacunarity: f64) -> Self {
        Self { lacunarity, ..self }
    }

    fn set_persistence(self, persistence: f64) -> Self {
        let octaves_amplitudes = {
            let mut octaves_amplitudes: SmallVec<[f64; 6]> = self.octaves_amplitudes;
            for x in 0..octaves_amplitudes.len() {
                octaves_amplitudes[x] = persistence.powi(x as i32);
            }
            octaves_amplitudes
        };

        Self {
            scale_factor: Self::calc_scale_factor(&octaves_amplitudes),
            octaves_amplitudes,
            ..self
        }
    }
}

impl<T> Seedable for Fbm<T>
where
    T: Default + Seedable,
{
    /// Sets the seed for this noise.
    fn set_seed(self, seed: u32) -> Self {
        if self.seed == seed {
            return self;
        }

        Self {
            seed,
            sources: build_sources(seed, &self.octaves_amplitudes),
            ..self
        }
    }

    fn seed(&self) -> u32 {
        self.seed
    }
}

/// 2-dimensional Fbm noise
impl<T> NoiseFn<f64, 2> for Fbm<T>
where
    T: NoiseFn<f64, 2>,
{
    fn get(&self, point: [f64; 2]) -> f64 {
        let mut point = DVec2::from_array(point);

        let mut result = 0.0;

        point *= self.frequency;

        for (&amplitude, source) in self.octaves_amplitudes.iter().zip(self.sources.iter()) {
            // skip amplitude=0 (or otherwise nonexistent) octaves
            if let Some(source) = source {
                // Get the signal.
                let mut signal = source.get(point.to_array());

                // Scale the amplitude appropriately for this frequency.
                signal *= amplitude;

                // Add the signal to the result.
                result += signal;
            }

            // Increase the frequency for the next octave.
            point *= self.lacunarity;
        }

        // Scale the result into the [-1,1] range
        result * self.scale_factor
    }
}

/// 3-dimensional Fbm noise
impl<T> NoiseFn<f64, 3> for Fbm<T>
where
    T: NoiseFn<f64, 3>,
{
    fn get(&self, point: [f64; 3]) -> f64 {
        let mut point = DVec3::from_array(point);

        let mut result = 0.0;

        point *= self.frequency;

        for (&amplitude, source) in self.octaves_amplitudes.iter().zip(self.sources.iter()) {
            // skip amplitude=0 (or otherwise nonexistent) octaves
            if let Some(source) = source {
                // Get the signal.
                let mut signal = source.get(point.to_array());

                // Scale the amplitude appropriately for this frequency.
                signal *= amplitude;

                // Add the signal to the result.
                result += signal;
            }

            // Increase the frequency for the next octave.
            point *= self.lacunarity;
        }

        // Scale the result into the [-1,1] range
        result * self.scale_factor
    }
}

/// 4-dimensional Fbm noise
impl<T> NoiseFn<f64, 4> for Fbm<T>
where
    T: NoiseFn<f64, 4>,
{
    fn get(&self, point: [f64; 4]) -> f64 {
        let mut point = DVec4::from_array(point);

        let mut result = 0.0;

        point *= self.frequency;

        for (&amplitude, source) in self.octaves_amplitudes.iter().zip(self.sources.iter()) {
            // skip amplitude=0 (or otherwise nonexistent) octaves
            if let Some(source) = source {
                // Get the signal.
                let mut signal = source.get(point.to_array());

                // Scale the amplitude appropriately for this frequency.
                signal *= amplitude;

                // Add the signal to the result.
                result += signal;
            }

            // Increase the frequency for the next octave.
            point *= self.lacunarity;
        }

        // Scale the result into the [-1,1] range
        result * self.scale_factor
    }
}
