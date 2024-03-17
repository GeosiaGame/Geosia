//! The networking layer of the game.

use std::net::SocketAddr;

pub mod client;
pub mod server;
pub mod thread;
pub mod transport;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
/// Address uniquely identifying a connected network or local peer (client or server on the other side of the connection).
pub enum PeerAddress {
    /// A local, in-process connection with a given ID to distinguish multiple local connections used in tests.
    Local(i32),
    /// A remote, over-the-network connection to a given peer at the specified IP address and port.
    Remote(SocketAddr),
}
