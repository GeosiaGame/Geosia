//! The network client thread implementation.

use bevy::log::{error, info, warn};
use bevy::prelude::*;
use capnp::capability::Promise;
use capnp::message::TypedReader;
use capnp::Error;
use capnp_rpc::rpc_twoparty_capnp::Side;
use capnp_rpc::twoparty::{VatId, VatNetwork};
use capnp_rpc::{pry, Disconnector, RpcSystem};
use ocg_common::network::server::LocalConnectionPipe;
use ocg_common::network::thread::NetworkThreadState;
use ocg_common::network::transport::{InProcessStream, RPC_LOCAL_READER_OPTIONS};
use ocg_common::network::PeerAddress;
use ocg_common::prelude::*;
use ocg_common::voxel::plugin::VoxelUniverse;
use ocg_schemas::coordinates::{AbsChunkPos, RelChunkPos};
use ocg_schemas::dependencies::itertools::iproduct;
use ocg_schemas::schemas::network_capnp as rpc;
use ocg_schemas::schemas::network_capnp::authenticated_client_connection::{
    AddChatMessageParams, AddChatMessageResults, TerminateConnectionParams, TerminateConnectionResults,
};
use ocg_schemas::schemas::NetworkStreamHeader;
use ocg_schemas::voxel::voxeltypes::{BlockEntry, EMPTY_BLOCK_NAME};
use tokio::task::{spawn_local, JoinHandle};
use tokio_util::bytes::Bytes;
use tracing::Instrument;

use crate::voxel::meshgen::mesh_from_chunk;
use crate::voxel::{ClientChunk, ClientChunkGroup};
use crate::{ClientData, GameControlChannel};

/// Pre-authentication
pub struct NetworkThreadClientConnectingState {
    /// Address being connected to.
    server_address: PeerAddress,
    /// The RPC object for sending messages to.
    server_rpc: Client2ServerConnection,
    /// Object to shut down the RPC system.
    rpc_disconnector: Option<Disconnector<Side>>,
    /// The RPC system task.
    rpc_task: JoinHandle<Result<()>>,
    /// The async stream system task.
    stream_task: JoinHandle<Result<()>>,
    /// The async stream creation channel.
    // TODO: use a trait object here, so we can use sockets too.
    _stream_sender: AsyncUnboundedSender<InProcessStream>,
}

/// Post-authentication
pub struct NetworkThreadClientAuthenticatedState {
    /// The state shared with [`NetworkThreadClientConnectingState`].
    connection: NetworkThreadClientConnectingState,
    /// The authenticated RPC object.
    server_auth_rpc: rpc::authenticated_server_connection::Client,
}

/// The state machine for [`NetworkThreadClientState`].
#[derive(Default)]
pub enum NetworkThreadClientStateVariant {
    /// No peer connected
    #[default]
    Disconnected,
    /// Pre-authentication
    Connecting(NetworkThreadClientConnectingState),
    /// Post-authentication
    Authenticated(NetworkThreadClientAuthenticatedState),
}

/// The network thread game client state, accessible from network functions.
pub struct NetworkThreadClientState {
    /// Channel for communicating with the client bevy instance
    game_control: GameControlChannel,
    /// The current variant storage.
    variant: NetworkThreadClientStateVariant,
}

impl NetworkThreadState for NetworkThreadClientState {
    async fn shutdown(this: Rc<RefCell<Self>>) {
        let disconnector = this
            .borrow_mut()
            .connecting_state_mut()
            .and_then(|s| s.rpc_disconnector.take());
        if let Some(disconnector) = disconnector {
            if let Err(e) = disconnector.await {
                error!("Error on client RPC disconnect: {e}");
            }
        }
        if let Some(s) = this.borrow_mut().connecting_state() {
            s.rpc_task.abort();
            s.stream_task.abort();
        }
    }
}

impl NetworkThreadClientState {
    /// Constructor.
    pub fn new(game_control: GameControlChannel) -> Self {
        Self {
            game_control,
            variant: Default::default(),
        }
    }

    fn connecting_state(&self) -> Option<&NetworkThreadClientConnectingState> {
        match &self.variant {
            NetworkThreadClientStateVariant::Disconnected => None,
            NetworkThreadClientStateVariant::Connecting(state) => Some(state),
            NetworkThreadClientStateVariant::Authenticated(NetworkThreadClientAuthenticatedState {
                connection: state,
                ..
            }) => Some(state),
        }
    }

    fn connecting_state_mut(&mut self) -> Option<&mut NetworkThreadClientConnectingState> {
        match &mut self.variant {
            NetworkThreadClientStateVariant::Disconnected => None,
            NetworkThreadClientStateVariant::Connecting(state) => Some(state),
            NetworkThreadClientStateVariant::Authenticated(NetworkThreadClientAuthenticatedState {
                connection: state,
                ..
            }) => Some(state),
        }
    }

