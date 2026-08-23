//! Centralizes the authenticated packet handling to a bevy system.

use gs_schemas::schemas::network_capnp::player_move_request;
use gs_schemas::{
    ErrorList, GameSide,
    coordinates::WorldPos,
    raycast::{RaycastHitMask, RaycastResult, RaycastSpec},
    schemas::{
        CapnpExt,
        game_types_capnp::{SimpleResult, block_action, game_bootstrap_data},
        network_capnp::{PacketId, block_action_request},
        new_packet_builder, new_simple_packet_builder,
    },
    voxel::{
        chunk_storage::ChunkStorage,
        voxeltypes::{BlockEntry, EMPTY_BLOCK_NAME},
    },
};
use smallvec::SmallVec;
use uuid::Uuid;

use super::{
    SharedRegistryHolder,
    server::{ConnectedPlayer, QueuedPacket},
    transport::PacketWrapper,
};
use crate::network::server_entity_syncer::ServerToClientEntityDirtyTag;
use crate::player::ServerPlayerAvatarController;
use crate::voxel::plugin::PersistentVoxelStorage;
use crate::{
    GameServerResource, InGameSystemSet, ServerData,
    network::transport::RPC_SERVER_READER_OPTIONS,
    prelude::*,
    raycast::{RaycastContext, raycast},
    voxel::{
        blocks::STONE_BLOCK_NAME,
        plugin::{BlockRegistryHolder, VoxelUniverse},
    },
};

/// Registers all the required bevy infrastructure for packet handling. Added by `NetworkServerPlugin` automatically.
pub struct ServerPacketHandlerPlugin;

impl Plugin for ServerPacketHandlerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedPreUpdate,
            (bootstrap_players_system, server_packet_handler_system).in_set(InGameSystemSet),
        );
    }
}

/// A tag placed on the [`ConnectedPlayer`] when the game bootstrap data has been sent to them.
#[derive(Component)]
pub struct BootstrappingGameDataTag;

/// A tag placed on the [`ConnectedPlayer`] when the game bootstrap data has been acknowledged as received by them.
#[derive(Component)]
pub struct BootstrappedGameDataTag;

fn bootstrap_players_system(
    engine: Res<GameServerResource>,
    shared_registries: Res<SharedRegistryHolder>,
    to_bootstrap: Populated<
        (Entity, &ConnectedPlayer),
        (Without<BootstrappedGameDataTag>, Without<BootstrappingGameDataTag>),
    >,
    mut commands: Commands,
) {
    let engine = &*engine.0;
    let mut bootstrap_packet = new_packet_builder::<game_bootstrap_data::Owned>();
    let mut root = bootstrap_packet.init_root();
    root.set_id(PacketId::BootstrapGameData);
    root.set_timestamp_ms(engine.network_thread.packet_timestamp());
    let mut payload = root.init_payload();
    Uuid::parse_str("05aaf964-aefa-49d0-9b6a-0aa376016ac2")
        .unwrap()
        .write_to_message(&mut payload.reborrow().init_universe_id());
    shared_registries.serialize_ids(&mut payload);
    let mut packet = PacketWrapper::from(bootstrap_packet);

    let mut bootstrapped_players: SmallVec<[_; 4]> = SmallVec::new();
    for (entity, player) in to_bootstrap.iter().skip(1) {
        let _ = player.main_s2c_stream.send_packet(packet.clone_mut());
        bootstrapped_players.push((entity, BootstrappingGameDataTag));
    }
    if let Some((entity, player)) = to_bootstrap.iter().next() {
        let _ = player.main_s2c_stream.send_packet(packet);
        bootstrapped_players.push((entity, BootstrappingGameDataTag));
    }

    commands.insert_batch(bootstrapped_players);
}

