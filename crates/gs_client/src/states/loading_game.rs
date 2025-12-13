//! The transitional state that waits for asynchronous game initialization and server connection, before switching to the in game state.

use std::net::SocketAddr;

use bevy::platform::cell::SyncCell;
use gs_common::GameServer;
use gs_common::config::{GameConfig, ServerConfig};
use gs_common::network::thread::NetworkThread;
use gs_schemas::GameSide;
use gs_schemas::savefile::SavefileMetadata;

use crate::network::{NetworkThreadClientCommand, NetworkThreadClientState};
use crate::prelude::*;
use crate::states::{ClientAppState, LoadingGameSystemSet};
use crate::{ClientNetworkThreadHolder, GameClientControlCommandReceiver};

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
    SinglePlayer {
        /// The savefile to load.
        savefile_metadata: SavefileMetadata,
    },
    /// Join a multiplayer game.
    MultiPlayer {
        /// The not-yet-resolved address to join
        server_address_raw: String,
    },
}

#[derive(Resource, Default)]
struct LoadingPromiseHolder {
    promises: Vec<Box<dyn GenericAsyncResult + Send + Sync>>,
}

/// Used to notify the loading state that the world bootstrap data has been acquired.
#[derive(Resource)]
pub struct LoadingBootstrapPromiseResolver(pub AsyncOneshotSender<Result<()>>);

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
        LoadingTransitionParams::SinglePlayer { savefile_metadata } => {
            info!("Starting a new single player game");

            let game_config = GameConfig {
                server: ServerConfig {
                    server_title: String::from("Integrated server"),
                    ..Default::default()
                },
            };
            let game_config = GameConfig::new_handle(game_config);
            let integ_server =
                GameServer::new(game_config, savefile_metadata).expect("Could not start integrated server");
            integ_server.set_paused(false);
            let server_pipe = integ_server
                .create_local_connection()
                .blocking_recv()
                .expect("Could not get integrated server connection");
            let (control_tx, control_rx) = std_unbounded_channel();

            let net_thread = NetworkThread::new(GameSide::Client, NetworkThreadClientState::new)
                .expect("Could not start client network thread");
            let net_thread = Arc::new(net_thread);

            world.insert_resource(ClientNetworkThreadHolder(Arc::clone(&net_thread)));
            world.insert_resource(GameClientControlCommandReceiver(SyncCell::new(control_rx)));

            let (connect_result, connect_result_tx) = AsyncResult::new_pair();
            net_thread.send_command(NetworkThreadClientCommand::ConnectLocally(
                Arc::clone(&net_thread),
                control_tx.clone(),
                server_pipe,
                connect_result_tx,
            ));
            connect_result
                .blocking_wait()
                .expect("Could not connect the client to the integrated server");

            kickoff_connected_game_transition(world);
        }
        LoadingTransitionParams::MultiPlayer { server_address_raw } => {
            info!("Trying to join the multiplayer game at {server_address_raw}");

            let server_address: SocketAddr = server_address_raw.parse().expect("Could not parse server address");

            let (control_tx, control_rx) = std_unbounded_channel();

            let net_thread = NetworkThread::new(GameSide::Client, NetworkThreadClientState::new)
                .expect("Could not start client network thread");
            let net_thread = Arc::new(net_thread);

            world.insert_resource(ClientNetworkThreadHolder(Arc::clone(&net_thread)));
            world.insert_resource(GameClientControlCommandReceiver(SyncCell::new(control_rx)));

            let (connect_result, connect_result_tx) = AsyncResult::new_pair();
            net_thread.send_command(NetworkThreadClientCommand::ConnectRemotely(
                Arc::clone(&net_thread),
                control_tx.clone(),
                server_address,
                connect_result_tx,
            ));
            connect_result
                .blocking_wait()
                .expect("Could not connect the client to the remote server");

            kickoff_connected_game_transition(world);
        }
    }
}

fn kickoff_connected_game_transition(world: &mut World) {
    let (world_bootstrapped, world_bootstrapped_tx) = AsyncResult::new_pair();
    world.insert_resource(LoadingBootstrapPromiseResolver(world_bootstrapped_tx));
    let mut promises = world.resource_mut::<LoadingPromiseHolder>();
    promises.promises.push(Box::new(world_bootstrapped));
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
