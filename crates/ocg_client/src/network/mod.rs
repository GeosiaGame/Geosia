//! The network client thread implementation.
#![deny(clippy::await_holding_refcell_ref, clippy::await_holding_lock)]

use std::cell::RefCell;
use std::rc::Rc;

use ocg_common::network::thread::NetworkThreadState;
use ocg_common::network::PeerAddress;
use ocg_schemas::schemas::network_capnp as rpc;

/// The network thread game client state, accessible from network functions.
#[derive(Default)]
pub enum NetworkThreadClientState {
    /// No peer connected
    #[default]
    Disconnected,
    /// Pre-authentication
    Connecting {
        /// Address being connected to.
        server_address: PeerAddress,
        /// The RPC object for sending messages to.
        server_rpc: Client2ServerConnection,
    },
    /// Post-authentication
    Authenticated {
        /// Address of the server.
        server_address: PeerAddress,
        /// The unauthenticated root RPC object.
        server_rpc: Client2ServerConnection,
        /// The authenticated RPC object.
        server_auth_rpc: rpc::authenticated_server_connection::Client,
    },
}

impl NetworkThreadState for NetworkThreadClientState {
    async fn shutdown(_this: Rc<RefCell<Self>>) {
        //
    }
}

/// An unauthenticated RPC client<->server connection handler on the client side.
pub struct Client2ServerConnection {
    server_addr: PeerAddress,
    server_rpc: rpc::game_server::Client,
}

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
