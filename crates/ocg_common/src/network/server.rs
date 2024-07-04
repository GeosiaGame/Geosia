//! The network server protocol implementation, hosting a game for zero or more clients.

use std::collections::BTreeMap;
use std::net::SocketAddr;

use bevy::log;
use bevy::prelude::*;
use capnp_rpc::rpc_twoparty_capnp::Side;
use capnp_rpc::{pry, RpcSystem};
use ocg_schemas::dependencies::capnp::capability::Promise;
use ocg_schemas::dependencies::capnp::Error;
use ocg_schemas::dependencies::kstring::KString;
use ocg_schemas::schemas::network_capnp::authenticated_server_connection::{
    BootstrapGameDataParams, BootstrapGameDataResults, SendChatMessageParams, SendChatMessageResults,
};
use ocg_schemas::schemas::{network_capnp as rpc, NetworkStreamHeader, SchemaUuidExt};
use tokio::task::JoinHandle;
use tracing::Instrument;
use uuid::Uuid;

use crate::network::thread::NetworkThreadState;
use crate::network::transport::{create_local_rpc_server, InProcessDuplex, InProcessStream};
use crate::network::PeerAddress;
use crate::prelude::*;
use crate::{
    GameServer, GAME_VERSION_BUILD, GAME_VERSION_MAJOR, GAME_VERSION_MINOR, GAME_VERSION_PATCH, GAME_VERSION_PRERELEASE,
};

/// The network thread game server state, accessible from network functions.
pub struct NetworkThreadServerState {
    ready_to_accept_streams: AsyncWatchSender<bool>,
    free_local_id: i32,
    connected_clients: HashMap<PeerAddress, ConnectedNetClient>,
    bootstrapped_clients: HashMap<PeerAddress, Rc<RefCell<AuthenticatedServer2ClientEndpoint>>>,
}

/// Network thread data for a live connected client.
pub struct ConnectedNetClient {
    rpc_task: JoinHandle<Result<()>>,
    stream_task: JoinHandle<Result<()>>,
    stream_sender: AsyncUnboundedSender<InProcessStream>,
}

impl ConnectedNetClient {
    /// Opens a fresh stream for sending data asynchronously to the main RPC channel.
    pub fn open_stream(&self, header: NetworkStreamHeader) -> Result<InProcessStream> {
        let (local, remote) = InProcessStream::new_pair(header);
        self.stream_sender.send(remote)?;
        Ok(local)
    }
}

/// A reference to a connected and bootstrapped player in the ECS.
#[derive(Component)]
pub struct ConnectedPlayer {
    /// The visible player nickname.
    pub nickname: KString,
    /// The network address the player is connected from.
    pub address: PeerAddress,
}

/// A table entity keeping lookup information for all connected players.
#[derive(Component, Default)]
pub struct ConnectedPlayersTable {
    /// Nickname-indexed players.
    pub players_by_nickname: BTreeMap<KString, Entity>,
    /// Address-indexed players.
    pub players_by_address: BTreeMap<PeerAddress, Entity>,
}

/// A Bevy plugin registering the server-related entities.
pub struct NetworkServerPlugin;

impl Plugin for NetworkServerPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(ConnectedPlayersTable::default());
    }
}

impl NetworkThreadState for NetworkThreadServerState {
    async fn shutdown(_this: Rc<RefCell<Self>>) {
        //
    }
}

/// The type to connect two local network runtimes together via an in-memory virtual "connection".
pub type LocalConnectionPipe = (PeerAddress, InProcessDuplex);

impl Default for NetworkThreadServerState {
    fn default() -> Self {
        let (tx, _rx) = async_watch_channel(false);
        Self {
            ready_to_accept_streams: tx,
            free_local_id: Default::default(),
            connected_clients: Default::default(),
            bootstrapped_clients: Default::default(),
        }
    }
}

impl NetworkThreadServerState {
    /// Constructs the server state without starting any listeners.
    pub fn new() -> Self {
        Self::default()
    }

