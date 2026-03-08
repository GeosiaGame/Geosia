//! World decorator registry & data

use std::fmt::Debug;
use std::hash::{Hash, Hasher};

use noise::Value;
use serde::{Deserialize, Serialize};

use crate::coordinates::{AbsBlockPos, AbsChunkPos, RelBlockPos};
use crate::registry::{Registry, RegistryDataSet, RegistryName, RegistryObject};
use crate::voxel::biome::BiomeDefinition;
use crate::voxel::chunk_storage::PaletteStorage;
use crate::voxel::generation::noises::Fbm;
use crate::voxel::voxeltypes::{BlockEntry, BlockRegistry};

/// A placer function.
/// You can only mutate the current chunk in this!
pub type DecoratorPlacer = fn(
    &DecoratorDefinition,
    &mut PaletteStorage<BlockEntry>,
    &Fbm<Value>,
    RelBlockPos,
    AbsChunkPos,
    &BlockRegistry,
);
/// A count function.
/// return `true` if a decorator should be placed at this position.
pub type DecoratorPlacementCheck =
    fn(&DecoratorDefinition, &Fbm<Value>, AbsBlockPos, i32, f64, f64, f64) -> bool;

/// A named registry of biome definitions.
pub type DecoratorRegistry = Registry<DecoratorDefinition>;

/// A definition of a decorator type, specifying properties such as registry name, shape, placement.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecoratorDefinition {
    /// The unique registry name
    pub name: RegistryName,
    /// The biomes this decorator can be placed in.
    pub biomes: RegistryDataSet<BiomeDefinition>,
    /// An offset added to the random placement function.
    pub salt: i32,
    /// The function that dictates if an object should be placed at a given block position
    /// The parameters are (this definition, weird noise, world position, terrain height, elevation, temperature, moisture).
    #[serde(default = "get_empty_placement_check_fn", skip)]
    pub placement_check: DecoratorPlacementCheck,
    /// The placer for this definition.
    /// MAKE SURE YOU DO **NOT** GO OVER CHUNK BOUNDARIES.
    /// The parameters are (this definition, the chunk block storage, weird noise, chunk-relative block position, the chunk's position, the block registry).
    #[serde(default = "get_empty_placer", skip)]
    pub placer: DecoratorPlacer,
}

impl PartialEq for DecoratorDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Hash for DecoratorDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl RegistryObject for DecoratorDefinition {
    fn registry_name(&self) -> crate::registry::RegistryNameRef {
        self.name.as_ref()
    }
}

/// A placement check function that always fails
pub const EMPTY_PLACEMENT_CHECK: DecoratorPlacementCheck = |_def, _noise, _pos, _h, _e, _t, _m| false;
/// A decorator placer that does nothing
pub const EMPTY_PLACER: DecoratorPlacer = |_def, _chk, _noise, _pos, _cpos, _block_reg| {};

const fn get_empty_placement_check_fn() -> DecoratorPlacementCheck {
    EMPTY_PLACEMENT_CHECK
}
const fn get_empty_placer() -> DecoratorPlacer {
    EMPTY_PLACER
}
