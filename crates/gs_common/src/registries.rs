//! A collection of all the shared registries that need to match up between server and client.
//! Server-only and client-only registries are stored in the respective implementations.

use std::sync::Arc;

use gs_schemas::registry::RegistryDeserializationError;
use gs_schemas::voxel::biome::BiomeRegistry;
use gs_schemas::voxel::voxeltypes::BlockRegistry;

use crate::entity::EntityRegistry;

/// A struct holding all the relevant shared registries.
#[derive(Clone)]
pub struct GameRegistries {
    /// Block (voxel) type definitions.
    pub block_types: Arc<BlockRegistry>,
    /// Biome type definitions.
    pub biome_types: Arc<BiomeRegistry>,
    /// Entity type definitions.
    pub entity_types: Arc<EntityRegistry>,
}

impl GameRegistries {
    /// Serializes the registry bootstrap data.
    pub fn serialize_ids(&self, builder: &mut gs_schemas::schemas::game_types_capnp::game_bootstrap_data::Builder) {
        self.block_types
            .serialize_ids(&mut builder.reborrow().init_block_registry());
        self.biome_types
            .serialize_ids(&mut builder.reborrow().init_biome_registry());
        self.entity_types
            .serialize_ids(&mut builder.reborrow().init_entity_registry());
    }

    /// Creates a derivative registry based on serialized bootstrap data.
    pub fn clone_with_serialized_ids(
        &self,
        bundle: &gs_schemas::schemas::game_types_capnp::game_bootstrap_data::Reader,
    ) -> Result<Self, RegistryDeserializationError> {
        let block_types = self
            .block_types
            .clone_with_serialized_ids(&bundle.get_block_registry()?)?;
        let biome_types = self
            .biome_types
            .clone_with_serialized_ids(&bundle.get_biome_registry()?)?;
        let entity_types = self
            .entity_types
            .clone_with_serialized_ids(&bundle.get_entity_registry()?)?;
        Ok(Self {
            block_types: Arc::new(block_types),
            biome_types: Arc::new(biome_types),
            entity_types: Arc::new(entity_types),
        })
    }
}
