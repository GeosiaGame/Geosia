//! The network client thread implementation.

pub mod client_entity_syncer;
pub mod client_packet_handlers;

use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};
use std::time::Instant;

use gs_common::network::PeerAddress;
use gs_common::network::server::{PacketStreamKey, QueuedPacket};
use gs_common::network::thread::{NetworkThread, NetworkThreadState};
use gs_common::network::transport::{
    NetworkConnection, PacketStream, PacketWrapper, RPC_CLIENT_READER_OPTIONS,
    RPC_SERVER_UNAUTHENTICATED_READER_OPTIONS, quinn_client_config,
};
use gs_common::prelude::rpc::{PacketId, authentication_acknowledgement, authentication_error, authentication_request};
use gs_schemas::GameSide;
use gs_schemas::player::{
    TEST_ALICE_CHARACTER_UUID, TEST_ALICE_PLAYER_UUID, TEST_BOB_CHARACTER_UUID, TEST_BOB_PLAYER_UUID,
    nonregistered_player_url,
};
use gs_schemas::schemas::{
    CapnpExt, game_types_capnp, network_capnp as rpc, new_packet_builder, new_simple_packet_builder,
};
use quinn::{Endpoint, EndpointConfig};
use slotmap::SlotMap;
use socket2::{Domain, Socket};
use tokio::task::spawn_local;

use crate::GameControlChannel;
use crate::prelude::*;

#[derive(Resource)]
/// Bevy resource for processing incoming network packets.
pub struct AuthenticatedNetworkClient {
    packet_reference_timestamp: Instant,
    /// The main stream for client-to-server requests.
    pub main_c2s_stream: Arc<PacketStream>,
    /// Key for the main packet stream.
    pub main_c2s_stream_key: PacketStreamKey,
    /// The packet queue for processing on the bevy side.
    pub packet_queue: AsyncUnboundedReceiver<QueuedPacket>,
}

impl AuthenticatedNetworkClient {
    /// Returns the reference timestamp used for packet timestamp calculations.
    pub fn startup_time(&self) -> Instant {
        self.packet_reference_timestamp
    }

    /// Computes the timestamp for a packet that's ready to be sent.
    pub fn packet_timestamp(&self) -> u64 {
        self.packet_reference_timestamp.elapsed().as_millis() as u64
    }
}

/// State of the network thread with a working connection to the server
pub struct NetworkThreadClientConnectedState {
    /// Connection to the server.
    server_connection: Arc<NetworkConnection>,
    /// Main stream for sending client requests to the server.
    _main_c2s_stream_key: PacketStreamKey,
    /// Stream storage map.
    streams: SlotMap<PacketStreamKey, Arc<PacketStream>>,
    /// Sender of messages to the bevy packet processing queue.
    _packet_queue_sender: AsyncUnboundedSender<QueuedPacket>,
}

/// Command type for the client network thread
pub enum NetworkThreadClientCommand {
    /// Connects to an in-process server.
    ConnectLocally(
        Arc<ClientNetworkThread>,
        GameControlChannel,
        NetworkConnection,
        AsyncOneshotSender<Result<()>>,
    ),
    /// Connects to a remote server.
    ConnectRemotely(
        Arc<ClientNetworkThread>,
        GameControlChannel,
        SocketAddr,
        AsyncOneshotSender<Result<()>>,
    ),
    /// Opens a new stream on the open connection, use an [`AsyncResult`] to read the result.
    OpenNewStream(AsyncOneshotSender<Result<(Arc<PacketStream>, PacketStreamKey)>>),
    /// Inserts a new stream into the table and returns the key for it.
    InsertAcceptedStream(Arc<PacketStream>, AsyncOneshotSender<PacketStreamKey>),
    /// Deregisters a (closed) stream from the stream table.
    RemoveStream(PacketStreamKey),
}

