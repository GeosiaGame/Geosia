//! Game configuration handling

use std::net::SocketAddr;

use smart_default::SmartDefault;

use crate::concurrency::VersionedArc;

/// The server-specific configuration.
#[derive(Clone, Eq, PartialEq, Debug, SmartDefault)]
pub struct ServerConfig {
    /// The server name, as advertised to clients on the server list.
    #[default = "OCG Server"]
    pub server_name: String,
    /// The maximum number of players allowed to join the server.
    #[default = 4]
    pub max_players: u32,
    /// The network IPs and ports to listen on.
    #[default(default_listen_addresses())]
    pub listen_addresses: Vec<SocketAddr>,
}

/// All game configuration saved into the config file.
#[derive(Clone, Eq, PartialEq, Debug, SmartDefault)]
pub struct GameConfig {
    /// Server configuration.
    pub server: ServerConfig,
}

/// A versioned GameConfig handle, used as the primary way of accessing the game configuration.
pub type GameConfigHandle = VersionedArc<GameConfig>;

fn default_listen_addresses() -> Vec<SocketAddr> {
    vec!["0.0.0.0:28032".parse().unwrap(), "[::]:28032".parse().unwrap()]
}
