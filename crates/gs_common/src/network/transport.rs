//! Network transport implementations - local message passing for singleplayer&unit tests and QUIC for multiplayer

use std::ops::{Deref, DerefMut};

use capnp::message::ReaderOptions;
use capnp::Word;
use capnp_rpc::rpc_twoparty_capnp::Side;
use capnp_rpc::twoparty::VatNetwork;
use capnp_rpc::RpcSystem;
use gs_schemas::dependencies::itertools::Itertools;
use gs_schemas::schemas::{network_capnp as rpc, read_leb128, write_leb128, NetworkStreamHeader};
use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{Connection, RecvStream, SendStream};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::version::TLS13;
use rustls::{DigitallySignedStruct, Error, SignatureScheme, SupportedProtocolVersion};
use tokio_util::bytes::Bytes;

use crate::network::server::{NetworkThreadServerState, Server2ClientEndpoint};
use crate::network::PeerAddress;
use crate::prelude::*;
use crate::GameServer;

/// The insecure server TLS verifier that does not actually check anything at all.
#[derive(Debug)]
pub struct NoopServerTlsVerification(Arc<rustls::crypto::CryptoProvider>);

impl NoopServerTlsVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self(Arc::new(rustls::crypto::aws_lc_rs::default_provider())))
    }
}

impl ServerCertVerifier for NoopServerTlsVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Capnproto reader options for local connections
pub static RPC_LOCAL_READER_OPTIONS: ReaderOptions = ReaderOptions {
    traversal_limit_in_words: Some(1024 * 1024 * 1024),
    nesting_limit: 48,
};

/// Capnproto reader options for remote connections accepted on the server
pub static RPC_SERVER_READER_OPTIONS: ReaderOptions = ReaderOptions {
    traversal_limit_in_words: Some(32 * 1024 * 1024),
    nesting_limit: 48,
};

/// Capnproto reader options for remote server connections on the client
pub static RPC_CLIENT_READER_OPTIONS: ReaderOptions = ReaderOptions {
    traversal_limit_in_words: Some(256 * 1024 * 1024),
    nesting_limit: 48,
};

/// Size in bytes of the in-process client-server "socket" buffer.
const INPROCESS_SOCKET_BUFFER_SIZE: usize = 1024 * 1024;

/// A byte array over-aligned to Cap'n proto requirements.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct AlignedBytesMut {
    buffer: Vec<Word>,
    len: usize,
}

impl AlignedBytesMut {
    /// Allocates a mutable byte array object with the given length in bytes, and the alignment required by Cap'n proto.
    pub fn new(len: usize) -> Self {
        Self {
            buffer: Word::allocate_zeroed_vec(len.div_ceil(size_of::<Word>())),
            len,
        }
    }
}

impl Deref for AlignedBytesMut {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &Word::words_to_bytes(&self.buffer)[0..self.len]
    }
}

impl DerefMut for AlignedBytesMut {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut Word::words_to_bytes_mut(&mut self.buffer)[0..self.len]
    }
}

impl AsRef<[u8]> for AlignedBytesMut {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl AsMut<[u8]> for AlignedBytesMut {
    fn as_mut(&mut self) -> &mut [u8] {
        self.deref_mut()
    }
}

impl From<AlignedBytesMut> for Bytes {
    fn from(value: AlignedBytesMut) -> Self {
        Self::from_owner(value)
    }
}

/// A QUIC network stream for communication asynchronous to the main RPC channel.
#[derive(Clone)]
pub struct QuicStream {
    /// The stream header, determining its type.
    pub header: NetworkStreamHeader,
    /// The outgoing QUIC stream handle.
    pub tx: Arc<AsyncMutex<SendStream>>,
    /// The incoming QUIC stream handle.
    pub rx: Arc<AsyncMutex<RecvStream>>,
}

impl QuicStream {
    /// Opens a new stream on an existing QUIC connection.
    pub async fn open(connection: Connection, header: NetworkStreamHeader) -> Result<Self> {
        let (mut tx, rx) = connection.open_bi().await?;
        let header_bytes = header.write_to_bytes();
        let len_bytes = write_leb128(header_bytes.len() as u64);
        tx.write_all(&len_bytes).await?;
        tx.write_all(&header_bytes).await?;
        Ok(Self {
            header,
            tx: Arc::new(AsyncMutex::new(tx)),
            rx: Arc::new(AsyncMutex::new(rx)),
        })
    }

