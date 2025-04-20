//! Centralizes the authenticated packet handling to a bevy system.

use gs_schemas::{
    ErrorList, GameSide,
    actions::BlockAction,
    coordinates::WorldPos,
    raycast::{RaycastHitMask, RaycastResult, RaycastSpec},
    schemas::{
        CapnpExt,
        game_types_capnp::{self, SimpleResult, block_action, game_bootstrap_data},
        network_capnp::{PacketId, block_action_request, game_server_metadata},
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
    server::{ConnectedPlayer, QueuedPacket},
    transport::PacketWrapper,
};
use crate::{
    GAME_VERSION_BUILD, GAME_VERSION_MAJOR, GAME_VERSION_MINOR, GAME_VERSION_PATCH, GAME_VERSION_PRERELEASE,
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
    engine.server_data.shared_registries.serialize_ids(&mut payload);
    let mut packet = PacketWrapper::from(bootstrap_packet);

    let mut bootstrapped_players: SmallVec<[_; 4]> = SmallVec::new();
    for (entity, player) in to_bootstrap.iter().skip(1) {
        player.main_s2c_stream.send_packet(packet.clone_mut());
        bootstrapped_players.push((entity, BootstrappingGameDataTag));
    }
    if let Some((entity, player)) = to_bootstrap.iter().next() {
        player.main_s2c_stream.send_packet(packet);
        bootstrapped_players.push((entity, BootstrappingGameDataTag));
    }

    commands.insert_batch(bootstrapped_players);
}

/// The main packet handling system
pub fn server_packet_handler_system(
    engine: Res<GameServerResource>,
    mut commands: Commands,
    packet_queues_query: Populated<(
        Entity,
        &ConnectedPlayer,
        Has<BootstrappingGameDataTag>,
        Has<BootstrappedGameDataTag>,
    )>,
    all_players: Query<(Entity, &ConnectedPlayer)>,
    block_registry: Option<Res<BlockRegistryHolder>>,
    mut voxel_universe: Single<Option<&mut VoxelUniverse<ServerData>>>,
) {
    let engine = &*engine.0;
    let response_timestamp = engine.network_thread.packet_timestamp();

    let mut handle_packet = move |player_entity: Entity,
                                  player: &ConnectedPlayer,
                                  is_bootstrapping: bool,
                                  is_bootstrapped: bool,
                                  incoming: QueuedPacket|
          -> Result<()> {
        const READER_OPTIONS: capnp::message::ReaderOptions = RPC_SERVER_READER_OPTIONS;

        let is_request = incoming.stream.initiating_side() == GameSide::Server;
        if is_request {
            // c2s
            match incoming.id {
                PacketId::Echo | PacketId::Authenticate => unreachable!(),
                PacketId::GetServerMetadata => {
                    let mut response = new_packet_builder::<game_server_metadata::Owned>();
                    let mut root = response.init_root();
                    root.set_id(rpc::PacketId::GetServerMetadata);
                    root.set_timestamp_ms(0);
                    let mut meta = root.init_payload();
                    let config = engine.config().borrow();
                    let mut ver = meta.reborrow().init_server_version();
                    ver.set_major(GAME_VERSION_MAJOR);
                    ver.set_minor(GAME_VERSION_MINOR);
                    ver.set_patch(GAME_VERSION_PATCH);
                    ver.set_build(GAME_VERSION_BUILD);
                    ver.set_prerelease(GAME_VERSION_PRERELEASE);

                    meta.set_title(&config.server.server_title);
                    meta.set_subtitle(&config.server.server_subtitle);
                    meta.set_player_count(0);
                    meta.set_player_limit(config.server.max_players as i32);
                    let _ = incoming.stream.send_packet(response.into());
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
                        player.authenticated_info.username, player.authenticated_info.address, message
                    );

                    let formatted_message = format!("[{}] {}", player.authenticated_info.username, message);
                    let mut response = new_packet_builder::<capnp::text::Owned>();
                    let mut root = response.init_root();
                    root.set_id(rpc::PacketId::ChatMessage);
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
                    let voxels = &mut *voxel_universe
                        .as_mut()
                        .map(Mut::reborrow)
                        .context("missing voxel universe")?;
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

                    let (chunk, local) = pos.split_chunk_component();
                    if let Some(chunk) = voxels.loaded_chunks_mut().get_chunk_mut(chunk) {
                        match which_action {
                            block_action::Which::PlaceBlock(_) => {
                                chunk.mutate_stored().blocks.put(local, BlockEntry::new(i_stone, 0));
                            }
                            block_action::Which::BreakBlock(_) => {
                                chunk.mutate_stored().blocks.put(local, BlockEntry::new(i_empty, 0));
                            }
                        }
                    }

                    let mut response = new_simple_packet_builder();
                    let mut root = response.init_root();
                    root.set_id(PacketId::BlockAction);
                    root.set_timestamp_ms(response_timestamp);
                    root.set_simple_payload(SimpleResult::Ok.as_i32());
                }
                PacketId::ChunkData => {
                    // no-op
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
                    }
                }
                PacketId::ChatMessage => {
                    // no-op
                }
                PacketId::BlockAction => {
                    // no-op
                }
                PacketId::ChunkData => {
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
            let result = handle_packet(player_entity, player, is_bootstrapping, is_bootstrapped, incoming);
            if let Err(e) = result {
                warn!(
                    "Error occured during packet {} handling from {} ({}): {}",
                    packet_id, player.authenticated_info.username, player.authenticated_info.address, e
                );
            }
        }
    }
}
