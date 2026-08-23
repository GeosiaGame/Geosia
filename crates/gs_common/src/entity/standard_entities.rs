//! Definitions for the usual entity types in the game.

use capnp::message::TypedBuilder;
use gs_schemas::coordinates::WorldPos;
use gs_schemas::player::{AccountId, CharacterId};
use gs_schemas::savefile::SAVEFILE_CAPNP_READER_OPTIONS;
use gs_schemas::schemas::{CapnpExt, game_types_capnp};
use uuid::{NonNilUuid, Uuid};

use crate::entity::{EntityDeserializer, EntityRegistry, EntitySchema};
use crate::player::ServerPlayerAvatar;
use crate::prelude::*;

/// Registry name for the player avatar.
pub const PLAYER_AVATAR_ENTITY_NAME: RegistryName = RegistryName::gs_const("player_avatar");

/// Populates the entity registry with the default server-side entities.
/// Keep in sync with the client entity setup function.
pub fn setup_standard_server_entities(registry: &mut EntityRegistry) {
    let player_deser: EntityDeserializer = |mut bytes, mut target| {
        let mut ut = target.get::<UniverseTransform>().copied().unwrap_or_default();
        let mut tf = target.get::<Transform>().copied().unwrap_or_default();
        let mut avatar = target
            .get::<ServerPlayerAvatar>()
            .cloned()
            .unwrap_or(ServerPlayerAvatar::new());

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
            @ServerPlayerAvatar {
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
            serialize_full: |data, writer| {
                let ut = data.get::<UniverseTransform>().context("get<UniverseTransform>")?;
                let tf = data.get::<Transform>().context("get<Transform>")?;
                let avatar = data.get::<ServerPlayerAvatar>().context("get<ServerPlayerAvatar>")?;

                let mut builder = TypedBuilder::<game_types_capnp::entity_components::Owned>::new_default();
                let root = builder.init_root();
                let mut components = root.init_components(2);
                {
                    let mut builder = components.reborrow().get(0).init_transform();
                    ut.position.write_to_message(&mut builder.reborrow().init_position());
                    tf.scale.write_to_message(&mut builder.reborrow().init_scale());
                    tf.rotation.write_to_message(&mut builder.reborrow().init_rotation());
                }
                {
                    let mut builder = components.reborrow().get(1).init_avatar();
                    avatar
                        .account_id
                        .0
                        .get()
                        .write_to_message(&mut builder.reborrow().init_account_id());
                    avatar
                        .character_id
                        .0
                        .get()
                        .write_to_message(&mut builder.reborrow().init_character_id());
                }
                capnp::serialize::write_message(writer, &builder.into_inner())?;
                Ok(())
            },
            serialize_delta: |data, writer| {
                // TODO: detect all deltas
                let ut = data.get_ref::<UniverseTransform>().context("get<UniverseTransform>")?;
                let tf = data.get_ref::<Transform>().context("get<Transform>")?;
                let any_changed = ut.is_changed() || tf.is_changed();
                if !any_changed {
                    return Ok(());
                }

                let mut builder = TypedBuilder::<game_types_capnp::entity_components::Owned>::new_default();
                let root = builder.init_root();
                let mut components = root.init_components(1);
                {
                    let mut builder = components.reborrow().get(0).init_transform();
                    ut.position.write_to_message(&mut builder.reborrow().init_position());
                    tf.scale.write_to_message(&mut builder.reborrow().init_scale());
                    tf.rotation.write_to_message(&mut builder.reborrow().init_rotation());
                }
                capnp::serialize::write_message(writer, &builder.into_inner())?;
                Ok(())
            },
            deserialize_full: player_deser,
            deserialize_delta: player_deser,
        })
        .unwrap();
}
