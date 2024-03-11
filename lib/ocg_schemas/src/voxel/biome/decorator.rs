//! Biome decorator-related types.

use core::hash::Hash;
use std::{hash::Hasher, num::NonZeroU32};

use bevy_math::IVec3;
use itertools::Itertools;
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::BiomeDefinition;
use crate::{
    coordinates::{AbsChunkPos, CHUNK_DIM},
    registry::{Registry, RegistryDataSet, RegistryName, RegistryObject},
    voxel::{
        chunk_group::ChunkGroup,
        chunk_storage::PaletteStorage,
        generation::{positional_random::PositionalRandomFactory, Context, NumberProvider},
        voxeltypes::BlockEntry,
    },
    OcgExtraData,
};

/// A named registry of block definitions.
pub type BiomeDecoratorRegistry = Registry<BiomeDecoratorDefinition>;

/// A definition of a decorator type, specifying properties such as registry name, shape, placement.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BiomeDecoratorDefinition {
    /// The unique registry name
    pub name: RegistryName,
    /// Placement of this biome decorator.
    pub placement: Vec<PlacementModifier>,
    /// The biomes this decorator can be placed in.
    pub biomes: RegistryDataSet<BiomeDefinition>,

    #[serde(skip)]
    placer: Option<fn(&Self, &mut [&mut PaletteStorage<BlockEntry>], &mut rand_xoshiro::Xoshiro512StarStar, IVec3) -> bool>,
}

impl PartialEq for BiomeDecoratorDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Hash for BiomeDecoratorDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

impl RegistryObject for BiomeDecoratorDefinition {
    fn registry_name(&self) -> crate::registry::RegistryNameRef {
        self.name.as_ref()
    }
}

impl BiomeDecoratorDefinition {
    /// Place this decorator as defined by `placement` and `placer`.
    pub fn do_place<ED: OcgExtraData>(
        &self,
        chunk_group: &mut ChunkGroup<ED>,
        pos: &IVec3,
        rand: &mut rand_xoshiro::Xoshiro512StarStar,
        context: &Context<'_>,
    ) -> bool {
        let iter = [*pos];
        let mut iter = Box::new(iter.into_iter()) as Box<dyn Iterator<Item = IVec3>>;

        for modifier in &self.placement {
            // TODO somehow get rid of the PositionalRandomFactory usage, it bothers me.
            let val = iter.flat_map(|pos| {
                modifier.pick_positions(pos, &mut PositionalRandomFactory::get_at_pos(pos), context, self)
            });
            iter = Box::new(val) as Box<dyn Iterator<Item = IVec3>>;
        }

        let chunks = &mut chunk_group
            .get_neighborhood_around_mut(AbsChunkPos(*pos / CHUNK_DIM))
            .transpose_option()
            .unwrap();
        let chunks = &mut chunks
            .objects_xzy_mut()
            .iter_mut()
            .map(|c| &mut c.blocks)
            .collect_vec()[..];

        let mut result = false;
        if let Some(placer) = self.placer {
            for entry in iter {
                if (placer)(self, chunks, rand, entry) {
                    result = true;
                }
            }
        }
        result
    }
}

/// Decorator types.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PlacementModifier {
    /// Y-position based on number provider
    YProvider(PlacementHeight, NumberProvider<i32>),
    /// On the surface of the `height_map` map of the BiomeMap.
    OnSurface(),
    /// Copy this placement `count` times.
    Count(u32),
    /// Rarity filter. chance is calculated as 1 / this.
    RarityFilter(NonZeroU32),
    /// offset xyz by the value the NumberProviders give.
    RandomOffset(NumberProvider<i32>, NumberProvider<i32>, NumberProvider<i32>),
    /// Return current position if this definition's biomes are valid. otherwise, none.
    BiomeFilter,
}

impl PlacementModifier {
    /// Pick blocks to place at based on the placer & placement type.
    pub fn pick_positions(
        &self,
        pos: IVec3,
        rand: &mut rand_xoshiro::Xoshiro512StarStar,
        context: &Context<'_>,
        definition: &BiomeDecoratorDefinition,
    ) -> Vec<IVec3> {
        let mut positions = Vec::new();
        match self {
            PlacementModifier::YProvider(p, provider) => {
                positions.push(IVec3::new(
                    pos.x,
                    p.get_point(&IVec3::new(pos.x, provider.sample(rand), pos.z), context),
                    pos.z,
                ));
            }
            PlacementModifier::OnSurface() => positions.push(IVec3::new(pos.x, context.ground_y, pos.z)),
            PlacementModifier::Count(count) => {
                for _ in 0..=*count {
                    positions.push(pos);
                }
            }
            PlacementModifier::RarityFilter(chance) => {
                if rand.gen::<f32>() > (1.0 / chance.get() as f32) {
                    positions.push(pos);
                }
            }
            PlacementModifier::RandomOffset(x, y, z) => {
                positions.push(IVec3::new(
                    pos.x + x.sample(rand),
                    pos.y + y.sample(rand),
                    pos.z + z.sample(rand),
                ));
            }
            PlacementModifier::BiomeFilter => {
                if definition.biomes.contains(context.biome) {
                    positions.push(pos);
                }
            }
        };
        positions
    }
}

/// Decorator placement on the Y-axis
#[derive(Clone, Debug, Serialize, Deserialize)]
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