/// The state machine for [`NetworkThreadClientState`].
#[derive(Default)]
pub enum NetworkThreadClientStateVariant {
    /// No peer connected
    #[default]
    Disconnected,
    /// Connection in progress
    Connecting(PeerAddress),
    /// Authenticated connection
    Connected(NetworkThreadClientConnectedState),
}

/// The network thread game client state, accessible from network functions.
pub struct NetworkThreadClientState {
    /// The current variant storage.
    variant: NetworkThreadClientStateVariant,
}

/// Convenience alias for the client-sided network thread implementation.
pub type ClientNetworkThread = NetworkThread<NetworkThreadClientState>;

impl NetworkThreadState for NetworkThreadClientState {
    type StateCommand = NetworkThreadClientCommand;

    async fn shutdown(&mut self) {
        self.variant = NetworkThreadClientStateVariant::Disconnected;
    }

    async fn on_command(&mut self, command: Self::StateCommand) {
        match command {
            NetworkThreadClientCommand::ConnectLocally(network_thread, game_channel, network_connection, sender) => {
                let result = self
                    .authenticate(network_thread, game_channel, network_connection)
                    .await;
                let _ = sender.send(result);
            }
            NetworkThreadClientCommand::ConnectRemotely(network_thread, game_channel, socket_addr, sender) => {
                let result = self
                    .connect_and_authenticate(network_thread, game_channel, socket_addr)
                    .await;
                let _ = sender.send(result);
            }
            NetworkThreadClientCommand::OpenNewStream(sender) => {
                let result = self.open_stream().await;
                let _ = sender.send(result);
            }
            NetworkThreadClientCommand::InsertAcceptedStream(packet_stream, sender) => {
                if let Some(state) = self.connected_state_mut() {
                    let key = state.streams.insert(packet_stream);
                    let _ = sender.send(key);
                }
            }
            NetworkThreadClientCommand::RemoveStream(packet_stream_key) => {
                if let Some(state) = self.connected_state_mut() {
                    state.streams.remove(packet_stream_key);
                }
            }
        }
    }
}

impl NetworkThreadClientState {
    /// Constructor.
    pub async fn new() -> Result<Self> {
        Ok(Self {
            variant: NetworkThreadClientStateVariant::default(),
        })
    }

    /// True if there is currently no connection or an attempt at a connection.
    pub fn is_disconnected(&self) -> bool {
        match &self.variant {
            NetworkThreadClientStateVariant::Disconnected => true,
            NetworkThreadClientStateVariant::Connecting(..) => false,
            NetworkThreadClientStateVariant::Connected(..) => false,
        }
    }

    /// True if there is currently an attempt at a connection.
    pub fn is_connecting(&self) -> bool {
        match &self.variant {
            NetworkThreadClientStateVariant::Disconnected => false,
            NetworkThreadClientStateVariant::Connecting(..) => true,
            NetworkThreadClientStateVariant::Connected(..) => false,
        }
    }

    /// True if there is currently an authenticated connection.
    pub fn is_connected(&self) -> bool {
        match &self.variant {
            NetworkThreadClientStateVariant::Disconnected => false,
            NetworkThreadClientStateVariant::Connecting(..) => false,
            NetworkThreadClientStateVariant::Connected(..) => true,
        }
    }

    /// Returns the address of the server connected/ing to.
    pub fn server_address(&self) -> Option<PeerAddress> {
        match &self.variant {
            NetworkThreadClientStateVariant::Disconnected => None,
            NetworkThreadClientStateVariant::Connecting(addr) => Some(*addr),
            NetworkThreadClientStateVariant::Connected(state) => Some(state.server_connection.address()),
        }
    }

    #[allow(dead_code)]
    fn connected_state(&self) -> Option<&NetworkThreadClientConnectedState> {
        match &self.variant {
            NetworkThreadClientStateVariant::Disconnected => None,
            NetworkThreadClientStateVariant::Connecting(..) => None,
            NetworkThreadClientStateVariant::Connected(state) => Some(state),
        }
    }