    /// Finds a connected client by address.
    pub fn find_connected_client(&self, address: PeerAddress) -> Option<&ConnectedNetClient> {
        self.connected_clients.get(&address)
    }

    /// Finds a bootstrapped client by address.
    pub fn find_bootstrapped_client(
        &self,
        address: PeerAddress,
    ) -> Option<&Rc<RefCell<AuthenticatedServer2ClientEndpoint>>> {
        self.bootstrapped_clients.get(&address)
    }

    /// Unblocks stream processing, call after all the handlers are registered.
    pub async fn allow_streams(this: &Rc<RefCell<Self>>) {
        this.borrow_mut().ready_to_accept_streams.send_replace(true);
    }

    /// Begins listening on the configured endpoints, and starts looking for configuration changes.
    /// Must be called within the tokio LocalSet.
    pub async fn bootstrap(this: &Rc<RefCell<Self>>, engine: Arc<GameServer>) -> Result<()> {
        let mut config_listener = engine.config().clone();
        let config = config_listener.borrow_and_update().server.clone();

        Self::update_listeners(this, &engine, &config.listen_addresses).await;
        Ok(())
    }

    /// Creates a new local server->client connection and returns the client address and stream to pass into the client object.
    pub async fn accept_local_connection(
        this_ptr: &Rc<RefCell<Self>>,
        engine: Arc<GameServer>,
    ) -> Result<LocalConnectionPipe> {
        let mut this = this_ptr.borrow_mut();
        let id = this.free_local_id;
        this.free_local_id += 1;
        let peer = PeerAddress::Local(id);

        let (spipe, cpipe) = InProcessDuplex::new_pair();
        let rpc_server = create_local_rpc_server(this_ptr.clone(), Arc::clone(&engine), spipe.rpc_pipe, peer);
        let rpc_listener = Self::local_listener_task(peer, Arc::clone(&engine), rpc_server)
            .instrument(tracing::info_span!("server-rpc", address = ?peer));
        let stream_listener =
            Self::local_stream_task(Rc::clone(this_ptr), Arc::clone(&engine), peer, spipe.incoming_streams)
                .instrument(tracing::info_span!("server-stream", address = ?peer));

        let rpc_task = tokio::task::spawn_local(rpc_listener);
        let stream_task = tokio::task::spawn_local(stream_listener);
        this.connected_clients.insert(
            peer,
            ConnectedNetClient {
                rpc_task,
                stream_task,
                stream_sender: spipe.outgoing_streams,
            },
        );

        info!("Constructed a new local connection: {peer:?}");

        Ok((peer, cpipe))
    }

