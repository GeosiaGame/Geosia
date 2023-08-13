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
        let seed: u64 = pos.x as u64 ^ 10645345 | pos.y as u64 * 136540234 & pos.z as u64 ^ 0xABCDEF01257;
        Rand::seed_from_u64(seed)
    }

    /// Get a new random from this position.
    pub fn get_at_pos_i(x: i32, y: i32, z: i32) -> Rand {
        Self::get_at_pos(IVec3::new(x, y, z))
    }
}