//! The network server protocol implementation, hosting a game for zero or more clients.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Instant;

use bevy::ecs::component::{ComponentHooks, Mutable, StorageType};
use bevy::ecs::world::DeferredWorld;
use gs_schemas::GameSide;
use gs_schemas::dependencies::kstring::KString;
use gs_schemas::schemas::game_types_capnp::result;
use gs_schemas::schemas::network_capnp::{
    PacketId, authentication_acknowledgement, authentication_error, authentication_request, game_server_metadata,
};
use gs_schemas::schemas::{network_capnp as rpc, new_packet_builder, new_simple_packet_builder};
use quinn::{Endpoint, EndpointConfig, VarInt};
use slotmap::{SlotMap, new_key_type};
use socket2::{Domain, Socket};
use tokio::task::{JoinHandle, JoinSet, spawn_local};
use tracing::Instrument;

use super::server_packet_handler::ServerPacketHandlerPlugin;
use super::thread::NetworkThreadState;
use super::transport::{
    NetworkConnection, PacketStream, PacketWrapper, RPC_SERVER_READER_OPTIONS,
    RPC_SERVER_UNAUTHENTICATED_READER_OPTIONS,
};
use crate::network::PeerAddress;
use crate::network::transport::{InProcessDuplex, quinn_server_config};
use crate::prelude::*;
use crate::{
    GAME_VERSION_BUILD, GAME_VERSION_MAJOR, GAME_VERSION_MINOR, GAME_VERSION_PATCH, GAME_VERSION_PRERELEASE, GameServer,
};

new_key_type! {
    /// Slotmap key for identifying unique connections made by clients to the server
    pub struct ServerConnectionKey;
    /// Slotmap key for identifying unique streams for a given client (may not be unique across different players)
    pub struct PacketStreamKey;
}

#[allow(dead_code)]
struct ServerConnection {
    authenticated_info: AuthenticatedInfo,
    packet_streams: SlotMap<PacketStreamKey, Arc<PacketStream>>,
    main_c2s_stream: PacketStreamKey,
    main_s2c_stream: PacketStreamKey,
    connection: Arc<NetworkConnection>,
}

/// The network thread game server state, accessible from network functions.
pub struct NetworkThreadServerState {
    free_local_id: i32,
    listeners: HashMap<SocketAddr, (Endpoint, JoinHandle<()>)>,
    connections: SlotMap<ServerConnectionKey, ServerConnection>,
}

#[allow(private_interfaces)]
/// Command type for performing changes on the server network runtime thread.
pub enum NetworkThreadServerCommand {
    /// Reloads the current listener list from the server config.
    UpdateListeners(Arc<GameServer>, AsyncOneshotSender<Result<()>>),
    /// Creates an in-process network connection.
    CreateLocalConnection(Arc<GameServer>, AsyncOneshotSender<NetworkConnection>),
    /// Inserts a connection object into the connection table, returning the key.
    InsertServerConnection(Box<ServerConnection>, AsyncOneshotSender<ServerConnectionKey>),
    /// Removes a connection object from the connection table and schedules a removal of the corresponding bevy objects.
    RemoveServerConnection(Arc<GameServer>, ServerConnectionKey),
    /// Opens a new stream on a given connection, use an [`AsyncResult`] to read the result.
    OpenNewStream(
        ServerConnectionKey,
        AsyncOneshotSender<Result<(Arc<PacketStream>, PacketStreamKey)>>,
    ),
    /// Inserts a new stream into the table and returns the key for it.
    InsertAcceptedStream(
        ServerConnectionKey,
        Arc<PacketStream>,
        AsyncOneshotSender<PacketStreamKey>,
    ),
    /// Deregisters a (closed) stream from the stream table.
    RemoveStream(ServerConnectionKey, PacketStreamKey),
}

#[derive(Clone)]
/// Information about a player obtained during the authentication process.
pub struct AuthenticatedInfo {
    /// The username that was logged in.
    pub username: KString,
    /// The original network address the player connected from.
    pub address: PeerAddress,
}

/// A packet entry in the queue for processing on the Bevy side.
pub struct QueuedPacket {
    /// Pre-parsed ID of the packet.
    pub id: PacketId,
    /// Raw packet data.
    pub data: PacketWrapper,
    /// Timestamp of when the packet was first seen on the network thread.
    pub received_at: Instant,
    /// Identifier of the connection this packet came over.
    pub connection_key: ServerConnectionKey,
    /// Identifier of the stream this packet came over.
    pub stream_key: PacketStreamKey,
    /// The stream this packet came from, if it needs a response this is the stream to send it to.
    pub stream: Arc<PacketStream>,
}

