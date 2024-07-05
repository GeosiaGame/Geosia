//! The transitional state that waits for asynchronous game initialization and server connection, before switching to the in game state.

use bevy::prelude::*;
use bevy::utils::synccell::SyncCell;
use gs_common::config::{GameConfig, ServerConfig};
use gs_common::network::thread::NetworkThread;
use gs_common::prelude::std_unbounded_channel;
use gs_common::prelude::*;
use gs_common::voxel::plugin::VoxelUniverseBuilder;
use gs_common::{builtin_game_registries, GameServer};
use gs_schemas::dependencies::uuid::Uuid;
use gs_schemas::registries::GameRegistries;
use gs_schemas::schemas::SchemaUuidExt;
use gs_schemas::GameSide;

use crate::network::NetworkThreadClientState;
use crate::states::{ClientAppState, LoadingGameSystemSet};
use crate::voxel::ClientVoxelUniverseBuilder;
use crate::{ClientData, ClientNetworkThreadHolder, GameClientControlCommandReceiver};

/// The "plugin" implementing the load transition for the game.
pub struct LoadingGamePlugin;

impl Plugin for LoadingGamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadingTransitionParams>()
            .init_resource::<LoadingPromiseHolder>();
        app.add_systems(OnEnter(ClientAppState::LoadingGame), kickoff_game_transition)
            .add_systems(Update, (loading_game_transition_handler,).in_set(LoadingGameSystemSet));
    }
}

/// Parameters for the next transition that happens.
#[derive(Clone, Resource, Debug, Default)]
pub enum LoadingTransitionParams {
    /// No queued transition.
    #[default]
    NoTransition,
    /// Unload the game and go to the main menu
    GoToMainMenu,
    /// Begin a singleplayer game.
    SinglePlayer {},
}

#[derive(Resource, Default)]
struct LoadingPromiseHolder {
    promises: Vec<Box<dyn GenericAsyncResult + Send + Sync>>,
}

fn kickoff_game_transition(world: &mut World) {
    let next_params = std::mem::take(&mut *world.resource_mut::<LoadingTransitionParams>());
    match next_params {
        LoadingTransitionParams::NoTransition => {
            static ERR_MSG: &str = "Entered game loading transition without loading parameters!";
            error!(ERR_MSG);
            panic!("{}", ERR_MSG);
        }
        LoadingTransitionParams::GoToMainMenu => {
            info!("Shutting down the currently running game");
            //
        }
        LoadingTransitionParams::SinglePlayer {} => {
            info!("Starting a new single player game");

            let default_registries = builtin_game_registries();
            let game_config = GameConfig {
                server: ServerConfig {
                    server_title: String::from("Integrated server"),
                    ..Default::default()
                },
            };
            let game_config = GameConfig::new_handle(game_config);
            let integ_server = GameServer::new(game_config).expect("Could not start integrated server");
            integ_server.set_paused(false);
            let server_pipe = integ_server.create_local_connection();
            let (control_tx, control_rx) = std_unbounded_channel();

            let net_thread = NetworkThread::new(GameSide::Client, move || NetworkThreadClientState::new(control_tx));
            let net_thread = Arc::new(net_thread);

            struct IntegBootstrap {
                registries: GameRegistries,
            }

            let net_thread2 = Arc::clone(&net_thread);
            let bootstrap_data = net_thread
                .schedule_task(|state| {
                    Box::pin(async move {
                        let local_conn = server_pipe
                            .async_wait()
                            .await
                            .context("integ_server.create_local_connection")?;
                        NetworkThreadClientState::connect_locally(state, net_thread2, local_conn)
                            .await
                            .context("NetworkThreadClientState::connect_locally")?;
                        let bootstrap_request = state
                            .borrow()
                            .server_auth_rpc()
                            .context("Missing auth endpoint")?
                            .bootstrap_game_data_request();
                        let bootstrap_response = bootstrap_request
                            .send()
                            .promise
                            .await
                            .context("Failed bootstrap request to the integrated server")?;
                        let bootstrap_response = bootstrap_response.get()?.get_data()?;
                        let uuid = Uuid::read_from_message(&bootstrap_response.get_universe_id()?);
                        let registries = default_registries.clone_with_serialized_ids(&bootstrap_response)?;
                        let nblocks = registries.block_types.len();
                        info!("Joining server world {uuid} with {nblocks} block types.");

                        Ok(IntegBootstrap { registries })
                    })
                })
                .blocking_wait()
                .expect("Could not connect the client to the integrated server");

            let client_data = ClientData {
                shared_registries: bootstrap_data.registries,
            };

            let mut promises = world.resource_mut::<LoadingPromiseHolder>();
            promises.promises.push(Box::new(net_thread.schedule_task(|state| {
                Box::pin(async move {
                    let auth_rpc = state.borrow().server_auth_rpc().cloned();
                    if let Some(auth_rpc) = auth_rpc {
                        let mut rq = auth_rpc.send_chat_message_request();
                        rq.get().set_text("Hello in-process networking!");
                        let _ = rq.send().promise.await;
                    }
                    Ok(())
                })
            })));

            let block_registry = Arc::clone(&client_data.shared_registries.block_types);
            let biome_registry = Arc::clone(&client_data.shared_registries.biome_types);

            world.insert_resource(client_data);
            world.insert_resource(ClientNetworkThreadHolder(Arc::clone(&net_thread)));
            world.insert_resource(GameClientControlCommandReceiver(SyncCell::new(control_rx)));

            VoxelUniverseBuilder::<ClientData>::new(world, block_registry, biome_registry)
                .unwrap()
                .with_network_client(&net_thread)
                .unwrap()
                .with_client_chunk_system()
                .build();

            let mut promises = world.resource_mut::<LoadingPromiseHolder>();
            promises.promises.push(Box::new(net_thread.schedule_task(|state| {
                Box::pin(async move {
                    NetworkThreadClientState::allow_streams(state).await;
                    Ok(())
                })
            })));
        }
    }
}

fn loading_game_transition_handler(
    mut next_state: ResMut<NextState<ClientAppState>>,
    mut promises: ResMut<LoadingPromiseHolder>,
) {
    let mut remaining_promises = Vec::new();
    for mut promise in promises.promises.drain(..) {
        match promise.generic_poll() {
            None => {
                remaining_promises.push(promise);
            }
            Some(Err(e)) => {
                error!("Error during loading phase: {e}");
            }
            Some(Ok(_)) => {}
        }
    }
    if remaining_promises.is_empty() {
        next_state.set(ClientAppState::InGame);
    } else {
        promises.promises.extend(remaining_promises);
    }
}
