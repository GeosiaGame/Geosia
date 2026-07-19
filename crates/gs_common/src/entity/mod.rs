//! Game entity registry and associated types and callbacks for runtime spawning/despawning of in-universe entities.
pub mod standard_entities;

use std::hash::{Hash, Hasher};
use std::io::Write;

use gs_schemas::registry::{Registry, RegistryObject};

use crate::prelude::*;

/// A callback type that reads network-serializable components off an entity and serializes them to the given writer.
pub type EntitySerializer = fn(data: EntityRef, writer: &mut dyn Write) -> std::io::Result<()>;
/// A callback type that reads network-serializable components from a byte slice and updates the in-world entity with them.
pub type EntityDeserializer = fn(bytes: &[u8], target: EntityWorldMut) -> Result<()>;

/// Stores all the callbacks needed for constructing and updating entities based on serialized data (network or disk).
#[derive(Clone, Debug)]
pub struct EntitySchema {
    /// The registry name of the entity, do not modify after creation.
    pub name: RegistryName,
    /// Called when all entity data must be serialized.
    pub serialize_full: EntitySerializer,
    /// Called when entity data that changed since the last tick must be serialized.
    pub serialize_delta: EntitySerializer,
    /// Called when all entity data must be deserialized.
    pub deserialize_full: EntityDeserializer,
    /// Called when entity data that changed since the last tick must be deserialized.
    pub deserialize_delta: EntityDeserializer,
}

impl PartialEq for EntitySchema {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(&other.name)
    }
}

impl Hash for EntitySchema {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl RegistryObject for EntitySchema {
    fn registry_name(&self) -> RegistryNameRef {
        self.name.as_ref()
    }
}

/// A registry of entities suitable for network/disk serialization.
pub type EntityRegistry = Registry<EntitySchema>;
