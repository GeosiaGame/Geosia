//! The network server protocol implementation, hosting a game for zero or more clients.

use std::collections::BTreeMap;
use std::net::SocketAddr;

use bevy::ecs::component::{ComponentHooks, StorageType};
use bevy::ecs::world::DeferredWorld;
use bevy::log;
use bevy::prelude::*;
use capnp_rpc::rpc_twoparty_capnp::Side;
use capnp_rpc::{pry, RpcSystem};
use futures::future::BoxFuture;
use futures::FutureExt;
use gs_schemas::dependencies::capnp::capability::Promise;
use gs_schemas::dependencies::capnp::Error;
use gs_schemas::dependencies::kstring::KString;
use gs_schemas::schemas::network_capnp::authenticated_server_connection::{
    BootstrapGameDataParams, BootstrapGameDataResults, SendChatMessageParams, SendChatMessageResults,
};
use gs_schemas::schemas::{network_capnp as rpc, write_leb128, NetworkStreamHeader, SchemaUuidExt};
use quinn::{Connection, EndpointConfig};
use socket2::{Domain, Socket};
use tokio::select;
use tokio::task::{spawn_local, JoinHandle, JoinSet};
use tracing::Instrument;
use uuid::Uuid;

use crate::network::thread::NetworkThreadState;
use crate::network::transport::{
    create_local_rpc_server, create_quic_rpc_server, quinn_server_config, InProcessDuplex, InProcessStream, QuicStream,
    TransportStream,
};
use crate::network::PeerAddress;
use crate::prelude::*;
use crate::promises::ShutdownHandle;
use crate::{
    GameServer, GAME_VERSION_BUILD, GAME_VERSION_MAJOR, GAME_VERSION_MINOR, GAME_VERSION_PATCH, GAME_VERSION_PRERELEASE,
};

/// The network thread game server state, accessible from network functions.
pub struct NetworkThreadServerState {
    ready_to_accept_streams: AsyncWatchSender<bool>,
    free_local_id: i32,
    connected_clients: HashMap<PeerAddress, ConnectedNetClient>,
    bootstrapped_clients: HashMap<PeerAddress, Rc<RefCell<AuthenticatedServer2ClientEndpoint>>>,
    listeners: HashMap<SocketAddr, JoinHandle<()>>,
}

enum NetClientConnectionData {
    Local {
        stream_sender: AsyncUnboundedSender<InProcessStream>,
    },
    Remote {
        connection: Connection,
    },
}

/// Network thread data for a live connected client.
pub struct ConnectedNetClient {
    shutdown_handle: ShutdownHandle,
    data: NetClientConnectionData,
    /// The network stream for chunk data.
    pub chunk_stream: Option<TransportStream>,
}

impl ConnectedNetClient {
    /// Opens a fresh stream for sending data asynchronously to the main RPC channel.
    /// Returns a future that actually performs the work to avoid holding the state RefCell borrowed across await points.
    pub fn open_stream(&self, header: NetworkStreamHeader) -> BoxFuture<'static, Result<TransportStream>> {
        match &self.data {
            NetClientConnectionData::Local { stream_sender, .. } => {
                let stream_sender = stream_sender.clone();
                (async move {
                    let (local, remote) = InProcessStream::new_pair(header);
                    stream_sender.send(remote)?;
                    Ok(local.into())
                })
                .boxed()
            }
            NetClientConnectionData::Remote { connection, .. } => {
                let connection = connection.clone();
                (async move {
                    let (mut tx, rx) = connection.open_bi().await?;
                    let header_bytes = header.write_to_bytes();
                    let len_bytes = write_leb128(header_bytes.len() as u64);
                    tx.write_all(&len_bytes).await?;
                    tx.write_all(&header_bytes).await?;
                    Ok(QuicStream {
                        header,
                        tx: Arc::new(AsyncMutex::new(tx)),
                        rx: Arc::new(AsyncMutex::new(rx)),
                    }
                    .into())
                })
                .boxed()
            }
        }
    }

    /// Gets the shutdown handle for this client's connection handling task set.
    pub fn shutdown_handle(&self) -> &ShutdownHandle {
        &self.shutdown_handle
    }
}

/// A reference to a connected and bootstrapped player in the ECS.
pub struct ConnectedPlayer {
    /// The visible player nickname.
    pub nickname: KString,
    /// The network address the player is connected from.
    pub address: PeerAddress,
}

/// A table entity keeping lookup information for all connected players.
/// Maintained by hooks on [`ConnectedPlayer`].
#[derive(Resource, Default)]
pub struct ConnectedPlayersTable {
    /// Address-indexed players.
    players_by_address: BTreeMap<PeerAddress, Entity>,
}

