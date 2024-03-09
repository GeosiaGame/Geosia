//! Biome decorator-related types.

use core::hash::Hash;
use std::hash::Hasher;

use bevy_math::IVec3;

use super::BiomeDefinition;
use crate::{
    registry::{RegistryDataSet, RegistryName, RegistryObject},
    voxel::generation::{Context, NumberProvider},
};

/// A definition of a decorator type, specifying properties such as registry name, shape, placement.
#[derive(Clone)]
pub struct BiomeDecoratorDefinition<'a> {
    /// The unique registry name
    pub name: RegistryName,
    /// Placement of this biome decorator.
    pub placement: Vec<PlacementModifier>,
    /// The biomes this decorator can be placed in.
    pub biomes: RegistryDataSet<'a, BiomeDefinition>,
}

impl PartialEq for BiomeDecoratorDefinition<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Hash for BiomeDecoratorDefinition<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

impl RegistryObject for BiomeDecoratorDefinition<'_> {
    fn registry_name(&self) -> crate::registry::RegistryNameRef {
        self.name.as_ref()
    }
}

/// Decorator types.
#[derive(Clone)]
pub enum PlacementModifier {
    /// Y-position based on number provider
    YProvider(PlacementHeight, NumberProvider<i32>),
    /// On the surface of the `height_map` map of the BiomeMap.
    OnSurface(),
    /// Copy this placement `count` times.
    Count(u32),
}

impl PlacementModifier {
    /// Pick blocks to place at based on the placer & placement type.
    pub fn pick_positions<T>(&self, pos: &IVec3, rand: &mut T, context: &Context<'_>) -> Vec<IVec3>
    where
        T: rand::Rng,
    {
        let mut positions = Vec::new();
        match self {
            PlacementModifier::YProvider(p, provider) => {
                positions.push(IVec3::new(
                    pos.x,
                    p.get_point(&IVec3::new(pos.x, provider.sample(rand), pos.z), context),
                    pos.z,
                ));
            }
            PlacementModifier::OnSurface() => {
                positions.push(IVec3::new(pos.x, context.ground_y, pos.z))
            }
            PlacementModifier::Count(count) => {
                for _ in 0..=*count {
                    positions.push(*pos);
                }
            }
        };
        positions
    }
}

/// Decorator placement on the Y-axis
#[derive(Clone)]
pub enum PlacementHeight {
    /// Y above the bottom of the world.
    AboveBottom,
    /// Absolute Y value.
    Absolute,
    /// Y below the top of the world.
    BelowTop,
}

impl PlacementHeight {
    /// Get the Y value for this placement.
    pub fn get_point(&self, pos: &IVec3, context: &Context<'_>) -> i32 {
        let y = pos.y;
        match self {
            PlacementHeight::AboveBottom => context.depth + y,
            PlacementHeight::Absolute => y,
            PlacementHeight::BelowTop => context.height - y,
        }
    }
}
