//! Biome decorator-related types.

use bevy_math::IVec3;

use super::BiomeDefinition;
use crate::{
    registry::{RegistryDataSet, RegistryName},
    voxel::generation::Context,
};

/// A definition of a decorator type, specifying properties such as registry name, shape, placement.
#[derive(Clone)]
pub struct BiomeDecoratorDefinition<'a> {
    /// The unique registry name
    pub name: RegistryName,
    /// Placement of this biome decorator.
    pub placement: DecoratorPlacement,
    /// The biomes this decorator can be placed in.
    pub biomes: RegistryDataSet<'a, BiomeDefinition>,
}

/// Decorator types.
#[derive(Clone)]
pub enum Placer {
    /// Constant position.
    Constant(DecoratorPlacement),
    /// Uniform `min..=max` range
    UniformRange(DecoratorPlacement, i32, i32),
}

impl Placer {
    /// Pick a block based on the placer & placement type.
    pub fn pick_pos<'a, T>(&self, pos: &IVec3, rand: &mut T, context: &Context<'a>) -> IVec3
    where
        T: rand::Rng,
    {
        let y = match self {
            Placer::Constant(p) => p.get_point(pos, context),
            Placer::UniformRange(p, min, max) => {
                p.get_point(&IVec3::new(pos.x, rand.gen_range(*min..=*max), pos.z), context)
            }
        };
        IVec3::new(pos.x, y, pos.z)
    }
}

/// Decorator placement on the Y-axis
#[derive(Clone)]
pub enum DecoratorPlacement {
    /// Y above the bottom of the world.
    AboveBottom,
    /// Absolute Y value.
    Absolute,
    /// Y below the top of the world.
    BelowTop,
}

impl DecoratorPlacement {
    /// Get the Y value for this placement.
    pub fn get_point<'a>(&self, pos: &IVec3, context: &Context<'a>) -> i32 {
        let y = pos.y;
        match self {
            DecoratorPlacement::AboveBottom => context.depth + y,
            DecoratorPlacement::Absolute => y,
            DecoratorPlacement::BelowTop => context.height - y,
        }
    }
}
