//! Network transport implementations - local message passing for singleplayer&unit tests and QUIC for multiplayer

use capnp::message::ReaderOptions;
use capnp_rpc::rpc_twoparty_capnp::Side;
use capnp_rpc::twoparty::VatNetwork;
use capnp_rpc::RpcSystem;
use gs_schemas::schemas::{network_capnp as rpc, NetworkStreamHeader};
use tokio_util::bytes::Bytes;

use crate::network::server::{NetworkThreadServerState, Server2ClientEndpoint};
use crate::network::PeerAddress;
use crate::prelude::*;
use crate::GameServer;

/// Capnproto reader options for local connections
pub static RPC_LOCAL_READER_OPTIONS: ReaderOptions = ReaderOptions {
    traversal_limit_in_words: Some(1024 * 1024 * 1024),
    nesting_limit: 128,
};

/// Size in bytes of the in-process client-server "socket" buffer.
const INPROCESS_SOCKET_BUFFER_SIZE: usize = 1024 * 1024;

/// An in-process stream, modelling QUIC streams when using in-process communication.
pub struct InProcessStream {
    /// The stream header, determining its type.
    pub header: NetworkStreamHeader,
    /// The sender "socket" for this stream side.
    pub tx: AsyncUnboundedSender<Bytes>,
    /// The receiver "socket" for this stream side.
    pub rx: AsyncUnboundedReceiver<Bytes>,
}

impl InProcessStream {
    /// Constructs a new, pre-connected bidirectional stream for in-process communication.
    pub fn new_pair(header: NetworkStreamHeader) -> (Self, Self) {
        let (tx12, rx12) = async_unbounded_channel();
        let (tx21, rx21) = async_unbounded_channel();
        let header2 = header.clone();
        (
            Self {
                header,
                tx: tx12,
                rx: rx21,
            },
            Self {
                header: header2,
                tx: tx21,
                rx: rx12,
            },
        )
    }
}

/// The bidirectional in-process "socket" used for client-integrated server communication
pub struct InProcessDuplex {
    /// The main RPC pipe for hosting the Cap'n proto RPC interfaces (corresponding to the initial QUIC stream)
    pub rpc_pipe: tokio::io::DuplexStream,
    /// Stream for accepting new in-process streams.
    pub incoming_streams: AsyncUnboundedReceiver<InProcessStream>,
    /// Stream for sending new in-process streams to the other side.
    pub outgoing_streams: AsyncUnboundedSender<InProcessStream>,
}

impl InProcessDuplex {
    /// Makes a new pair of connected in-process "sockets".
    pub fn new_pair() -> (Self, Self) {
        let (duplex1, duplex2) = tokio::io::duplex(INPROCESS_SOCKET_BUFFER_SIZE);
        let (streams12_tx, streams12_rx) = async_unbounded_channel();
        let (streams21_tx, streams21_rx) = async_unbounded_channel();
        (
            Self {
                rpc_pipe: duplex1,
                incoming_streams: streams21_rx,
                outgoing_streams: streams12_tx,
            },
            Self {
                rpc_pipe: duplex2,
                incoming_streams: streams12_rx,
                outgoing_streams: streams21_tx,
            },
        )
    }
}

/// Create a Future that will handle in-memory messages coming into a [`Server2ClientEndpoint`] and any child RPC objects on the given `server`&`id`.
pub fn create_local_rpc_server(
    net_state: Rc<RefCell<NetworkThreadServerState>>,
    server: Arc<GameServer>,
    pipe: tokio::io::DuplexStream,
    id: PeerAddress,
) -> RpcSystem<Side> {
    let (read, write) = pipe.compat().split();
    let network = VatNetwork::new(read, write, Side::Server, RPC_LOCAL_READER_OPTIONS);
    let bootstrap_object = Server2ClientEndpoint::new(net_state, server, id);
    let bootstrap_client: rpc::game_server::Client = capnp_rpc::new_client(bootstrap_object);
    RpcSystem::new(Box::new(network), Some(bootstrap_client.clone().client))
}

/// Unit test utilities
#[cfg(test)]
pub mod test {

    use capnp_rpc::twoparty::VatId;

    use crate::network::transport::*;
    use crate::GameServerControlCommand;

    /// A dummy client implementation for basic RPC testing
    pub struct TestClient2ServerConnection {
        server_addr: PeerAddress,
        server_rpc: rpc::game_server::Client,
    }

    impl TestClient2ServerConnection {
        pub fn new(server_addr: PeerAddress, server_rpc: rpc::game_server::Client) -> Self {
            Self {
                server_addr,
                server_rpc,
            }
        }

        pub fn server_addr(&self) -> PeerAddress {
            self.server_addr
        }

        pub fn server_rpc(&self) -> &rpc::game_server::Client {
            &self.server_rpc
        }
    }

    /// Create a Future that will handle in-memory messages coming from a [`Server2ClientEndpoint`] and any child RPC objects on the given `server`&`id`.
    pub fn create_test_rpc_client(
        pipe: tokio::io::DuplexStream,
        id: PeerAddress,
    ) -> (RpcSystem<Side>, TestClient2ServerConnection) {
        let (read, write) = pipe.compat().split();
        let network = VatNetwork::new(read, write, Side::Client, RPC_LOCAL_READER_OPTIONS);
        let mut rpc_system = RpcSystem::new(Box::new(network), None);
        let server_object: rpc::game_server::Client = rpc_system.bootstrap(VatId::Server);
        (rpc_system, TestClient2ServerConnection::new(id, server_object))
    }

    #[test]
    fn test_server_metadata() {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .unwrap()
            .block_on(async move {
                tokio::task::LocalSet::new()
                    .run_until(async move {
                        let dummy_state = Rc::new(RefCell::new(NetworkThreadServerState::new()));
                        let addr = PeerAddress::Local(0);
                        let (cpipe, spipe) = tokio::io::duplex(1024 * 1024);
                        let server = GameServer::new_test();
                        let rpc_server = create_local_rpc_server(dummy_state, server.clone(), spipe, addr);
                        let s_disconnector = rpc_server.get_disconnector();
                        let rpc_server = tokio::task::spawn_local(rpc_server);
                        let (rpc_client, c_server) = create_test_rpc_client(cpipe, addr);
                        let c_disconnector = rpc_client.get_disconnector();
                        let rpc_client = tokio::task::spawn_local(rpc_client);

                        let mut ping_request = c_server.server_rpc.ping_request();
                        ping_request.get().set_input(123);
                        let ping_reply = ping_request.send().promise.await.expect("ping request failed");
                        let ping_reply = ping_reply.get().expect("ping reply get failed");
                        assert_eq!(123, ping_reply.get_output());

                        let metadata = c_server
                            .server_rpc
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
