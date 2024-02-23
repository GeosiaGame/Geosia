//! Positional random source.

use std::marker::PhantomData;

use bevy_math::IVec3;
use rand::{RngCore, SeedableRng};

/// 
pub trait Random: RngCore + SeedableRng {}
impl<T> Random for T where T: RngCore + SeedableRng {}

/// Positional random generator.
pub struct PositionalRandomFactory<Rand: Random>(PhantomData<Rand>);

impl<Rand> Default for PositionalRandomFactory<Rand> where Rand: RngCore + SeedableRng {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<Rand> PositionalRandomFactory<Rand> where Rand: RngCore + SeedableRng {
    /// Get a new random from this position.
    pub fn get_at_pos(pos: IVec3) -> Rand {
        Self::get_at_pos_i(pos.x, pos.y, pos.z)
    }

    /// Get a new random from this position.
    pub fn get_at_pos_i(x: i32, y: i32, z: i32) -> Rand {
        let seed: u64 = x as u64 ^ 10645345 | y as u64 * 136540234 & z as u64 ^ 0xABCDEF01257;
        Rand::seed_from_u64(seed)
    }
}