    /// Listens for a single new stream on an existing QUIC connection.
    pub async fn accept(connection: &Connection) -> Result<Self> {
        let (tx, mut rx) = connection.accept_bi().await?;
        let len_bytes: usize = read_leb128(&mut rx).await?.try_into()?;
        let mut header_bytes: Box<[u8]> = vec![0u8; len_bytes].into_boxed_slice();
        rx.read_exact(&mut header_bytes).await?;
        let header = NetworkStreamHeader::read_from_bytes(&header_bytes, RPC_SERVER_READER_OPTIONS)?;
        Ok(Self {
            header,
            tx: Arc::new(AsyncMutex::new(tx)),
            rx: Arc::new(AsyncMutex::new(rx)),
        })
    }
}

/// An in-process stream, modelling QUIC streams when using in-process communication.
#[derive(Clone)]
pub struct InProcessStream {
    /// The stream header, determining its type.
    pub header: NetworkStreamHeader,
    /// The sender "socket" for this stream side.
    pub tx: AsyncUnboundedSender<Bytes>,
    /// The receiver "socket" for this stream side.
    pub rx: Arc<AsyncMutex<AsyncUnboundedReceiver<Bytes>>>,
}

/// An abstraction over the two stream kinds (in-process and network).
#[derive(Clone)]
pub enum TransportStream {
    /// Remote QUIC stream.
    Network(QuicStream),
    /// Local in-process stream.
    Process(InProcessStream),
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
                rx: Arc::new(AsyncMutex::new(rx21)),
            },
            Self {
                header: header2,
                tx: tx21,
                rx: Arc::new(AsyncMutex::new(rx12)),
            },
        )
    }
}

impl From<InProcessStream> for TransportStream {
    fn from(value: InProcessStream) -> Self {
        TransportStream::Process(value)
    }
}

impl From<QuicStream> for TransportStream {
    fn from(value: QuicStream) -> Self {
        TransportStream::Network(value)
    }
}

impl TransportStream {
    /// Returns the header information set at stream opening time.
    pub fn header(&self) -> &NetworkStreamHeader {
        match self {
            TransportStream::Network(quic) => &quic.header,
            TransportStream::Process(ipc) => &ipc.header,
        }
    }

    /// Wraps the given message in a length-prefixed frame if needed and sends it over the stream.
    pub async fn send(&self, message: Bytes) -> Result<()> {
        match self {
            Self::Process(ipc) => {
                ipc.tx.send(message)?;
                Ok(())
            }
            Self::Network(quic) => {
                let len_bytes = write_leb128(message.len() as u64);
                let mut tx = quic.tx.lock().await;
                tx.write_all(&len_bytes).await?;
                tx.write_all(&message).await?;
                Ok(())
            }
        }
    }

    /// Reads an incoming message from this stream. Returns None if no further messages can be read.
    pub async fn recv(&self) -> Option<Bytes> {
        match self {
            Self::Process(ipc) => ipc.rx.lock().await.recv().await,
            Self::Network(quic) => {
                let mut rx = quic.rx.lock().await;
                let len = read_leb128(&mut *rx).await.ok()? as usize;
                let mut buf = AlignedBytesMut::new(len);
                rx.read_exact(&mut buf).await.ok()?;
                assert_eq!(len, buf.len());
                Some(buf.into())
            }
        }
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

/// Create a Future that will handle QUIC messages coming into a [`Server2ClientEndpoint`] and any child RPC objects on the given `server`&`id`.
pub fn create_quic_rpc_server(
    net_state: Rc<RefCell<NetworkThreadServerState>>,
    server: Arc<GameServer>,
    tx: SendStream,
    rx: RecvStream,
    id: PeerAddress,
) -> RpcSystem<Side> {
    let network = VatNetwork::new(rx, tx, Side::Server, RPC_SERVER_READER_OPTIONS);
    let bootstrap_object = Server2ClientEndpoint::new(net_state, server, id);
    let bootstrap_client: rpc::game_server::Client = capnp_rpc::new_client(bootstrap_object);
    RpcSystem::new(Box::new(network), Some(bootstrap_client.clone().client))
}

static ALPN_GEOSIA: &[&[u8]] = &[b"game-geosia/1"];
static TLS_PROTO_VERSIONS: &[&SupportedProtocolVersion] = &[&TLS13];

/// Makes a simple QUINN endpoint client config object.
pub fn quinn_client_config() -> quinn::ClientConfig {
    let mut crypto = rustls::ClientConfig::builder_with_protocol_versions(TLS_PROTO_VERSIONS)
        .dangerous()
        .with_custom_certificate_verifier(NoopServerTlsVerification::new())
        .with_no_client_auth();
    crypto.alpn_protocols = ALPN_GEOSIA.iter().map(|a| a.to_vec()).collect_vec();
    quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(crypto).unwrap()))
}

/// Makes a simple QUINN endpoint server config object.
pub fn quinn_server_config() -> quinn::ServerConfig {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let key = PrivateKeyDer::Pkcs8(cert.key_pair.serialize_der().into());
    let cert = cert.cert.into();

    let mut crypto = rustls::ServerConfig::builder_with_protocol_versions(TLS_PROTO_VERSIONS)
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .unwrap();
    crypto.alpn_protocols = ALPN_GEOSIA.iter().map(|a| a.to_vec()).collect_vec();
    quinn::ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(crypto).unwrap()))
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
        /// Constructs a dummy connection.
        pub fn new(server_addr: PeerAddress, server_rpc: rpc::game_server::Client) -> Self {
            Self {
                server_addr,
                server_rpc,
            }
        }

        /// Getter for the server address.
        pub fn server_addr(&self) -> PeerAddress {
            self.server_addr
        }

        /// Getter for the server RPC client object.
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
                        shutdown_rx.await.unwrap().unwrap();
                    })
                    .await;
            });
    }
}
