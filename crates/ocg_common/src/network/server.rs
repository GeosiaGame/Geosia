//! The network server protocol implementation, hosting a game for zero or more clients.

use std::sync::Arc;

use capnp_rpc::pry;
use ocg_schemas::dependencies::capnp::capability::Promise;
use ocg_schemas::dependencies::capnp::Error;
use ocg_schemas::dependencies::kstring::KString;
use ocg_schemas::schemas::network_capnp as rpc;
use ocg_schemas::schemas::network_capnp::authenticated_server_connection::{
    BootstrapGameDataParams, BootstrapGameDataResults, SendChatMessageParams, SendChatMessageResults,
};

use crate::network::PeerAddress;
use crate::{
    GameServer, GAME_VERSION_BUILD, GAME_VERSION_MAJOR, GAME_VERSION_MINOR, GAME_VERSION_PATCH, GAME_VERSION_PRERELEASE,
};

/// An unauthenticated RPC client<->server connection handler on the server side.
pub struct Server2ClientEndpoint {
    server: Arc<GameServer>,
    peer: PeerAddress,
}

/// An authenticated RPC client<->server connection handler on the server side.
pub struct AuthenticatedServer2ClientEndpoint {
    _server: Arc<GameServer>,
    _peer: PeerAddress,
    _username: KString,
    connection: rpc::authenticated_client_connection::Client,
}

impl Server2ClientEndpoint {
    /// Constructor.
    pub fn new(server: Arc<GameServer>, peer: PeerAddress) -> Self {
        Self { server, peer }
    }

    /// The server this endpoint is associated with.
    pub fn server(&self) -> &Arc<GameServer> {
        &self.server
    }

    /// The peer address this endpoint is connected to.
    pub fn peer(&self) -> PeerAddress {
        self.peer
    }
}

impl rpc::game_server::Server for Server2ClientEndpoint {
    fn get_server_metadata(
        &mut self,
        _params: rpc::game_server::GetServerMetadataParams,
        mut results: rpc::game_server::GetServerMetadataResults,
    ) -> Promise<(), Error> {
        let config = self.server.config.peek();
        let mut meta = results.get().init_metadata();
        let mut ver = meta.reborrow().init_server_version();
        ver.set_major(GAME_VERSION_MAJOR);
        ver.set_minor(GAME_VERSION_MINOR);
        ver.set_patch(GAME_VERSION_PATCH);
        ver.set_build(GAME_VERSION_BUILD);
        ver.set_prerelease(GAME_VERSION_PRERELEASE);

        meta.set_title(&config.server.server_title);
        meta.set_subtitle(&config.server.server_subtitle);
        meta.set_player_count(0);
        meta.set_player_limit(config.server.max_players as i32);
        Promise::ok(())
    }

    fn ping(
        &mut self,
        params: rpc::game_server::PingParams,
        mut results: rpc::game_server::PingResults,
    ) -> Promise<(), Error> {
        let input = pry!(params.get()).get_input();
        results.get().set_output(input);
        Promise::ok(())
    }

    fn authenticate(
        &mut self,
        params: rpc::game_server::AuthenticateParams,
        mut results: rpc::game_server::AuthenticateResults,
    ) -> Promise<(), Error> {
        let params = pry!(params.get());
        let username = KString::from_ref(pry!(pry!(params.get_username()).to_str()));
        let connection = pry!(params.get_connection());

        // TODO: validate username

        let client = AuthenticatedServer2ClientEndpoint {
            _server: self.server.clone(),
            _peer: self.peer,
            _username: username,
            connection,
        };

        let mut result = results.get().init_conn();
        let np_client: rpc::authenticated_server_connection::Client = capnp_rpc::new_client(client);
        pry!(result.set_ok(np_client));

        Promise::ok(())
    }
}

impl AuthenticatedServer2ClientEndpoint {
    /// The RPC instance for sending messages to the connected client.
    pub fn rpc(&self) -> &rpc::authenticated_client_connection::Client {
        &self.connection
    }
}

impl rpc::authenticated_server_connection::Server for AuthenticatedServer2ClientEndpoint {
    fn bootstrap_game_data(&mut self, _: BootstrapGameDataParams, _: BootstrapGameDataResults) -> Promise<(), Error> {
        todo!()
    }

    fn send_chat_message(&mut self, _: SendChatMessageParams, _: SendChatMessageResults) -> Promise<(), Error> {
        todo!()
    }
}
