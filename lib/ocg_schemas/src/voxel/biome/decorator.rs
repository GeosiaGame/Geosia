//! Biome decorator-related types.

use core::fmt::Debug;
use core::hash::Hash;
use std::{any::Any, hash::Hasher};

use bevy_math::IVec3;
use serde::{Deserialize, Serialize};
use tuple_list::TupleList;

use super::BiomeDefinition;
use crate::{
    coordinates::{AbsBlockPos, AbsChunkPos},
    registry::{Registry, RegistryDataSet, RegistryId, RegistryName, RegistryObject},
    voxel::{
        chunk_storage::PaletteStorage,
        generation::Context,
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
    pub pos: AbsBlockPos,
    /// Extra data this feature uses to determine placement.
    pub extra_data: Option<Box<dyn DecoratorData>>,
    /// is this decorator placement complete?
    pub is_complete: bool,
}

impl Debug for BiomeDecoratorEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BiomeDecoratorEntry")
            .field("id", &self.id)
            .field("pos", &self.pos)
            .field("is_complete", &self.is_complete)
            .finish()
    }
}

impl BiomeDecoratorEntry {
    /// Helper to construct a new decorator entry
    pub fn new(id: RegistryId, pos: AbsBlockPos, extra_data: Option<Box<dyn DecoratorData>>) -> Self {
        Self {
            id,
            pos,
            extra_data,
            is_complete: false,
        }
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
/// A count function.
/// returns the amount of this decorator in the area based on the input parameters.
pub type CountFunction = fn(&BiomeDecoratorDefinition, &Context<'_>, f64, f64, f64) -> usize;

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
    /// The biomes this decorator can be placed in.
    pub biomes: RegistryDataSet<BiomeDefinition>,
    /// An offset added to the random placement function.
    pub salt: i32,
    /// The function that dictates how many objects to place at a given noise map position.
    /// params are (self, elevation, temperature, moisture).
    #[serde(skip)]
    pub count_fn: Option<CountFunction>,
    /// The placer for this definition.
    /// MAKE SURE YOU DO **NOT** GO OVER CHUNK BOUNDARIES.
    #[serde(skip)]
    pub placer_fn: Option<PlacerFunction>,
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
