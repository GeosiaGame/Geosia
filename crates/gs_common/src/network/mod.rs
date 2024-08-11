//! The networking layer of the game.

use std::fmt::{Display, Formatter};
use std::net::SocketAddr;

pub mod server;
pub mod thread;
pub mod transport;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
/// Address uniquely identifying a connected network or local peer (client or server on the other side of the connection).
pub enum PeerAddress {
    /// A local, in-process connection with a given ID to distinguish multiple local connections used in tests.
    Local(i32),
    /// A remote, over-the-network connection to a given peer at the specified IP address and port, connected to a local IP and port.
    Network {
        /// The local network interface address and port bound for this peer
        local: SocketAddr,
        /// The peer's address and port
        remote: SocketAddr,
    },
}

impl PeerAddress {
    /// Obtains the underlying socket address for a remote address, or None for other types.
    pub fn remote_addr(self) -> Option<SocketAddr> {
        match self {
            PeerAddress::Network { remote, .. } => Some(remote),
            _ => None,
        }
    }
}

impl Display for PeerAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(loc) => write!(f, "Local:{loc}"),
            Self::Network { local, remote } => write!(f, "Remote:({local} -> {remote})"),
        }
    }
}
