//! Definitions for the usual entity types in the game.

use gs_schemas::player::{INVALID_ACCOUNT_ID, INVALID_CHARACTER_ID};

use crate::entity::{EntityRegistry, EntitySchema};
use crate::player::ServerPlayerAvatar;
use crate::prelude::*;

/// Registry name for the player avatar.
pub const PLAYER_AVATAR_ENTITY_NAME: RegistryName = RegistryName::gs_const("player_avatar");

/// Populates the entity registry with the default server-side entities.
/// Keep in sync with the client entity setup function.
pub fn setup_standard_server_entities(registry: &mut EntityRegistry) {
    registry
        .push_object(EntitySchema {
            name: PLAYER_AVATAR_ENTITY_NAME,
            serialize_full: |data, writer| {
                //
                Ok(())
            },
            serialize_delta: |data, writer| {
                //
                Ok(())
            },
            deserialize_full: |bytes, mut target| {
                // TODO: serialize actual values
                target.insert((
                    ServerPlayerAvatar::new(INVALID_ACCOUNT_ID, INVALID_CHARACTER_ID),
                    UniverseTransform::default(),
                ));
                Ok(())
            },
            deserialize_delta: |bytes, target| {
                //
                Ok(())
            },
        })
        .unwrap();
}
