//! Descriptors for in-game voxel/block types.
use std::fmt::{Debug, Formatter};

use rgb::RGBA8;
use serde::{Deserialize, Serialize};

use crate::registry::{Registry, RegistryId, RegistryName, RegistryNameRef, RegistryObject};

pub type BlockMetadata = u32;

/// A Block type reference (id + metadata) stored in a chunk, used to uniquely identify a registered block variant.
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[repr(C)]
pub struct BlockEntry {
    /// The block ID in the registry
    pub id: RegistryId,
    /// Metadata, controlled by the game engine in case of standard-shape blocks and by the block if using custom shapes
    pub metadata: BlockMetadata,
}

/// A named registry of block definitions.
pub type BlockRegistry = Registry<BlockDefinition>;

impl BlockEntry {
    /// Helper to construct a new block ID
    pub fn new(id: RegistryId, metadata: u32) -> Self {
        Self { id, metadata }
    }

    /// Helper to look up the block definition corresponding to this ID
    pub fn lookup(self, registry: &BlockRegistry) -> Option<&BlockDefinition> {
        registry.lookup_id_to_object(self.id)
    }
}

impl Debug for BlockEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlockEntry{{id={}, metadata=0x{:08X}}}", self.id, self.metadata)
    }
}

/// The type of the block definition's shape variants.
#[derive(Clone, Debug, Default, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum BlockShapeSet {
    /// All the standard shapes available (cube, slope, corner, etc.).
    #[default]
    StandardShapedMaterial,
    /// A cube filling the entire voxel
    FullCubeOnly,
    /// A block type that has its own defined shape(s) and does not use standard auto-generated ones.
    Custom {},
}

/// A definition of a block type, specifying properties such as registry name, shape, textures.
#[derive(Clone, Debug, Hash, Serialize, Deserialize)]
pub struct BlockDefinition {
    /// The unique registry name
    pub name: RegistryName,
    /// The set of shapes available
    pub shape_set: BlockShapeSet,
    /// A color that can represent the block on maps, debug views, etc.
    pub representative_color: RGBA8,
    /// If the block can be collided with
    pub has_collision_box: bool,
    /// If the block has a mesh that can be rendered
    pub has_drawable_mesh: bool,
}

/// The registry name of [`EMPTY_BLOCK`]
pub const EMPTY_BLOCK_NAME: RegistryName = RegistryName::ocg_const("empty");

/// The empty block definition, used when no specific blocks have been generated
pub static EMPTY_BLOCK: BlockDefinition = BlockDefinition {
    name: EMPTY_BLOCK_NAME,
    shape_set: BlockShapeSet::FullCubeOnly,
    representative_color: RGBA8::new(0, 0, 0, 0),
    has_collision_box: false,
    has_drawable_mesh: false,
};

impl RegistryObject for BlockDefinition {
    fn registry_name(&self) -> RegistryNameRef {
        self.name.as_ref()
    }
}

impl BlockDefinition {}
