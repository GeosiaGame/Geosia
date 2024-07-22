//! A collection of all the shared registries that need to match up between server and client.
//! Server-only and client-only registries are stored in the respective implementations.

use std::sync::Arc;

use crate::registry::RegistryDeserializationError;
use crate::voxel::biome::BiomeRegistry;
use crate::voxel::generation::decorator::DecoratorRegistry;
use crate::voxel::voxeltypes::BlockRegistry;

/// A struct holding all the relevant shared registries.
#[derive(Clone)]
pub struct GameRegistries {
    /// Block (voxel) type definitions.
    pub block_types: Arc<BlockRegistry>,
    /// Biome type definitions.
    pub biome_types: Arc<BiomeRegistry>,
    /// Decorator type definitions.
    pub decorator_types: Arc<DecoratorRegistry>,
}

impl GameRegistries {
    /// Serializes the registry bootstrap data.
    pub fn serialize_ids(&self, builder: &mut crate::schemas::game_types_capnp::game_bootstrap_data::Builder) {
        self.block_types
            .serialize_ids(&mut builder.reborrow().init_block_registry());
        self.biome_types
            .serialize_ids(&mut builder.reborrow().init_biome_registry());
        self.decorator_types
            .serialize_ids(&mut builder.reborrow().init_decorator_registry());
    }

    /// Creates a derivative registry based on serialized bootstrap data.
    pub fn clone_with_serialized_ids(
        &self,
        bundle: &crate::schemas::game_types_capnp::game_bootstrap_data::Reader,
    ) -> Result<Self, RegistryDeserializationError> {
        let block_types = self
            .block_types
            .clone_with_serialized_ids(&bundle.get_block_registry()?)?;
        let biome_types = self
            .biome_types
            .clone_with_serialized_ids(&bundle.get_biome_registry()?)?;
        let decorator_types = self
            .decorator_types
            .clone_with_serialized_ids(&bundle.get_decorator_registry()?)?;
        Ok(Self {
            block_types: Arc::new(block_types),
            biome_types: Arc::new(biome_types),
            decorator_types: Arc::new(decorator_types),
        })
    }
}