    fn authenticated_state(&self) -> Option<&NetworkThreadClientAuthenticatedState> {
        match &self.variant {
            NetworkThreadClientStateVariant::Disconnected => None,
            NetworkThreadClientStateVariant::Connecting(_) => None,
            NetworkThreadClientStateVariant::Authenticated(state) => Some(state),
        }
    }

    /// Returns the address of the connected/ing peer.
    pub fn peer_address(&self) -> Option<PeerAddress> {
        self.connecting_state().map(|s| s.server_address)
    }

    /// Returns the server RPC object, if connected.
    pub fn server_rpc(&self) -> Option<&Client2ServerConnection> {
        self.connecting_state().map(|s| &s.server_rpc)
    }

    /// Returns the authenticated server RPC object, if authenticated.
    pub fn server_auth_rpc(&self) -> Option<&rpc::authenticated_server_connection::Client> {
        self.authenticated_state().map(|s| &s.server_auth_rpc)
    }

    /// Initiates a new local connection on the given pipe.
    pub async fn connect_locally(this: &Rc<RefCell<Self>>, (address, pipe): LocalConnectionPipe) -> Result<()> {
        if let Some(existing_connection) = this.borrow().peer_address() {
            return Err(anyhow!("Already connected to {existing_connection:?}"));
        }

        let (rpc_system, connection) = create_local_rpc_client(address, pipe.rpc_pipe);
        let rpc_disconnector = rpc_system.get_disconnector();
        let rpc_task: JoinHandle<Result<()>> = spawn_local(
            async move { rpc_system.await.map_err(anyhow::Error::from) }
                .instrument(tracing::info_span!("client-rpc", address = ?address)),
        );

        // Authenticate
        let mut auth_request = connection.server_rpc.authenticate_request();
        {
            let mut builder = auth_request.get();
            builder.set_username("LocalPlayer");
            let auth_rpc = AuthenticatedClientConnectionImpl {};
            builder.set_connection(capnp_rpc::new_client(auth_rpc));
        }
        let auth_response = auth_request
            .send()
            .promise
            .await
            .context("RPC failure to authenticate with integrated server")?;
        let auth_response = auth_response.get().context("Invalid authentication response")?;
        let auth_response = auth_response
            .get_conn()
            .context("Missing authentication response")?
            .which()
            .context("Illegal authentication response")?;
        let server_auth_rpc = match auth_response {
            ocg_schemas::schemas::game_types_capnp::result::Which::Ok(ok) => ok?,
            ocg_schemas::schemas::game_types_capnp::result::Which::Err(err) => {
                let err = err?;
                let msg = err.get_message()?.to_str()?;
                bail!("Integrated server authentication error {msg}");
            }
        };

        info!(
            "Authenticated to the integrated server via {:?}",
            connection.server_addr
        );

        let stream_task: JoinHandle<Result<()>> = spawn_local(
            Self::local_stream_acceptor(Rc::clone(this), pipe.incoming_streams)
                .instrument(tracing::info_span!("client-stream", address = ?address)),
        );

        this.borrow_mut().variant =
            NetworkThreadClientStateVariant::Authenticated(NetworkThreadClientAuthenticatedState {
                connection: NetworkThreadClientConnectingState {
                    server_address: connection.server_addr,
                    server_rpc: connection,
                    rpc_disconnector: Some(rpc_disconnector),
                    rpc_task,
                    stream_task,
                    _stream_sender: pipe.outgoing_streams,
                },
                server_auth_rpc,
            });

        Ok(())
    }

    async fn local_stream_acceptor(
        this: Rc<RefCell<Self>>,
        mut incoming_streams: AsyncUnboundedReceiver<InProcessStream>,
    ) -> Result<()> {
        while let Some(stream) = incoming_streams.recv().await {
            use rpc::stream_header::StandardTypes;
            match stream.header {
                NetworkStreamHeader::Standard(StandardTypes::ChunkData) => {
                    spawn_local(Self::chunk_data_handler(Rc::clone(&this), stream));
                }
                unknown => {
                    warn!("Unrecognized network stream type {unknown:?}");
                }
            }
        }
        Ok(())
    }

    async fn chunk_data_handler(this: Rc<RefCell<Self>>, stream: InProcessStream) {
        let InProcessStream { mut rx, .. } = stream;
        while let Some(raw_packet) = rx.recv().await {
            if let Err(e) = this.borrow().handle_chunk_packet_net(raw_packet) {
                error!("Error while processing chunk data packet: {e:?}");
            }
        }
    }

