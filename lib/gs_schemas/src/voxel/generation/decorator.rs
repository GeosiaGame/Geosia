//! World decorator registry & data

use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use bevy_math::IVec3;
use crate::coordinates::{AbsBlockPos, AbsChunkPos};
use crate::registry::{Registry, RegistryDataSet, RegistryId, RegistryName, RegistryObject};
use crate::voxel::biome::BiomeDefinition;
use crate::voxel::chunk_storage::PaletteStorage;
use crate::voxel::generation::Context;
use crate::voxel::voxeltypes::{BlockEntry, BlockRegistry};

/// A placer function.
/// Return (true, false) if you did NOT place all blocks, but DID place some.
/// Return (false, false) if you placed NO blocks.
/// return (true, true) if you placed all blocks.
pub type PlacerFunction = fn(
    &DecoratorDefinition,
    &mut PaletteStorage<BlockEntry>,
    &mut rand_xoshiro::Xoshiro512StarStar,
    IVec3,
    AbsChunkPos,
    &BlockRegistry,
);
/// A count function.
/// returns the amount of this decorator in the area based on the input parameters.
pub type CountFunction = fn(&DecoratorDefinition, &Context<'_>, f64, f64, f64) -> usize;

/// A Biome Decorator type reference (id)
#[derive(Clone)]
#[repr(C)]
pub struct DecoratorEntry {
    /// The decorator ID in the registry
    pub id: RegistryId,
    /// The position of this decorator within this chunk.
    pub pos: AbsBlockPos,
}

impl Debug for DecoratorEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BiomeDecoratorEntry")
            .field("id", &self.id)
            .field("pos", &self.pos)
            .finish()
    }
}

impl DecoratorEntry {
    /// Helper to construct a new decorator entry
    pub fn new(id: RegistryId, pos: AbsBlockPos) -> Self {
        Self {
            id,
            pos,
        }
    }

    /// Helper to look up the decorator definition corresponding to this ID
    pub fn lookup<'a>(&'a self, registry: &'a BiomeDecoratorRegistry) -> Option<&'a BiomeDecoratorDefinition> {
        registry.lookup_id_to_object(self.id)
    }
}

/// A named registry of biome definitions.
pub type DecoratorRegistry = Registry<DecoratorDefinition>;

/// A definition of a decorator type, specifying properties such as registry name, shape, placement.
#[derive(Clone, Debug)]
pub struct DecoratorDefinition {
    /// The unique registry name
    pub name: RegistryName,
    /// The biomes this decorator can be placed in.
    pub biomes: RegistryDataSet<BiomeDefinition>,
    /// An offset added to the random placement function.
    pub salt: i32,
    /// The function that dictates how many objects to place at a given noise map position.
    /// params are (self, elevation, temperature, moisture).
    pub count_fn: Option<CountFunction>,
    /// The placer for this definition.
    /// MAKE SURE YOU DO **NOT** GO OVER CHUNK BOUNDARIES.
    pub placer_fn: Option<PlacerFunction>,
}

impl PartialEq for DecoratorDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Hash for DecoratorDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

impl RegistryObject for DecoratorDefinition {
    fn registry_name(&self) -> crate::registry::RegistryNameRef {
        self.name.as_ref()
    }
}
