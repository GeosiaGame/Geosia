//! Centralizes the authenticated client packet handling to a bevy system.

use bevy::app::{FixedPreUpdate, Plugin};
use gs_common::{
    InGameSystemSet, builtin_game_registries,
    network::{
        server::QueuedPacket,
        transport::{PacketWrapper, RPC_CLIENT_READER_OPTIONS},
    },
    prelude::rpc::PacketId,
    voxel::plugin::VoxelUniverseBuilder,
};
use gs_schemas::{
    GameSide,
    dependencies::uuid::Uuid,
    schemas::{
        CapnpExt,
        game_types_capnp::{SimpleResult, game_bootstrap_data},
        new_simple_packet_builder,
    },
};

use super::AuthenticatedNetworkClient;
use crate::{
    ClientData, ClientNetworkThreadHolder, prelude::*, states::loading_game::LoadingBootstrapPromiseResolver,
    voxel::ClientVoxelUniverseBuilder,
};
use crate::{
    states::{ClientAppState, LoadingGameSystemSet},
    voxel::NetworkVoxelClient,
};

/// Registers the relevant systems for client packet handling.
pub struct ClientPacketHandlerPlugin;

impl Plugin for ClientPacketHandlerPlugin {
    fn build(&self, app: &mut gs_common::prelude::App) {
        app.add_systems(
            FixedPreUpdate,
            (client_packet_handler_system)
                .before(InGameSystemSet)
                .before(LoadingGameSystemSet)
                .run_if(
                    Condition::or(in_state(ClientAppState::LoadingGame), in_state(ClientAppState::InGame))
                        .and(resource_exists::<AuthenticatedNetworkClient>)
                        .and(resource_exists::<ClientNetworkThreadHolder>),
                ),
        );
    }
}

/// The main system handling client packets.
#[allow(private_interfaces)]
pub fn client_packet_handler_system(
    mut client: ResMut<AuthenticatedNetworkClient>,
    net_thread: Res<ClientNetworkThreadHolder>,
    current_state: Res<State<ClientAppState>>,
    mut commands: Commands,
    mut network_voxel_client: Option<Single<&mut NetworkVoxelClient>>,
) {
    let client = &mut *client;
    let response_timestamp = net_thread.0.packet_timestamp();

    let mut handle_packet = |incoming: QueuedPacket| -> Result<()> {
        const READER_OPTIONS: capnp::message::ReaderOptions = RPC_CLIENT_READER_OPTIONS;
        let is_request = incoming.stream.initiating_side() == GameSide::Server;
        if is_request {
            // s2c
            match incoming.id {
                PacketId::Echo | PacketId::Authenticate => unreachable!(),
                PacketId::GetServerMetadata => {
                    // no-op
                }
                PacketId::BootstrapGameData => {
                    if *current_state.get() != ClientAppState::LoadingGame {
                        return Err(anyhow!("Received game bootstrap data outside of game loading"));
                    }

                    let incoming_data = incoming
                        .data
                        .parse_typed::<game_bootstrap_data::Owned>(READER_OPTIONS)?;
                    let message = incoming_data.get()?.get_payload()?;
                    let default_registries = builtin_game_registries();
                    let uuid = Uuid::read_from_message(&message.get_universe_id()?)?;
                    let registries = default_registries.clone_with_serialized_ids(&message)?;
                    let nblocks = registries.block_types.len();
                    info!("Joining server world {uuid} with {nblocks} block types.");
                    let client_data = ClientData {
                        shared_registries: registries,
                    };

                    let mut response = new_simple_packet_builder();
                    let mut root = response.init_root();
                    root.set_id(PacketId::BootstrapGameData);
                    root.set_timestamp_ms(response_timestamp);
                    let response = PacketWrapper::from(response);
                    let response_stream = incoming.stream;

                    commands.queue(move |world: &mut World| {
                        let block_registry = Arc::clone(&client_data.shared_registries.block_types);
                        let biome_registry = Arc::clone(&client_data.shared_registries.biome_types);
                        world.insert_resource(client_data);
                        VoxelUniverseBuilder::<ClientData>::new(world, block_registry, biome_registry)
                            .unwrap()
                            .with_client_chunk_system()
                            .build();

                        let _ = response_stream.send_packet(response);

                        if let Some(LoadingBootstrapPromiseResolver(resolver)) =
                            world.remove_resource::<LoadingBootstrapPromiseResolver>()
                        {
                            let _ = resolver.send(Ok(()));
                        }
                    });
                }
                PacketId::ChatMessage => {
                    let root = incoming.data.parse_typed::<capnp::text::Owned>(READER_OPTIONS)?;
                    let message = String::from_utf8_lossy(root.get()?.get_payload()?.as_bytes());
                    info!("Chat message received: {message}");
                }
                PacketId::BlockAction => {
                    // no-op
                }
                PacketId::ChunkData => {
                    network_voxel_client
                        .as_mut()
                        .context("Received chunk data while missing a processing queue")?
                        .chunk_packet_queue
                        .push_back(incoming);
                }
            }
        } else {
            // response on c2s
            match incoming.id {
                PacketId::Echo | PacketId::Authenticate => unreachable!(),
                PacketId::GetServerMetadata => {
                    // no-op
                }
                PacketId::BootstrapGameData => {
                    // no-op
                }
                PacketId::ChatMessage => {
                    let root = incoming.data.parse_typed::<capnp::text::Owned>(READER_OPTIONS)?;
                    let message = String::from_utf8_lossy(root.get()?.get_payload()?.as_bytes());
                    info!("Chat message echo received: {message}");
                }
                PacketId::BlockAction => {
                    let root = incoming.data.parse_simple(READER_OPTIONS)?;
                    let result = SimpleResult::from_i32(root.get()?.get_simple_payload());
                    info!("Block action result: {result:?}");
                }
                PacketId::ChunkData => {
                    // no-op
                }
            }
        }
        Ok(())
    };

    while let Ok(incoming) = client.packet_queue.try_recv() {
        let packet_id = incoming.id;
        if let Err(e) = handle_packet(incoming) {
            warn!("Could not handle packet {packet_id} from the server: {e}");
        }
    }
}