    fn handle_chunk_packet_net(&self, raw_packet: Bytes) -> Result<()> {
        self.game_control
            .send(Box::new(move |world| {
                if let Err(e) = Self::handle_chunk_packet_engine(world, raw_packet) {
                    error!("Could not handle chunk packet: {e:?}");
                }
            }))
            .ok()
            .context("Game control socket closed")?;

        Ok(())
    }

    fn handle_chunk_packet_engine(world: &mut World, raw_packet: Bytes) -> Result<()> {
        let mut slice = &raw_packet as &[u8];
        let msg = capnp::serialize::read_message_from_flat_slice(&mut slice, RPC_LOCAL_READER_OPTIONS)?;
        let typed_reader = TypedReader::<_, rpc::chunk_data_stream_packet::Owned>::new(msg);
        let root = typed_reader.get()?;
        let cpos_r = root.reborrow().get_position()?;
        let pos = AbsChunkPos::new(cpos_r.get_x(), cpos_r.get_y(), cpos_r.get_z());
        let data_r = root.reborrow().get_data()?;

        {
            let universe = world.get_resource_mut::<VoxelUniverse<ClientData>>();
            let Some(universe) = universe else {
                warn!("Missing voxel universe while processing chunk data packet, did the game already shut down?");
                return Ok(());
            };
            // TODO: actually add the chunk to the universe chunk loader
            let block_registry = Arc::clone(&universe.block_registry);
            let empty = block_registry
                .lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref())
                .context("no empty block")?
                .0;

            // Just add the chunk mesh directly right now for testing
            let mut test_chunks = ClientChunkGroup::new();
            for (cx, cy, cz) in iproduct!(-1..=1, -1..=1, -1..=1) {
                let cpos = RelChunkPos::new(cx, cy, cz) + pos;
                let chunk = ClientChunk::new(BlockEntry::new(empty, 0), Default::default());
                test_chunks.chunks.insert(cpos, chunk);
            }
            let mid_chunk = ClientChunk::read_full(&data_r, Default::default())?;
            test_chunks.chunks.insert(pos, mid_chunk);

            let white_material = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
                base_color: Color::GRAY,
                ..default()
            });

            for (pos, _) in test_chunks.chunks.iter() {
                let chunks = &test_chunks.get_neighborhood_around(*pos).transpose_option();
                if let Some(chunks) = chunks {
                    let chunk_mesh = mesh_from_chunk(&block_registry, chunks).unwrap();

                    let mesh = world.resource_mut::<Assets<Mesh>>().add(chunk_mesh);

                    world.spawn(PbrBundle {
                        mesh,
                        material: white_material.clone(),
                        transform: Transform::from_xyz(0.0, 0.0, 0.0),
                        ..default()
                    });
                }
            }
        }

        info!("Received chunk packet at position {pos}");
        Ok(())
    }
}

/// An unauthenticated RPC client<->server connection handler on the client side.
pub struct Client2ServerConnection {
    server_addr: PeerAddress,
    server_rpc: rpc::game_server::Client,
}

struct AuthenticatedClientConnectionImpl {}

impl Client2ServerConnection {
    /// Constructor.
    pub fn new(server_addr: PeerAddress, server_rpc: rpc::game_server::Client) -> Self {
        Self {
            server_addr,
            server_rpc,
        }
    }

    /// The RPC instance for sending messages to the connected server.
    pub fn rpc(&self) -> &rpc::game_server::Client {
        &self.server_rpc
    }

    /// The address of the connected server.
    pub fn server_addr(&self) -> PeerAddress {
        self.server_addr
    }
}

impl ocg_schemas::schemas::network_capnp::authenticated_client_connection::Server
    for AuthenticatedClientConnectionImpl
{
    fn terminate_connection(
        &mut self,
        _: TerminateConnectionParams,
        _: TerminateConnectionResults,
    ) -> Promise<(), Error> {
        //
        Promise::ok(())
    }

    fn add_chat_message(&mut self, params: AddChatMessageParams, _: AddChatMessageResults) -> Promise<(), Error> {
        let params = pry!(params.get());
        let chat_text = pry!(params.get_text());
        let chat_text = pry!(chat_text.to_str());
        info!("Client received chat message: {chat_text}");
        Promise::ok(())
    }
}

/// Create a Future that will handle in-memory messages coming from a [`Server2ClientEndpoint`] and any child RPC objects on the given `server`&`id`.
pub fn create_local_rpc_client(
    id: PeerAddress,
    pipe: tokio::io::DuplexStream,
) -> (RpcSystem<Side>, Client2ServerConnection) {
    let (read, write) = pipe.compat().split();
    let network = VatNetwork::new(read, write, Side::Client, RPC_LOCAL_READER_OPTIONS);
    let mut rpc_system = RpcSystem::new(Box::new(network), None);
    let server_object: rpc::game_server::Client = rpc_system.bootstrap(VatId::Server);
    (rpc_system, Client2ServerConnection::new(id, server_object))
}
