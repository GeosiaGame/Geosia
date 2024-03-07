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
use crate::GameServer;

/// An unauthenticated RPC client<->server connection handler on the server side.
pub struct Server2ClientEndpoint {
    server: Arc<GameServer>,
    peer: PeerAddress,
}

/// An authenticated RPC client<->server connection handler on the server side.
pub struct AuthenticatedServer2ClientEndpoint {
    server: Arc<GameServer>,
    peer: PeerAddress,
    username: KString,
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
        let title = "OCG Server";
        let subtitle = "Subtitles to be implemented!";
        let mut meta = results.get().init_metadata();
        let mut ver = meta.reborrow().init_server_version();
        ver.set_major(0);
        ver.set_minor(0);
        ver.set_patch(1);
        ver.set_build("todo");
        ver.set_prerelease("");

        meta.set_title(title);
        meta.set_subtitle(subtitle);
        meta.set_player_count(0);
        meta.set_player_limit(12);
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
            server: self.server.clone(),
            peer: self.peer,
            username,
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