    async fn update_listeners(this: &Rc<RefCell<Self>>, engine: &Arc<GameServer>, new_listeners: &[SocketAddr]) {
        let new_set: HashSet<SocketAddr> = HashSet::from_iter(new_listeners.iter().copied());
        let old_set: HashSet<SocketAddr> = HashSet::from_iter(
            this.borrow()
                .connected_clients
                .keys()
                .copied()
                .filter_map(PeerAddress::remote_addr),
        );
        for &shutdown_addr in old_set.difference(&new_set) {
            let addr = PeerAddress::Remote(shutdown_addr);

            // remove from the bevy world
            engine
                .schedule_bevy(move |world| {
                    let mut table = world.query::<(Entity, &ConnectedPlayersTable)>();
                    let table = table.get_single(world);
                    match table {
                        Ok((etable, table)) => {
                            let (pent, nick) = {
                                let pent = table.players_by_address.get(&addr);
                                let Some(&pent) = pent else {
                                    bail!("Mismatched player table and bevy state for {addr:?}");
                                };
                                let Some(player) = world.get::<ConnectedPlayer>(pent) else {
                                    bail!(
                                        "Mismatched player table and bevy state for {addr:?} with entity ID {pent:?}"
                                    );
                                };
                                (pent, player.nickname.clone())
                            };
                            world.despawn(pent);
                            // we have to re-borrow here
                            let mut table = world
                                .get_mut::<ConnectedPlayersTable>(etable)
                                .context("Getting ConnectedPlayersTable")?;
                            table.players_by_address.remove(&addr);
                            table.players_by_nickname.remove(&nick);
                        }
                        Err(e) => {
                            warn!("Could not remove player connection {addr:?}: {e}");
                        }
                    }
                    Ok(())
                })
                .async_log_when_fails("removing disconnected players from the ConnectedPlayersTable");

            let listener = this.borrow_mut().connected_clients.remove(&addr);
            let Some(listener) = listener else { continue };
            listener.rpc_task.abort();
            listener.stream_task.abort();
            if let Ok(Err(e)) = listener.rpc_task.await {
                log::warn!("RPC listener for address {shutdown_addr} finished with an error {e}");
            }
            if let Ok(Err(e)) = listener.stream_task.await {
                log::warn!("Stream listener for address {shutdown_addr} finished with an error {e}");
            }
        }
        for &_setup_addr in new_set.difference(&old_set) {
            /*
            let listener = Self::listener_task(PeerAddress::Remote(setup_addr), engine.clone());
            let task = tokio::task::spawn_local(listener);
            self.listeners.insert(PeerAddress::Remote(setup_addr), NetListener {
                task,
            });
             */
        }
    }

    async fn local_listener_task(
        addr: PeerAddress,
        _engine: Arc<GameServer>,
        rpc_server: RpcSystem<Side>,
    ) -> Result<()> {
        let _s_disconnector = rpc_server.get_disconnector();
        log::debug!("Starting the local listener for {addr:?}");
        rpc_server.await?;
        Ok(())
    }

    async fn local_stream_task(
        this: Rc<RefCell<Self>>,
        engine: Arc<GameServer>,
        _addr: PeerAddress,
        mut incoming_streams: AsyncUnboundedReceiver<InProcessStream>,
    ) -> Result<()> {
        let mut ready_watcher = this.borrow().ready_to_accept_streams.subscribe();
        while !*ready_watcher.borrow_and_update() {
            ready_watcher.changed().await?;
        }
        while let Some(stream) = incoming_streams.recv().await {
            let handler = engine.network_thread.create_stream_handler(Rc::clone(&this), stream);
            match handler {
                Ok(handler) => {
                    tokio::task::spawn_local(handler);
                }
                Err(stream) => {
                    error!(
                        "No stream handler found for incoming client stream of type {:?}",
                        stream.header
                    );
                }
            }
        }
        Ok(())
    }
}

/// An unauthenticated RPC client<->server connection handler on the server side.
pub struct Server2ClientEndpoint {
    net_state: Rc<RefCell<NetworkThreadServerState>>,
    server: Arc<GameServer>,
    peer: PeerAddress,
    auth_attempted: bool,
}

/// An authenticated RPC client<->server connection handler on the server side.
pub struct AuthenticatedServer2ClientEndpoint {
    _net_state: Rc<RefCell<NetworkThreadServerState>>,
    server: Arc<GameServer>,
    peer: PeerAddress,
    username: KString,
    connection: rpc::authenticated_client_connection::Client,
}

#[derive(Clone, Deref)]
#[repr(transparent)]
struct RcAuthenticatedServer2ClientEndpoint(Rc<RefCell<AuthenticatedServer2ClientEndpoint>>);

impl Server2ClientEndpoint {
    /// Constructor.
    pub fn new(net_state: Rc<RefCell<NetworkThreadServerState>>, server: Arc<GameServer>, peer: PeerAddress) -> Self {
        Self {
            net_state,
            server,
            peer,
            auth_attempted: false,
        }
    }

    /// The server this endpoint is associated with.
    pub fn server(&self) -> &Arc<GameServer> {
        &self.server
    }

