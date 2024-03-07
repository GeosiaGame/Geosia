//! The network client, connected to a server.

use ocg_schemas::schemas::network_capnp as rpc;

use crate::network::PeerAddress;

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