impl ConnectedPlayersTable {
    /// Gets the lookup table for player entity IDs by their address.
    pub fn players_by_address(&self) -> &BTreeMap<PeerAddress, Entity> {
        &self.players_by_address
    }
}

impl Component for ConnectedPlayer {
    const STORAGE_TYPE: StorageType = StorageType::Table;

    fn register_component_hooks(hooks: &mut ComponentHooks) {
        hooks.on_insert(|mut world: DeferredWorld, entity, _component_id| {
            let player = world.get::<ConnectedPlayer>(entity).unwrap();
            let addr = player.address;
            let mut table = world.resource_mut::<ConnectedPlayersTable>();
            let old = table.players_by_address.insert(addr, entity);
            if let Some(old) = old {
                let new_nick = &world.get::<ConnectedPlayer>(entity).unwrap().nickname;
                let old_nick = world
                    .get::<ConnectedPlayer>(old)
                    .map(|p| &p.nickname as &str)
                    .unwrap_or("<missing nickname>");
                panic!(
                    "Attempting to insert a player `{new_nick}` with a duplicate peer address: {addr} of `{old_nick}`"
                );
            }
        });
        hooks.on_remove(|mut world: DeferredWorld, entity, _component_id| {
            let player = world.get::<ConnectedPlayer>(entity).unwrap();
            let addr = player.address;
            let mut table = world.resource_mut::<ConnectedPlayersTable>();
            table.players_by_address.remove(&addr);
        });
    }
}

/// A Bevy plugin registering the server-related entities.
pub struct NetworkServerPlugin;

