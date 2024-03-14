//! Biome decorator-related types.

use core::hash::Hash;
use std::{any::Any, hash::Hasher, num::NonZeroU32};

use bevy_math::IVec3;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tuple_list::TupleList;

use super::BiomeDefinition;
use crate::{
    coordinates::{AbsChunkPos, InChunkPos},
    registry::{Registry, RegistryDataSet, RegistryId, RegistryName, RegistryObject},
    voxel::{
        chunk_storage::PaletteStorage,
        generation::{Context, NumberProvider},
        voxeltypes::{BlockEntry, BlockRegistry},
    },
};

/// A Biome Decorator type reference (id)
#[derive(Clone)]
#[repr(C)]
pub struct BiomeDecoratorEntry {
    /// The decorator ID in the registry
    pub id: RegistryId,
    /// The position of this decorator within this chunk.
    pub pos: InChunkPos,
    /// Extra data this feature uses to determine placement.
    pub extra_data: Option<Box<dyn DecoratorData>>,
}

impl BiomeDecoratorEntry {
    /// Helper to construct a new decorator entry
    pub fn new(id: RegistryId, pos: InChunkPos, extra_data: Option<Box<dyn DecoratorData>>) -> Self {
        Self { id, pos, extra_data }
    }

    /// Helper to look up the decorator definition corresponding to this ID
    pub fn lookup<'a>(&'a self, registry: &'a BiomeDecoratorRegistry) -> Option<&'a BiomeDecoratorDefinition> {
        registry.lookup_id_to_object(self.id)
    }
}

/// A named registry of block definitions.
pub type BiomeDecoratorRegistry = Registry<BiomeDecoratorDefinition>;

/// A placer function.
/// Return (true, false) if you did NOT place all blocks, but DID place some.
/// Return (false, false) if you placed NO blocks.
/// return (true, true) if you placed all blocks.
pub type PlacerFunction = fn(
    &BiomeDecoratorDefinition,
    &Option<Box<dyn DecoratorData>>,
    &mut PaletteStorage<BlockEntry>,
    &mut rand_xoshiro::Xoshiro512StarStar,
    IVec3,
    AbsChunkPos,
    &BlockRegistry,
) -> (bool, bool, Box<dyn DecoratorData>);

/// Generic data for the decorator.
pub trait DecoratorData {
    /// Get this object as `any`.
    fn as_any(&self) -> &dyn Any;
    /// Clone this object wrapped in a box.
    fn clone_box(&self) -> Box<dyn DecoratorData>;
}
impl Clone for Box<dyn DecoratorData> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
impl<T> DecoratorData for Box<T>
where
    T: DecoratorData + Clone + 'static,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn DecoratorData> {
        self.clone()
    }
}

impl DecoratorData for i32 {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn DecoratorData> {
        Box::new(*self)
    }
}
impl DecoratorData for f64 {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn DecoratorData> {
        Box::new(*self)
    }
}
impl DecoratorData for IVec3 {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn DecoratorData> {
        Box::new(*self)
    }
}
impl DecoratorData for () {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn DecoratorData> {
        Box::new(())
    }
}
impl<Head, Tail> DecoratorData for (Head, Tail)
where
    Head: DecoratorData + Copy + 'static,
    Tail: DecoratorData + TupleList + Copy + 'static,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn DecoratorData> {
        Box::new(*self)
    }
}

/// A definition of a decorator type, specifying properties such as registry name, shape, placement.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BiomeDecoratorDefinition {
    /// The unique registry name
    pub name: RegistryName,
    /// Placement of this biome decorator.
    pub placement: Vec<PlacementModifier>,
    /// The biomes this decorator can be placed in.
    pub biomes: RegistryDataSet<BiomeDefinition>,
    /// The placer for this definition.
    /// MAKE SURE YOU DO **NOT** GO OVER CHUNK BOUNDARIES.
    #[serde(skip)]
    pub placer: Option<PlacerFunction>,
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
                if context
                    .biomes
                    .iter()
                    .any(|(biome, _)| definition.biomes.contains_value(biome))
                {
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
