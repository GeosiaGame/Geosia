//! The network client thread implementation.

use bevy::log::{error, info};
use capnp::capability::Promise;
use capnp::Error;
use capnp_rpc::rpc_twoparty_capnp::Side;
use capnp_rpc::twoparty::{VatId, VatNetwork};
use capnp_rpc::{pry, Disconnector, RpcSystem};
use ocg_common::network::server::LocalConnectionPipe;
use ocg_common::network::thread::NetworkThreadState;
use ocg_common::network::transport::RPC_LOCAL_READER_OPTIONS;
use ocg_common::network::PeerAddress;
use ocg_common::prelude::*;
use ocg_schemas::schemas::network_capnp as rpc;
use ocg_schemas::schemas::network_capnp::authenticated_client_connection::{
    AddChatMessageParams, AddChatMessageResults, TerminateConnectionParams, TerminateConnectionResults,
};
use tokio::task::{spawn_local, JoinHandle};

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
}

/// Post-authentication
pub struct NetworkThreadClientAuthenticatedState {
    /// The state shared with [`NetworkThreadClientConnectingState`].
    connection: NetworkThreadClientConnectingState,
    /// The authenticated RPC object.
    server_auth_rpc: rpc::authenticated_server_connection::Client,
}

/// The network thread game client state, accessible from network functions.
#[derive(Default)]
pub enum NetworkThreadClientState {
    /// No peer connected
    #[default]
    Disconnected,
    /// Pre-authentication
    Connecting(NetworkThreadClientConnectingState),
    /// Post-authentication
    Authenticated(NetworkThreadClientAuthenticatedState),
}

impl NetworkThreadState for NetworkThreadClientState {
    async fn shutdown(this: Rc<RefCell<Self>>) {
        let disconnector = this
            .borrow_mut()
            .connecting_state_mut()
            .and_then(|s| s.rpc_disconnector.take());
        if let Some(disconnector) = disconnector {
            if let Err(e) = disconnector.await {
                error!("Error on client RPC shutdown: {e}");
            }
        }
        if let Some(s) = this.borrow_mut().connecting_state() {
            s.rpc_task.abort();
        }
    }
}

impl NetworkThreadClientState {
    fn connecting_state(&self) -> Option<&NetworkThreadClientConnectingState> {
        match self {
            NetworkThreadClientState::Disconnected => None,
            NetworkThreadClientState::Connecting(state) => Some(state),
            NetworkThreadClientState::Authenticated(NetworkThreadClientAuthenticatedState {
                connection: state,
                ..
            }) => Some(state),
        }
    }

    fn connecting_state_mut(&mut self) -> Option<&mut NetworkThreadClientConnectingState> {
        match self {
            NetworkThreadClientState::Disconnected => None,
            NetworkThreadClientState::Connecting(state) => Some(state),
            NetworkThreadClientState::Authenticated(NetworkThreadClientAuthenticatedState {
                connection: state,
                ..
            }) => Some(state),
        }
    }

    fn authenticated_state(&self) -> Option<&NetworkThreadClientAuthenticatedState> {
        match self {
            NetworkThreadClientState::Disconnected => None,
            NetworkThreadClientState::Connecting(_) => None,
            NetworkThreadClientState::Authenticated(state) => Some(state),
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
    pub async fn connect_locally(this: &Rc<RefCell<Self>>, pipe: LocalConnectionPipe) -> Result<()> {
        if let Some(existing_connection) = this.borrow().peer_address() {
            return Err(anyhow!("Already connected to {existing_connection:?}"));
        }

        let (rpc_system, connection) = create_local_rpc_client(pipe);
        let rpc_disconnector = rpc_system.get_disconnector();
        let rpc_task: JoinHandle<Result<()>> =
            spawn_local(async move { rpc_system.await.map_err(anyhow::Error::from) });

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

        *this.borrow_mut() = Self::Authenticated(NetworkThreadClientAuthenticatedState {
            connection: NetworkThreadClientConnectingState {
                server_address: connection.server_addr,
                server_rpc: connection,
                rpc_disconnector: Some(rpc_disconnector),
                rpc_task,
            },
            server_auth_rpc,
        });

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
pub fn create_local_rpc_client((id, pipe): LocalConnectionPipe) -> (RpcSystem<Side>, Client2ServerConnection) {
    let (read, write) = pipe.compat().split();
    let network = VatNetwork::new(read, write, Side::Client, RPC_LOCAL_READER_OPTIONS);
    let mut rpc_system = RpcSystem::new(Box::new(network), None);
    let server_object: rpc::game_server::Client = rpc_system.bootstrap(VatId::Server);
    (rpc_system, Client2ServerConnection::new(id, server_object))
}