/// The main packet handling system
pub fn server_packet_handler_system(
    engine: Res<GameServerResource>,
    mut commands: Commands,
    packet_queues_query: Query<(
        Entity,
        &ConnectedPlayer,
        Has<BootstrappingGameDataTag>,
        Has<BootstrappedGameDataTag>,
    )>,
    all_players: Query<(Entity, &ConnectedPlayer)>,
    block_registry: Option<Res<BlockRegistryHolder>>,
    mut voxel_universe: Query<(
        &mut VoxelUniverse<ServerData>,
        Option<&mut PersistentVoxelStorage<ServerData>>,
    )>,
) {
    let engine = &*engine.0;
    let response_timestamp = engine.network_thread.packet_timestamp();

    let mut handle_packet = move |player_entity: Entity,
                                  player: &ConnectedPlayer,
                                  is_bootstrapping: bool,
                                  is_bootstrapped: bool,
                                  incoming: QueuedPacket,
                                  commands: &mut Commands|
          -> Result<()> {
        const READER_OPTIONS: capnp::message::ReaderOptions = RPC_SERVER_READER_OPTIONS;

        let is_request = incoming.stream.initiating_side() == GameSide::Client;
        if is_request {
            // c2s stream
            match incoming.id {
                PacketId::Echo | PacketId::Authenticate => unreachable!(),
                PacketId::GetServerMetadata => {
                    let packet: PacketWrapper = engine
                        .server_metadata
                        .lock()
                        .expect("Poisoned server metadata mutex")
                        .clone()
                        .into();
                    let _ = incoming.stream.send_packet(packet);
                }
                PacketId::BootstrapGameData => {
                    // no-op
                }
                PacketId::ChatMessage => {
                    let incoming_data = incoming.data.parse_typed::<capnp::text::Owned>(READER_OPTIONS)?;
                    let message = incoming_data.get()?.get_payload()?.as_bytes();
                    let message = String::from_utf8_lossy(message);

                    info!(
                        "Incoming chat message from {} ({}): {}",
                        player.authenticated_info.player_character.display_name,
                        player.authenticated_info.address,
                        message
                    );

                    let formatted_message = format!(
                        "[{}] {}",
                        player.authenticated_info.player_character.display_name, message
                    );
                    let mut response = new_packet_builder::<capnp::text::Owned>();
                    let mut root = response.init_root();
                    root.set_id(PacketId::ChatMessage);
                    root.set_timestamp_ms(response_timestamp);
                    root.set_payload(&formatted_message)?;
                    let mut response = PacketWrapper::from(response);

                    let mut total_result = ErrorList::new();

                    for (_, player) in all_players.iter() {
                        if player.connection_key == incoming.connection_key {
                            continue;
                        }
                        total_result.attach_if_err(player.main_s2c_stream.send_packet(response.clone_mut()));
                    }

                    total_result.attach_if_err(incoming.stream.send_packet(response));
                    total_result.into_result()?;
                }
                PacketId::BlockAction => {
                    let incoming_data = incoming
                        .data
                        .parse_typed::<block_action_request::Owned>(READER_OPTIONS)?;
                    let message = incoming_data.get()?.get_payload()?;
                    let position = message.get_position()?;
                    let action = message.get_action()?;
                    let ray_spec = RaycastSpec {
                        start: WorldPos::from_offset_blockpos(
                            IVec3::read_from_message(&position.get_position()?)?.into(),
                            Vec3::read_from_message(&position.get_offset()?)?.into(),
                        ),
                        direction: Dir3::new(Vec3::read_from_message(&position.get_look()?)?)?.into(),
                        distance_limit: 64.0,
                        hit_mask: RaycastHitMask::all(),
                    };
                    let bregistry = &**block_registry.as_ref().context("missing block registry")?;
                    let mut voxels = voxel_universe.single_mut()?;
                    let (voxels, voxel_storage) = (&mut *voxels.0, voxels.1.as_deref_mut());
                    let ray_ctx = RaycastContext {
                        block_registry: Some(bregistry),
                        voxel_world: Some(voxels),
                    };

                    let rc = raycast(&ray_ctx, &ray_spec);
                    let RaycastResult::BlockHit(rc) = rc else {
                        return Ok(());
                    };
                    let which_action = action.which()?;
                    let pos = if let block_action::Which::PlaceBlock(_) = which_action {
                        rc.position.direction_offset(rc.face, 1)
                    } else {
                        rc.position
                    };
                    let (i_stone, _) = bregistry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
                    let (i_empty, _) = bregistry.lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref()).unwrap();

                    let (chunk_pos, local) = pos.split_chunk_component();
                    if let Some(chunk) = voxels.loaded_chunks_mut().get_chunk_mut(chunk_pos) {
                        match which_action {
                            block_action::Which::PlaceBlock(_) => {
                                chunk.mutate_stored().blocks.put(local, BlockEntry::new(i_stone, 0));
                            }
                            block_action::Which::BreakBlock(_) => {
                                chunk.mutate_stored().blocks.put(local, BlockEntry::new(i_empty, 0));
                            }
                        }
                        if let Some(voxel_storage) = voxel_storage {
                            voxel_storage
                                .persistence_layer
                                .request_save(vec![(chunk_pos, chunk.clone())].into_boxed_slice());
                        }
                    }

                    let mut response = new_simple_packet_builder();
                    let mut root = response.init_root();
                    root.set_id(PacketId::BlockAction);
                    root.set_timestamp_ms(response_timestamp);
                    root.set_simple_payload(SimpleResult::Ok.as_i32());
                    let response = PacketWrapper::from(response);
                    incoming.stream.send_packet(response)?;
                }
                PacketId::ChunkData | PacketId::EntityData => {
                    // no-op
                }
                PacketId::MovePlayer => {
                    let incoming_data = incoming
                        .data
                        .parse_typed::<player_move_request::Owned>(READER_OPTIONS)?;
                    let message = incoming_data.get()?.get_payload()?;
                    let new_position = WorldPos::read_from_message(&message.get_position()?)?;
                    let new_rotation = Quat::read_from_message(&message.get_rotation()?)?;

                    commands.queue_silenced(move |world: &mut World| {
                        // Ignore errors because the player might have left the game since this command was queued.
                        let Ok(player) = world.get_entity(player_entity) else {
                            return;
                        };
                        let Some(has_character) = player.get::<ServerPlayerAvatarController>() else {
                            return;
                        };
                        let avatar_id = has_character.server_player_avatar_id();
                        let Ok(mut avatar) = world.get_entity_mut(avatar_id) else {
                            return;
                        };
                        let Some(mut utf) = avatar.get_mut::<UniverseTransform>() else {
                            return;
                        };
                        // TODO: Anti-cheat, once we actually have physics :P
                        utf.position = new_position;
                        let Some(mut tf) = avatar.get_mut::<Transform>() else {
                            return;
                        };
                        tf.rotation = new_rotation;
                        avatar.insert_if_new(ServerToClientEntityDirtyTag);
                    });

                    let mut response = new_simple_packet_builder();
                    let mut root = response.init_root();
                    root.set_id(PacketId::MovePlayer);
                    root.set_timestamp_ms(response_timestamp);
                    root.set_simple_payload(SimpleResult::Ok.as_i32());
                    let response = PacketWrapper::from(response);
                    incoming.stream.send_packet(response)?;
                }
            }
        } else {
            // is a response sent on a s2c channel
            match incoming.id {
                PacketId::Echo | PacketId::Authenticate => unreachable!(),
                PacketId::GetServerMetadata => {
                    // no-op
                }
                PacketId::BootstrapGameData => {
                    if is_bootstrapping && !is_bootstrapped {
                        commands
                            .entity(player_entity)
                            .insert(BootstrappedGameDataTag)
                            .remove::<BootstrappingGameDataTag>();

                        // Announce the join to all players
                        let mut packet = new_packet_builder::<capnp::text::Owned>();
                        let mut root = packet.init_root();
                        root.set_id(PacketId::ChatMessage);
                        root.set_timestamp_ms(response_timestamp);
                        root.set_payload(format!(
                            "{} has joined!",
                            player.authenticated_info.player_character.display_name
                        ))?;
                        let mut packet = PacketWrapper::from(packet);
                        for (_, player) in all_players.iter().skip(1) {
                            let _ = player.main_s2c_stream.send_packet(packet.clone_mut());
                        }
                        if let Some((_, player)) = all_players.iter().next() {
                            let _ = player.main_s2c_stream.send_packet(packet);
                        }
                    }
                }
                PacketId::ChatMessage => {
                    // no-op
                }
                PacketId::BlockAction => {
                    // no-op
                }
                PacketId::ChunkData | PacketId::EntityData => {
                    // no-op
                }
                PacketId::MovePlayer => {
                    // no-op
                }
            }
        }

        Ok(())
    };

    for (player_entity, player, is_bootstrapping, is_bootstrapped) in packet_queues_query.iter() {
        let mut queue = player.received_packet_queue.blocking_lock();
        while let Ok(incoming) = queue.try_recv() {
            let packet_id = incoming.id;
            let result = handle_packet(
                player_entity,
                player,
                is_bootstrapping,
                is_bootstrapped,
                incoming,
                &mut commands,
            );
            if let Err(e) = result {
                warn!(
                    "Error occured during packet {} handling from {} ({}): {}",
                    packet_id, player.authenticated_info.player_character, player.authenticated_info.address, e
                );
            }
        }
    }
}