/// A reference to a connected and bootstrapped player in the ECS.
pub struct ConnectedPlayer {
    /// Information acquired about the player during authentication.
    pub authenticated_info: AuthenticatedInfo,
    /// A key into the network thread's connection table.
    pub connection_key: ServerConnectionKey,
    /// Packet queue for Bevy system access.
    pub received_packet_queue: AsyncMutex<AsyncUnboundedReceiver<QueuedPacket>>,
    /// Main ordered stream for server requests to the client and their replies.
    pub main_s2c_stream: Arc<PacketStream>,
    /// Main ordered stream for client requests to the server and their replies.
    pub main_c2s_stream: Arc<PacketStream>,
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
    type Mutability = Mutable;

    fn register_component_hooks(hooks: &mut ComponentHooks) {
        hooks.on_insert(|mut world: DeferredWorld, context| {
            let entity = context.entity;
            let player = world.get::<ConnectedPlayer>(entity).unwrap();
            let addr = player.authenticated_info.address;
            let mut table = world.resource_mut::<ConnectedPlayersTable>();
            let old = table.players_by_address.insert(addr, entity);
            if let Some(old) = old {
                let new_nick = &world
                    .get::<ConnectedPlayer>(entity)
                    .unwrap()
                    .authenticated_info
                    .username;
                let old_nick = world
                    .get::<ConnectedPlayer>(old)
                    .map(|p| &p.authenticated_info.username as &str)
                    .unwrap_or("<missing nickname>");
                panic!(
                    "Attempting to insert a player `{new_nick}` with a duplicate peer address: {addr} of `{old_nick}`"
                );
            }
        });
        hooks.on_remove(|mut world: DeferredWorld, context| {
            let entity = context.entity;
            let player = world.get::<ConnectedPlayer>(entity).unwrap();
            let addr = player.authenticated_info.address;
            let mut table = world.resource_mut::<ConnectedPlayersTable>();
            table.players_by_address.remove(&addr);
        });
    }
}

/// A Bevy plugin registering the server-related entities.
pub struct NetworkServerPlugin;

impl Plugin for NetworkServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ServerPacketHandlerPlugin);
        app.world_mut().insert_resource(ConnectedPlayersTable::default());
    }
}

impl NetworkThreadState for NetworkThreadServerState {
    type StateCommand = NetworkThreadServerCommand;

    async fn on_command(&mut self, command: Self::StateCommand) {
        match command {
            NetworkThreadServerCommand::UpdateListeners(engine, return_channel) => {
                let new_listeners = engine.config().borrow().server.listen_addresses.clone();
                let _ = return_channel.send(self.update_listeners(engine, &new_listeners).await);
            }
            NetworkThreadServerCommand::CreateLocalConnection(engine, return_channel) => {
                let id = self.free_local_id;
                self.free_local_id += 1;
                let peer = PeerAddress::Local(id);
                let (duplex_a, duplex_b) = InProcessDuplex::new_pair();
                let server_connection = NetworkConnection::wrap_local(GameSide::Server, peer, duplex_a);
                let client_connection = NetworkConnection::wrap_local(GameSide::Client, peer, duplex_b);
                Self::accept_connection(engine, server_connection).await;
                let _ = return_channel.send(client_connection);
            }
            NetworkThreadServerCommand::InsertServerConnection(server_connection, sender) => {
                let key = self.connections.insert(*server_connection);
                let _ = sender.send(key);
            }
            NetworkThreadServerCommand::RemoveServerConnection(engine, key) => {
                if let Some(_conn) = self.connections.remove(key) {
                    let _ = engine.schedule_bevy(move |world| {
                        let mut query = world.query::<(Entity, &ConnectedPlayer)>();
                        let id = query
                            .iter(world)
                            .find(|(_, p)| p.connection_key == key)
                            .map(|(id, _)| id);
                        if let Some(id) = id {
                            world.despawn(id);
                        }
                        Ok(())
                    });
                }
            }
            NetworkThreadServerCommand::OpenNewStream(server_connection_key, sender) => {
                let Some(conn) = self.connections.get_mut(server_connection_key) else {
                    let _ = sender.send(Err(anyhow!("connection already dead")));
                    return;
                };
                let stream = conn.connection.open_stream().await;
                let stream = match stream {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = sender.send(Err(e));
                        return;
                    }
                };
                let stream = Arc::new(stream);
                let stream_key = conn.packet_streams.insert(stream.clone());
                let _ = sender.send(Ok((stream, stream_key)));
            }
            NetworkThreadServerCommand::InsertAcceptedStream(server_connection_key, packet_stream, sender) => {
                let Some(conn) = self.connections.get_mut(server_connection_key) else {
                    return;
                };
                let key = conn.packet_streams.insert(packet_stream);
                let _ = sender.send(key);
            }
            NetworkThreadServerCommand::RemoveStream(server_connection_key, packet_stream_key) => {
                let Some(conn) = self.connections.get_mut(server_connection_key) else {
                    return;
                };
                conn.packet_streams.remove(packet_stream_key);
            }
        }
    }

    async fn shutdown(&mut self) {
        // no-op
    }
}

