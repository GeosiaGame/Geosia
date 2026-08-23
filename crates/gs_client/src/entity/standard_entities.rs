//! Defines the default entity types and their client-side deserialization functions.
use std::io::Write;

use gs_common::entity::standard_entities::*;
use gs_common::entity::{EntityDeserializer, EntityRegistry, EntitySchema, NetworkEntityRef};
use gs_schemas::coordinates::WorldPos;
use gs_schemas::player::{AccountId, CharacterId};
use gs_schemas::savefile::SAVEFILE_CAPNP_READER_OPTIONS;
use gs_schemas::schemas::{CapnpExt, game_types_capnp};
use uuid::{NonNilUuid, Uuid};

use crate::entity::remote_player_avatar::RemotePlayerAvatar;
use crate::prelude::*;

fn dummy_client_serializer(_data: NetworkEntityRef, _writer: &mut dyn Write) -> Result<()> {
    panic!("Trying to serialize an entity for storage on the client")
}

/// Populates the entity registry with the default client-side entities.
/// Keep in sync with the server entity setup function.
pub fn setup_standard_client_entities(registry: &mut EntityRegistry) {
    let player_deser: EntityDeserializer = |mut bytes, mut target| {
        let mut avatar = target
            .get::<RemotePlayerAvatar>()
            .cloned()
            .unwrap_or(RemotePlayerAvatar::new());
        let mut ut = target.get::<UniverseTransform>().copied().unwrap_or_default();
        let mut tf = target.get::<Transform>().copied().unwrap_or_default();

        let reader = capnp::serialize::read_message_from_flat_slice(&mut bytes, SAVEFILE_CAPNP_READER_OPTIONS)?;
        let reader = reader.get_root::<game_types_capnp::entity_components::Reader>()?;
        let components = reader.get_components()?;

        for component in components.iter() {
            match component.which()? {
                game_types_capnp::entity_component::Which::Transform(reader) => {
                    let reader = reader?;
                    if reader.has_position() {
                        ut.position = WorldPos::read_from_message(&reader.get_position()?)?;
                    }
                    if reader.has_scale() {
                        tf.scale = Vec3::read_from_message(&reader.get_scale()?)?;
                    }
                    if reader.has_rotation() {
                        tf.rotation = Quat::read_from_message(&reader.get_rotation()?)?;
                    }
                }
                game_types_capnp::entity_component::Which::Avatar(reader) => {
                    let reader = reader?;
                    if reader.has_account_id() {
                        avatar.account_id = AccountId(NonNilUuid::try_from(Uuid::read_from_message(
                            &reader.get_account_id()?,
                        )?)?);
                    }
                    if reader.has_character_id() {
                        avatar.character_id = CharacterId(NonNilUuid::try_from(Uuid::read_from_message(
                            &reader.get_character_id()?,
                        )?)?);
                    }
                }
                _ => {
                    // skip unsupported components
                }
            }
        }

        target.apply_scene(bsn! {
            @RemotePlayerAvatar {
                account_id: {avatar.account_id},
                character_id: {avatar.character_id},
            }
            UniverseTransform { position: {ut.position} }
            Transform {
                scale: {tf.scale},
                rotation: {tf.rotation}
            }
        })?;

        Ok(())
    };
    registry
        .push_object(EntitySchema {
            name: PLAYER_AVATAR_ENTITY_NAME,
            serialize_full: dummy_client_serializer,
            serialize_delta: dummy_client_serializer,
            deserialize_full: player_deser,
            deserialize_delta: player_deser,
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
        .map(|(_id, name, _value)| name.to_owned())
        .collect();
    server_ids.sort();
    let mut client_ids: Vec<_> = client_registry
        .iter()
        .map(|(_id, name, _value)| name.to_owned())
        .collect();
    client_ids.sort();
    assert_eq!(server_ids, client_ids);
}
