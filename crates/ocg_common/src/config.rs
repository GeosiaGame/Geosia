//! Game configuration handling

use std::net::SocketAddr;
use std::sync::Arc;

use smart_default::SmartDefault;

use crate::prelude::{async_watch_channel, AsyncWatchReceiver, AsyncWatchSender};

/// The server-specific configuration.
#[derive(Clone, Eq, PartialEq, Debug, SmartDefault)]
pub struct ServerConfig {
    /// The server title, as advertised to clients on the server list.
    #[default = "OCG Server"]
    pub server_title: String,
    /// The server subtitle, as advertised to clients on the server list.
    #[default = ""]
    pub server_subtitle: String,
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

/// A GameConfig handle that can listen to changes, used as the primary way of accessing the game configuration.
pub type GameConfigHandle = Arc<(AsyncWatchSender<GameConfig>, AsyncWatchReceiver<GameConfig>)>;

impl GameConfig {
    /// Constructs a [`GameConfigHandle`] from this [`GameConfig`]
    pub fn new_handle(self) -> GameConfigHandle {
        GameConfigHandle::new(async_watch_channel(self))
    }
}

fn default_listen_addresses() -> Vec<SocketAddr> {
    vec!["0.0.0.0:28032".parse().unwrap(), "[::]:28032".parse().unwrap()]
}