impl NetworkThreadServerState {
    /// Begins listening on the configured endpoints, and starts looking for configuration changes.
    pub async fn new() -> Result<Self> {
        Ok(Self {
            free_local_id: default(),
            listeners: default(),
            connections: SlotMap::with_capacity_and_key(32),
        })
    }

    async fn accept_connection(engine: Arc<GameServer>, connection: NetworkConnection) {
        let address = connection.address();
        spawn_local(
            async move {
                if let Err(e) = Self::unauthenticated_connection(engine, connection).await {
                    warn!("Unauthenticated connection {} closed with error {}", address, e);
                }
            }
            .instrument(info_span!("unauth-connection", address = %address)),
        );
    }

    async fn unauthenticated_connection(engine: Arc<GameServer>, connection: NetworkConnection) -> Result<()> {
        let c2s_stream = connection.accept_stream().await?;
        loop {
            let packet = c2s_stream.recv_packet().await?;
            let packet_id = packet.parse_id(RPC_SERVER_UNAUTHENTICATED_READER_OPTIONS)?;
            match packet_id {
                rpc::PacketId::Echo => {
                    let reader = packet.parse_simple(RPC_SERVER_UNAUTHENTICATED_READER_OPTIONS)?;
                    let payload = reader.get()?.get_simple_payload();
                    let mut response = new_simple_packet_builder();
                    let mut root = response.init_root();
                    root.set_id(PacketId::Echo);
                    root.set_timestamp_ms(engine.network_thread.packet_timestamp());
                    root.set_simple_payload(payload);
                    c2s_stream.send_packet(response.into())?;
                }
                rpc::PacketId::GetServerMetadata => {
                    let reader = packet.parse_simple(RPC_SERVER_UNAUTHENTICATED_READER_OPTIONS)?;
                    let terminate_on_reply = reader.get()?.get_simple_payload() == 1;

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
                    let _ = c2s_stream.send_packet(response.into());

                    if terminate_on_reply {
                        c2s_stream.close();
                        connection.close();
                        return Ok(());
                    }
                }
                rpc::PacketId::Authenticate => {
                    let reader = packet
                        .parse_typed::<authentication_request::Owned>(RPC_SERVER_UNAUTHENTICATED_READER_OPTIONS)?;
                    let payload = reader.get()?.get_payload()?;
                    let mut response = new_packet_builder::<
                        result::Owned<authentication_acknowledgement::Owned, authentication_error::Owned>,
                    >();
                    let mut root = response.init_root();
                    root.set_id(rpc::PacketId::Authenticate);
                    root.set_timestamp_ms(engine.network_thread.packet_timestamp());
                    let result = root.init_payload();

                    let username = payload.get_username()?.to_str()?;

                    if username.is_empty() || !username.is_ascii() {
                        let mut err = result.init_err();
                        err.set_kind(authentication_error::Kind::InvalidUsername);
                        err.set_message("Username must be non-empty and ASCII only");
                        let _ = c2s_stream.send_packet(response.into());
                        c2s_stream.close();
                        connection.close();
                        return Ok(());
                    }

                    // TODO: verify identity

                    let address = connection.address();
                    let auth_info = AuthenticatedInfo {
                        username: KString::from_ref(username),
                        address,
                    };

                    let _ = result.init_ok();
                    let _ = c2s_stream.send_packet(response.into());
                    let s2c_stream = connection.open_stream().await?;

                    spawn_local(
                        Self::authenticated_connection(engine, connection, auth_info, c2s_stream, s2c_stream)
                            .instrument(info_span!("connection", address = %address, username = %username)),
                    );
                    return Ok(());
                }
                _ => {
                    return Err(anyhow!("Invalid packet ID {:?} received", packet_id));
                }
            }
        }
    }

