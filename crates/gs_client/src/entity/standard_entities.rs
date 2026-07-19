use gs_common::entity::standard_entities::*;
use gs_common::entity::{EntityRegistry, EntitySchema};
use gs_common::player::ServerPlayerAvatar;
use gs_schemas::player::{INVALID_ACCOUNT_ID, INVALID_CHARACTER_ID};

use crate::prelude::*;

/// Populates the entity registry with the default client-side entities.
/// Keep in sync with the server entity setup function.
pub fn setup_standard_client_entities(registry: &mut EntityRegistry) {
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

#[test]
fn same_entities_on_both_game_sides() {
    let mut server_registry = EntityRegistry::default();
    let mut client_registry = EntityRegistry::default();
    setup_standard_server_entities(&mut server_registry);
    setup_standard_client_entities(&mut client_registry);
    let mut server_ids: Vec<_> = server_registry
        .iter()
        .map(|(id, name, value)| name.to_owned())
        .collect();
    server_ids.sort();
    let mut client_ids: Vec<_> = client_registry
        .iter()
        .map(|(id, name, value)| name.to_owned())
        .collect();
    client_ids.sort();
    assert_eq!(server_ids, client_ids);
}