    fn connected_state_mut(&mut self) -> Option<&mut NetworkThreadClientConnectedState> {
        match &mut self.variant {
            NetworkThreadClientStateVariant::Disconnected => None,
            NetworkThreadClientStateVariant::Connecting(..) => None,
            NetworkThreadClientStateVariant::Connected(state) => Some(state),
        }
    }

    async fn open_stream(&mut self) -> Result<(Arc<PacketStream>, PacketStreamKey)> {
        let state = self.connected_state_mut().context("Not connected to server")?;
        let stream = state.server_connection.open_stream().await?;
        let stream = Arc::new(stream);
        let key = state.streams.insert(Arc::clone(&stream));
        Ok((stream, key))
    }

    /// Initiates a new authenticated connection on the given [`NetworkConnection`].
    pub async fn authenticate(
        &mut self,
        net_thread: Arc<ClientNetworkThread>,
        game_channel: GameControlChannel,
        net_conn: NetworkConnection,
    ) -> Result<()> {
        if self.is_connected() {
            return Err(anyhow!("Already connected!"));
        }

        let net_conn = Arc::new(net_conn);
        let main_c2s_stream = Arc::new(net_conn.open_stream().await?);
        let mut streams = SlotMap::with_capacity_and_key(16);
        let main_c2s_stream_key = streams.insert(Arc::clone(&main_c2s_stream));
        let (packets_tx, packets_rx) = async_unbounded_channel();
        let next_state = NetworkThreadClientConnectedState {
            server_connection: Arc::clone(&net_conn),
            _main_c2s_stream_key: main_c2s_stream_key,
            streams,
            _packet_queue_sender: packets_tx.clone(),
        };

        // Authenticate
        let mut auth_request = new_packet_builder::<authentication_request::Owned>();
        {
            let mut builder = auth_request.init_root();
            builder.set_id(rpc::PacketId::Authenticate);
            builder.set_timestamp_ms(net_thread.packet_timestamp());
            let mut payload = builder.init_payload();
            if next_state.server_connection.is_local() {
                payload.set_player_url(nonregistered_player_url(TEST_ALICE_PLAYER_UUID));
                payload.set_player_display_name("Alice");
                TEST_ALICE_CHARACTER_UUID
                    .0
                    .get()
                    .write_to_message(&mut payload.reborrow().init_character_id());
                payload.set_character_display_name("Alice Zero");
            } else {
                payload.set_player_url(nonregistered_player_url(TEST_BOB_PLAYER_UUID));
                payload.set_player_display_name("Bob");
                TEST_BOB_CHARACTER_UUID
                    .0
                    .get()
                    .write_to_message(&mut payload.reborrow().init_character_id());
                payload.set_character_display_name("Bob One");
            }
        }
        let auth_request = PacketWrapper::from(auth_request);
        main_c2s_stream.send_packet(auth_request)?;
        let auth_response = main_c2s_stream.recv_packet().await?;
        let root = auth_response.parse_typed::<game_types_capnp::result::Owned<authentication_acknowledgement::Owned, authentication_error::Owned>>(RPC_CLIENT_READER_OPTIONS)?;
        let root = root.get()?;
        let rid = root.get_id()?;
        if rid != PacketId::Authenticate {
            return Err(anyhow!("Received packet id was {}, not authenticate", rid));
        }
        match root.get_payload()?.which()? {
            gs_schemas::schemas::game_types_capnp::result::Which::Ok(_ok) => {
                // continue on
            }
            gs_schemas::schemas::game_types_capnp::result::Which::Err(err) => {
                let err = err?;
                let kind = err.get_kind()?;
                let msg = err.get_message()?.to_str()?;
                return Err(anyhow!("Server authentication error {kind:?}: {msg}"));
            }
        }

        info!(
            "Authenticated to the server at {}",
            next_state.server_connection.address()
        );

        self.variant = NetworkThreadClientStateVariant::Connected(next_state);

        let bevy_access = AuthenticatedNetworkClient {
            packet_reference_timestamp: net_thread.startup_time(),
            main_c2s_stream: Arc::clone(&main_c2s_stream),
            main_c2s_stream_key,
            packet_queue: packets_rx,
        };
        let _ = game_channel.send(Box::new(move |world| {
            world.insert_resource(bevy_access);
        }));

        spawn_local(Self::packet_stream_receiver(
            net_thread.startup_time(),
            main_c2s_stream_key,
            main_c2s_stream,
            packets_tx.clone(),
        ));

        spawn_local(Self::packet_stream_acceptor(
            net_thread.startup_time(),
            net_thread,
            net_conn,
            packets_tx,
        ));

        Ok(())
    }