    async fn authenticated_connection(
        engine: Arc<GameServer>,
        connection: NetworkConnection,
        auth_info: AuthenticatedInfo,
        main_c2s_stream: PacketStream,
        main_s2c_stream: PacketStream,
    ) {
        let mut stream_map = SlotMap::with_capacity_and_key(16);
        let main_c2s_stream = Arc::new(main_c2s_stream);
        let main_s2c_stream = Arc::new(main_s2c_stream);
        let main_c2s_key = stream_map.insert(main_c2s_stream.clone());
        let main_s2c_key = stream_map.insert(main_s2c_stream.clone());

        let (connection_key_tx, connection_key_rx) = async_oneshot_channel::<ServerConnectionKey>();
        let connection = Arc::new(connection);
        let server_connection = ServerConnection {
            authenticated_info: auth_info.clone(),
            packet_streams: stream_map,
            main_c2s_stream: main_c2s_key,
            main_s2c_stream: main_s2c_key,
            connection: Arc::clone(&connection),
        };
        engine
            .network_thread
            .send_command(NetworkThreadServerCommand::InsertServerConnection(
                Box::new(server_connection),
                connection_key_tx,
            ));
        let Ok(connection_key) = connection_key_rx.await else {
            return;
        };

        let (packet_tx, packet_rx) = async_unbounded_channel();

        let s2c_s = main_s2c_stream.clone();
        let c2s_s = main_c2s_stream.clone();
        let _ = engine
            .schedule_bevy(move |world| {
                world.spawn(ConnectedPlayer {
                    authenticated_info: auth_info,
                    connection_key,
                    received_packet_queue: AsyncMutex::new(packet_rx),
                    main_s2c_stream: s2c_s,
                    main_c2s_stream: c2s_s,
                });
                Ok(())
            })
            .async_wait()
            .await;

        let s2c_rx = spawn_local(Self::packet_stream_receiver(
            engine.network_thread.startup_time(),
            connection_key,
            main_s2c_key,
            main_s2c_stream.clone(),
            packet_tx.clone(),
        ));
        let c2s_rx = spawn_local(Self::packet_stream_receiver(
            engine.network_thread.startup_time(),
            connection_key,
            main_c2s_key,
            main_c2s_stream.clone(),
            packet_tx.clone(),
        ));
        spawn_local(Self::packet_stream_acceptor(
            engine.network_thread.startup_time(),
            Arc::clone(&engine),
            connection_key,
            connection,
            packet_tx,
        ));

        // If either side's main stream is closed, begin the connection shutdown process
        futures::future::select(s2c_rx, c2s_rx).await;

        engine
            .network_thread
            .send_command(NetworkThreadServerCommand::RemoveServerConnection(
                engine.clone(),
                connection_key,
            ));
    }

    async fn packet_stream_acceptor(
        startup_time: Instant,
        engine: Arc<GameServer>,
        connection_key: ServerConnectionKey,
        net_conn: Arc<NetworkConnection>,
        sender: AsyncUnboundedSender<QueuedPacket>,
    ) {
        let net_thread = &engine.network_thread;
        while let Ok(stream) = net_conn.accept_stream().await {
            let stream = Arc::new(stream);
            let (key_tx, key_rx) = async_oneshot_channel();
            net_thread.send_command(NetworkThreadServerCommand::InsertAcceptedStream(
                connection_key,
                Arc::clone(&stream),
                key_tx,
            ));
            let Ok(key) = key_rx.await else {
                break;
            };
            let receiver = Self::packet_stream_receiver(startup_time, connection_key, key, stream, sender.clone());
            let engine = Arc::clone(&engine);
            spawn_local(async move {
                receiver.await;
                engine
                    .network_thread
                    .send_command(NetworkThreadServerCommand::RemoveStream(connection_key, key));
            });
        }
    }

