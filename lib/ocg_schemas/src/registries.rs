//! A collection of all the shared registries that need to match up between server and client.
//! Server-only and client-only registries are stored in the respective implementations.

use crate::registry::RegistryDeserializationError;
use crate::voxel::voxeltypes::BlockRegistry;

/// A struct holding all the relevant shared registries.
#[derive(Clone)]
pub struct GameRegistries {
    /// Block (voxel) type definitions.
    pub block_types: BlockRegistry,
}

impl GameRegistries {
    /// Serializes the registry bootstrap data.
    pub fn serialize_ids(&self, builder: &mut crate::schemas::game_types_capnp::game_bootstrap_data::Builder) {
        self.block_types
            .serialize_ids(&mut builder.reborrow().init_block_registry());
    }

    /// Creates a derivative registry based on serialized bootstrap data.
    pub fn clone_with_serialized_ids(
        &self,
        bundle: &crate::schemas::game_types_capnp::game_bootstrap_data::Reader,
    ) -> Result<Self, RegistryDeserializationError> {
        let block_types = self
            .block_types
            .clone_with_serialized_ids(&bundle.get_block_registry()?)?;
        Ok(Self { block_types })
    }
}