    /// The peer address this endpoint is connected to.
    pub fn peer(&self) -> PeerAddress {
        self.peer
    }
}

impl rpc::game_server::Server for Server2ClientEndpoint {
    fn get_server_metadata(
        &mut self,
        _params: rpc::game_server::GetServerMetadataParams,
        mut results: rpc::game_server::GetServerMetadataResults,
    ) -> Promise<(), Error> {
        let config = self.server.config().borrow();
        let mut meta = results.get().init_metadata();
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
        Promise::ok(())
    }

    fn ping(
        &mut self,
        params: rpc::game_server::PingParams,
        mut results: rpc::game_server::PingResults,
    ) -> Promise<(), Error> {
        let input = pry!(params.get()).get_input();
        results.get().set_output(input);
        Promise::ok(())
    }

    fn authenticate(
        &mut self,
        params: rpc::game_server::AuthenticateParams,
        mut results: rpc::game_server::AuthenticateResults,
    ) -> Promise<(), Error> {
        if self.auth_attempted {
            return Promise::err(Error::failed("Authentication was already attempted once".to_owned()));
        }
        self.auth_attempted = true;

        let params = pry!(params.get());
        let username = KString::from_ref(pry!(pry!(params.get_username()).to_str()));
        let connection = pry!(params.get_connection());

        // TODO: validate username

        let client = Rc::new(RefCell::new(AuthenticatedServer2ClientEndpoint {
            _net_state: self.net_state.clone(),
            server: self.server.clone(),
            peer: self.peer,
            username: username.clone(),
            connection,
        }));

        let mut result = results.get().init_conn();
        let np_client: rpc::authenticated_server_connection::Client =
            capnp_rpc::new_client(RcAuthenticatedServer2ClientEndpoint(client.clone()));
        pry!(result.set_ok(np_client.clone()));

        self.net_state
            .borrow_mut()
            .bootstrapped_clients
            .insert(self.peer, client);

        // add to the bevy world
        let nickname = username.clone();
        let address = self.peer;
        self.server
            .schedule_bevy(move |world| {
                let player = world
                    .spawn(ConnectedPlayer {
                        nickname: nickname.clone(),
                        address,
                    })
                    .id();
                let mut table = world.query::<&mut ConnectedPlayersTable>();
                let Ok(mut table) = table.get_single_mut(world) else {
                    bail!("Could not add player connection {address:?} due to missing player table");
                };
                table.players_by_address.insert(address, player);
                table.players_by_nickname.insert(nickname, player);
                Ok(())
            })
            .async_log_when_fails("Adding player to the connection table");

        Promise::ok(())
    }
}

impl AuthenticatedServer2ClientEndpoint {
    /// The RPC instance for sending messages to the connected client.
    pub fn rpc(&self) -> &rpc::authenticated_client_connection::Client {
        &self.connection
    }
}

impl rpc::authenticated_server_connection::Server for RcAuthenticatedServer2ClientEndpoint {
    fn bootstrap_game_data(
        &mut self,
        _: BootstrapGameDataParams,
        mut results: BootstrapGameDataResults,
    ) -> Promise<(), Error> {
        let builder = results.get();
        let mut data = builder.init_data();
        // TODO: use saved world data here
        Uuid::parse_str("05aaf964-aefa-49d0-9b6a-0aa376016ac2")
            .unwrap()
            .write_to_message(&mut data.reborrow().init_universe_id());
        self.0
            .borrow()
            .server
            .server_data
            .shared_registries
            .serialize_ids(&mut data);
        Promise::ok(())
    }

    fn send_chat_message(&mut self, params: SendChatMessageParams, _: SendChatMessageResults) -> Promise<(), Error> {
        let params = pry!(params.get());
        let text = pry!(pry!(params.get_text()).to_str());
        info!(
            "Client {} ({:?}) sent a chat message `{}`",
            self.0.borrow().username,
            self.0.borrow().peer,
            text
        );
        Promise::ok(())
    }
}
