//! Network transport implementations - local message passing for singleplayer&unit tests and QUIC for multiplayer

use std::sync::Arc;

use capnp::message::ReaderOptions;
use capnp_rpc::rpc_twoparty_capnp::Side;
use capnp_rpc::twoparty::{VatId, VatNetwork};
use capnp_rpc::RpcSystem;
use futures::AsyncReadExt;
use ocg_schemas::schemas::network_capnp as rpc;
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::network::client::Client2ServerConnection;
use crate::network::server::Server2ClientEndpoint;
use crate::network::PeerAddress;
use crate::GameServer;

static RPC_LOCAL_READER_OPTIONS: ReaderOptions = ReaderOptions {
    traversal_limit_in_words: Some(1024 * 1024 * 1024),
    nesting_limit: 128,
};

/// Create a Future that will handle in-memory messages coming into a [`Server2ClientEndpoint`] and any child RPC objects on the given `server`&`id`.
pub fn create_local_rpc_server(
    server: Arc<GameServer>,
    pipe: tokio::io::DuplexStream,
    id: PeerAddress,
) -> RpcSystem<Side> {
    let (read, write) = pipe.compat().split();
    let network = VatNetwork::new(read, write, Side::Server, RPC_LOCAL_READER_OPTIONS);
    let bootstrap_object = Server2ClientEndpoint::new(server, id);
    let bootstrap_client: rpc::game_server::Client = capnp_rpc::new_client(bootstrap_object);
    RpcSystem::new(Box::new(network), Some(bootstrap_client.clone().client))
}

/// Create a Future that will handle in-memory messages coming from a [`Server2ClientEndpoint`] and any child RPC objects on the given `server`&`id`.
pub fn create_local_rpc_client(
    pipe: tokio::io::DuplexStream,
    id: PeerAddress,
) -> (RpcSystem<Side>, Client2ServerConnection) {
    let (read, write) = pipe.compat().split();
    let network = VatNetwork::new(read, write, Side::Client, RPC_LOCAL_READER_OPTIONS);
    let mut rpc_system = RpcSystem::new(Box::new(network), None);
    let server_object: rpc::game_server::Client = rpc_system.bootstrap(VatId::Server);
    (rpc_system, Client2ServerConnection::new(id, server_object))
}

#[cfg(test)]
mod test {
    use crate::network::transport::*;
    use crate::prelude::*;
    use crate::GameServerControlCommand;

    #[test]
    fn test_server_metadata() {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .unwrap()
            .block_on(async move {
                tokio::task::LocalSet::new()
                    .run_until(async move {
                        let addr = PeerAddress::Local(0);
                        let (cpipe, spipe) = tokio::io::duplex(1024 * 1024);
                        let server = GameServer::new_test();
                        let rpc_server = create_local_rpc_server(server.server.clone(), spipe, addr);
                        let s_disconnector = rpc_server.get_disconnector();
                        let rpc_server = tokio::task::spawn_local(rpc_server);
                        let (rpc_client, c_server) = create_local_rpc_client(cpipe, addr);
                        let c_disconnector = rpc_client.get_disconnector();
                        let rpc_client = tokio::task::spawn_local(rpc_client);

                        let mut ping_request = c_server.rpc().ping_request();
                        ping_request.get().set_input(123);
                        let ping_reply = ping_request.send().promise.await.expect("ping request failed");
                        let ping_reply = ping_reply.get().expect("ping reply get failed");
                        assert_eq!(123, ping_reply.get_output());

                        let metadata = c_server
                            .rpc()
                            .get_server_metadata_request()
                            .send()
                            .promise
                            .await
                            .expect("metadata request failed");
                        let metadata = metadata.get().expect("metadata get failed");
                        eprintln!(
                            "Metadata: {:?}",
                            metadata.get_metadata().expect("metadata nested get failed")
                        );

                        // Disconnect the RPC endpoint, then await graceful shutdown.
                        let _ = s_disconnector.await;
                        let _ = c_disconnector.await;
                        let _ = rpc_server.await;
                        let _ = rpc_client.await;
                        let (shutdown_tx, shutdown_rx) = async_oneshot_channel();
                        server
                            .control_channel
                            .send(GameServerControlCommand::Shutdown(shutdown_tx))
                            .unwrap();
                        shutdown_rx.await.unwrap();
                    })
                    .await;
            });
    }
}