    /// Initiates a new remote connection to the given address.
    pub async fn connect_and_authenticate(
        &mut self,
        net_thread: Arc<NetworkThread<NetworkThreadClientState>>,
        game_channel: GameControlChannel,
        remote_address: SocketAddr,
    ) -> Result<()> {
        if !self.is_disconnected() {
            return Err(anyhow!("Already connecting/ed to {:?}", self.server_address()));
        }

        let bind_addr = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0);

        self.variant = NetworkThreadClientStateVariant::Connecting(PeerAddress::Network {
            local: bind_addr.into(),
            remote: remote_address,
        });

        let socket = Socket::new(Domain::IPV6, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
        socket.set_only_v6(false)?;
        socket.bind(&bind_addr.into())?;
        let local_addr = socket.local_addr()?;
        let endpoint = Endpoint::new(
            EndpointConfig::default(),
            None,
            socket.into(),
            quinn::default_runtime().unwrap(),
        )?;
        let quic_connection = endpoint
            .connect_with(quinn_client_config(), remote_address, "example.com")?
            .await?;

        let address = PeerAddress::Network {
            local: local_addr.as_socket().context("Obtaining local socket address")?,
            remote: quic_connection.remote_address(),
        };
        self.variant = NetworkThreadClientStateVariant::Connecting(address);

        let net_conn = NetworkConnection::wrap_remote(GameSide::Client, address, endpoint, quic_connection);

        self.authenticate(net_thread, game_channel, net_conn).await
    }

    async fn packet_stream_acceptor(
        startup_time: Instant,
        net_thread: Arc<ClientNetworkThread>,
        net_conn: Arc<NetworkConnection>,
        sender: AsyncUnboundedSender<QueuedPacket>,
    ) {
        while let Ok(stream) = net_conn.accept_stream().await {
            let stream = Arc::new(stream);
            let (key_tx, key_rx) = async_oneshot_channel();
            net_thread.send_command(NetworkThreadClientCommand::InsertAcceptedStream(
                Arc::clone(&stream),
                key_tx,
            ));
            let Ok(key) = key_rx.await else {
                break;
            };
            let receiver = Self::packet_stream_receiver(startup_time, key, stream, sender.clone());
            let net_thread = Arc::clone(&net_thread);
            spawn_local(async move {
                receiver.await;
                net_thread.send_command(NetworkThreadClientCommand::RemoveStream(key));
            });
        }
    }

    async fn packet_stream_receiver(
        startup_time: Instant,
        stream_key: PacketStreamKey,
        stream: Arc<PacketStream>,
        sender: AsyncUnboundedSender<QueuedPacket>,
    ) {
        while let Ok(packet) = stream.recv_packet().await {
            let id = match packet.parse_id(RPC_CLIENT_READER_OPTIONS) {
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
                        warn!("Received unexpected Authenticate packet");
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
                    connection_key: default(),
                    stream_key,
                    stream: Arc::clone(&stream),
                })
                .is_err()
            {
                return;
            }
        }
    }
}
