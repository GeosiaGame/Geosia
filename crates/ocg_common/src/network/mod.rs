//! The networking layer of the game.
#![deny(clippy::await_holding_refcell_ref, clippy::await_holding_lock)]
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

impl PeerAddress {
    /// Obtains the underlying socket address for a remote address, or None for other types.
    pub fn remote_addr(self) -> Option<SocketAddr> {
        match self {
            PeerAddress::Remote(r) => Some(r),
            _ => None,
        }
    }
}
