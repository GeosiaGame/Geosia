//! Game entity registry and associated types and callbacks for runtime spawning/despawning of in-universe entities.
pub mod standard_entities;

use std::hash::{Hash, Hasher};
use std::io::Write;

use bevy::ecs::world::EntityRefExcept;
use gs_schemas::registry::{Registry, RegistryObject};

use crate::prelude::*;

/// A [`EntityRef`]-like type used for network serialization.
/// It excludes mutable components needed for the operation of the synchronization system itself.
pub type NetworkEntityRef<'w, 's> = EntityRefExcept<'w, 's, (ServerToClientSyncableEntity,)>;

/// A callback type that reads network-serializable components off an entity and serializes them to the given writer.
pub type EntitySerializer = fn(data: NetworkEntityRef, writer: &mut dyn Write) -> Result<()>;
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

// Catches generic errors in the serializer signatures early for more readable compiler messages.
// This happens when e.g. the bundle in `NetworkEntityRef` is not a valid set of ECS components.
#[test]
fn can_implement_serializers() {
    fn serializer(_data: NetworkEntityRef, _writer: &mut dyn Write) -> Result<()> {
        Ok(())
    }
    let _: EntitySerializer = serializer;
    fn deserializer(_bytes: &[u8], _target: EntityWorldMut) -> Result<()> {
        Ok(())
    }
    let _: EntityDeserializer = deserializer;
}