    async fn packet_stream_receiver(
        startup_time: Instant,
        connection_key: ServerConnectionKey,
        stream_key: PacketStreamKey,
        stream: Arc<PacketStream>,
        sender: AsyncUnboundedSender<QueuedPacket>,
    ) {
        while let Ok(packet) = stream.recv_packet().await {
            let id = match packet.parse_id(RPC_SERVER_READER_OPTIONS) {
                Ok(id) => id,
                Err(e) => {
                    warn!("Illegal packet received, closing stream: {}", e);
                    return;
                }
            };
            // handle Echo and Authenticate in the network thread, bypassing the engine
            if stream.initiating_side() == GameSide::Client {
                match id {
                    PacketId::Echo => {
                        let reader = packet.parse_simple(RPC_SERVER_UNAUTHENTICATED_READER_OPTIONS);
                        let payload = reader.and_then(|r| r.get().map(|m| m.get_simple_payload()));
                        let payload = match payload {
                            Ok(v) => v,
                            Err(e) => {
                                warn!("Could not read Echo payload, shutting down stream: {}", e);
                                return;
                            }
                        };
                        let mut response = new_simple_packet_builder();
                        let mut root = response.init_root();
                        root.set_id(PacketId::Echo);
                        root.set_timestamp_ms(startup_time.elapsed().as_millis() as u64);
                        root.set_simple_payload(payload);
                        if let Err(e) = stream.send_packet(response.into()) {
                            warn!("Could not send Echo reply, shutting down stream: {}", e);
                            return;
                        }
                        continue;
                    }
                    PacketId::Authenticate => {
                        let mut response = new_packet_builder::<
                            result::Owned<authentication_acknowledgement::Owned, authentication_error::Owned>,
                        >();
                        let mut root = response.init_root();
                        root.set_id(PacketId::Echo);
                        root.set_timestamp_ms(startup_time.elapsed().as_millis() as u64);
                        let result = root.init_payload();
                        let mut err = result.init_err();
                        err.set_kind(authentication_error::Kind::AlreadyAuthenticated);
                        err.set_message("You have already authenticated with this server");
                        if let Err(e) = stream.send_packet(response.into()) {
                            warn!("Could not send Echo reply, shutting down stream: {}", e);
                            return;
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            if sender
                .send(QueuedPacket {
                    id,
                    data: packet,
                    received_at: Instant::now(),
                    connection_key,
                    stream_key,
                    stream: stream.clone(),
                })
                .is_err()
            {
                return;
            }
        }
    }

    async fn update_listeners(&mut self, engine: Arc<GameServer>, new_listeners: &[SocketAddr]) -> Result<()> {
        let new_set: HashSet<SocketAddr> = HashSet::from_iter(new_listeners.iter().copied());
        let old_set: HashSet<SocketAddr> = HashSet::from_iter(self.listeners.keys().copied());

        let mut all_shutdowns = JoinSet::new();
        for &shutdown_addr in old_set.difference(&new_set) {
            let Some((endpoint, listener)) = self.listeners.remove(&shutdown_addr) else {
                continue;
            };
            endpoint.close(VarInt::default(), &[]);
            all_shutdowns.spawn_local(async move { endpoint.wait_idle().await });
            all_shutdowns.spawn_local(async move {
                let _ = listener.await;
            });
        }
        all_shutdowns.join_all().await;
        for &setup_addr in new_set.difference(&old_set) {
            let server_config = quinn_server_config();
            let socket = Socket::new(Domain::IPV6, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
            socket.set_only_v6(false)?;
            socket.bind(&setup_addr.into())?;
            let endpoint = quinn::Endpoint::new(
                EndpointConfig::default(),
                Some(server_config),
                socket.into(),
                quinn::default_runtime().unwrap(),
            )?;
            info!(
                "Listening for QUIC connections on {} (based on config {})",
                endpoint.local_addr()?,
                setup_addr
            );

            let endpoint2 = endpoint.clone();
            let listener = Self::remote_listener_task(endpoint, Arc::clone(&engine), setup_addr);
            let task = spawn_local(async move {
                if let Err(e) = listener.await {
                    error!("Listening for connections on {setup_addr} failed: {e}");
                }
            });
            self.listeners.insert(setup_addr, (endpoint2, task));
        }
        Ok(())
    }

    async fn remote_listener_task(endpoint: Endpoint, engine: Arc<GameServer>, server_addr: SocketAddr) -> Result<()> {
        while let Some(conn) = endpoint.accept().await {
            if !conn.remote_address_validated() {
                conn.retry()?;
                continue;
            }
            let conn_addr = conn.remote_address();
            let local_engine = Arc::clone(&engine);
            let peer_addr = PeerAddress::Network {
                local: server_addr,
                remote: conn_addr,
            };
            spawn_local(async move {
                let conn = match conn.await {
                    Ok(conn) => conn,
                    Err(e) => {
                        warn!(address = %conn_addr, "Client could not connect: {e}");
                        return;
                    }
                };
                info!(address = %conn_addr, "Accepting remote connection");
                let netconn = NetworkConnection::wrap_remote(GameSide::Server, peer_addr, conn);
                Self::accept_connection(local_engine, netconn).await;
            });
        }

        Ok(())
    }
}