impl Plugin for NetworkServerPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().insert_resource(ConnectedPlayersTable::default());
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
            listeners: Default::default(),
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

    /// Finds a connected client by address.
    pub fn find_connected_client_mut(&mut self, address: PeerAddress) -> Option<&mut ConnectedNetClient> {
        self.connected_clients.get_mut(&address)
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
            .instrument(tracing::info_span!("server-local-rpc", address = %peer));
        let stream_listener =
            Self::local_stream_task(Rc::clone(this_ptr), Arc::clone(&engine), peer, spipe.incoming_streams)
                .instrument(tracing::info_span!("server-local-stream", address = %peer));

        let shutdown_handle = ShutdownHandle::new();
        let mut join_set = JoinSet::new();
        join_set.spawn_local(rpc_listener);
        join_set.spawn_local(stream_listener);
        let inner_shutdown = shutdown_handle.clone();
        spawn_local(
            async move { Self::player_handler_task(inner_shutdown, join_set).await }
                .instrument(info_span!("server-player-handler", address = %peer)),
        );

        this.connected_clients.insert(
            peer,
            ConnectedNetClient {
                shutdown_handle,
                data: NetClientConnectionData::Local {
                    stream_sender: spipe.outgoing_streams,
                },
                chunk_stream: None,
            },
        );

        info!("Constructed a new local connection: {peer}");

        Ok((peer, cpipe))
    }

    async fn player_handler_task(shutdown_handle: ShutdownHandle, mut subsystem_tasks: JoinSet<Result<()>>) {
        let _guard = shutdown_handle.guard();
        'task_loop: loop {
            select! { biased;
                _shutdown = shutdown_handle.handler_future() => {
                    subsystem_tasks.abort_all();
                }
                result = subsystem_tasks.join_next() => {
                    let Some(result) = result else {break 'task_loop;};
                    match result {
                        Err(join_error) => {
                            if join_error.is_cancelled() {
                                continue;
                            } else if join_error.is_panic() {
                                std::panic::resume_unwind(join_error.into_panic());
                            } else {
                                unreachable!();
                            }
                        }
                        Ok(Err(e)) => {
                            error!("Error encountered from a player's network subsystem: {e}");
                            subsystem_tasks.abort_all();
                            continue;
                        }
                        Ok(Ok(())) => {
                            continue;
                        }
                    }
                }
            }
        }
    }

    async fn update_listeners(this: &Rc<RefCell<Self>>, engine: &Arc<GameServer>, new_listeners: &[SocketAddr]) {
        let new_set: HashSet<SocketAddr> = HashSet::from_iter(new_listeners.iter().copied());
        let old_set: HashSet<SocketAddr> = HashSet::from_iter(this.borrow().listeners.keys().copied());

        for &shutdown_addr in old_set.difference(&new_set) {
            let Some(listener) = this.borrow_mut().listeners.remove(&shutdown_addr) else {
                continue;
            };
            listener.abort();
            let _ = listener.await;
        }
        for &setup_addr in new_set.difference(&old_set) {
            let listener = Self::remote_listener_task(Rc::clone(this), Arc::clone(engine), setup_addr);
            let task = spawn_local(async move {
                if let Err(e) = listener.await {
                    error!("Listening for connections on {setup_addr} failed: {e}");
                }
            });
            this.borrow_mut().listeners.insert(setup_addr, task);
        }
    }

    async fn remote_listener_task(
        net_state: Rc<RefCell<Self>>,
        engine: Arc<GameServer>,
        server_addr: SocketAddr,
    ) -> Result<()> {
        let mut ready_watcher = net_state.borrow().ready_to_accept_streams.subscribe();
        while !*ready_watcher.borrow_and_update() {
            ready_watcher.changed().await?;
        }

        let server_config = quinn_server_config();
        let socket = Socket::new(Domain::IPV6, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
        socket.set_only_v6(false)?;
        socket.bind(&server_addr.into())?;
        let endpoint = quinn::Endpoint::new(
            EndpointConfig::default(),
            Some(server_config),
            socket.into(),
            quinn::default_runtime().unwrap(),
        )?;
        info!("Listening for QUIC connections on {}", endpoint.local_addr()?);

        while let Some(conn) = endpoint.accept().await {
            if !conn.remote_address_validated() {
                conn.retry()?;
                continue;
            }
            let conn_addr = conn.remote_address();
            let local_state = Rc::clone(&net_state);
            let local_engine = Arc::clone(&engine);
            let peer_addr = PeerAddress::Network {
                local: server_addr,
                remote: conn_addr,
            };
            spawn_local(
                async move {
                    let conn_addr = conn.remote_address();
                    let conn = match conn.await {
                        Ok(conn) => conn,
                        Err(e) => {
                            warn!(address = %conn_addr, "Client could not connect: {e}");
                            return;
                        }
                    };
                    info!(address = %conn_addr, "Accepting remote connection");
                    if let Err(e) = Self::remote_connection_task(local_state, local_engine, peer_addr, conn).await {
                        warn!(address = %conn_addr, "Client connection handler failed: {e}");
                    }
                }
                .instrument(info_span!("server-quic-peer", address = %conn_addr)),
            );
        }

        Ok(())
    }

    async fn remote_connection_task(
        net_state: Rc<RefCell<Self>>,
        engine: Arc<GameServer>,
        peer_address: PeerAddress,
        connection: Connection,
    ) -> Result<()> {
        trace!("Awaiting the bootstrap bidi RPC channel");
        let (rpc_tx, rpc_rx) = connection.accept_bi().await?;
        let rpc = create_quic_rpc_server(Rc::clone(&net_state), Arc::clone(&engine), rpc_tx, rpc_rx, peer_address);
        let _disconnector = rpc.get_disconnector();

        let mut join_set: JoinSet<Result<()>> = JoinSet::new();
        join_set.spawn_local(
            async move { rpc.await.with_context(|| format!("Remote RPC with {peer_address:?}")) }
                .instrument(info_span!("server-quic-rpc", address = %peer_address)),
        );

        let stream_this = Rc::clone(&net_state);
        let stream_engine = Arc::clone(&engine);
        let stream_conn = connection.clone();
        join_set.spawn_local(
            async move { Self::remote_stream_task(stream_this, stream_engine, peer_address, stream_conn).await }
                .instrument(info_span!("server-quic-stream", address = %peer_address)),
        );

        let shutdown_handle = ShutdownHandle::new();
        let inner_shutdown = shutdown_handle.clone();
        spawn_local(
            async move { Self::player_handler_task(inner_shutdown, join_set).await }
                .instrument(info_span!("server-player-handler", address = %peer_address)),
        );

        net_state.borrow_mut().connected_clients.insert(
            peer_address,
            ConnectedNetClient {
                shutdown_handle,
                data: NetClientConnectionData::Remote { connection },
                chunk_stream: None,
            },
        );

        info!("Constructed a new remote connection: {peer_address}");

        Ok(())
    }

    async fn remote_stream_task(
        _this: Rc<RefCell<Self>>,
        _engine: Arc<GameServer>,
        _addr: PeerAddress,
        connection: Connection,
    ) -> Result<()> {
        while let Ok((_tx, _rx)) = connection.accept_bi().await {
            // TODO
            error!("Got a stream open request from the client, currently there are no c->s streams");
        }
        Ok(())
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
            let handler = engine
                .network_thread
                .create_stream_handler(Rc::clone(&this), stream.into());
            match handler {
                Ok(handler) => {
                    spawn_local(handler);
                }
                Err(stream) => {
                    error!(
                        "No stream handler found for incoming client stream of type {:?}",
                        stream.header()
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
                info!("Spawning player `{nickname}`@{address} into the world");
                world.spawn(ConnectedPlayer {
                    nickname: nickname.clone(),
                    address,
                });
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
