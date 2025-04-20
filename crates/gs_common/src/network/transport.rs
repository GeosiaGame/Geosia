//! Network transport implementations - local message passing for singleplayer&unit tests and QUIC for multiplayer

use capnp::message::{HeapAllocator, ReaderOptions, ReaderSegments, TypedReader};
use capnp::serialize::BufferSegments;
use capnp::traits::Owned;
use gs_schemas::GameSide;
use gs_schemas::dependencies::itertools::Itertools;
use gs_schemas::schemas::network_capnp::{PacketId, network_packet};
use gs_schemas::schemas::{AlignedBytesMut, CapnpBuilder, read_leb128, read_packet_id, write_leb128};
use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{Connection, RecvStream, SendStream, VarInt};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::version::TLS13;
use rustls::{DigitallySignedStruct, Error, SignatureScheme, SupportedProtocolVersion};
use thiserror::Error;
use tokio::task::spawn_local;
use tokio_util::bytes::Bytes;

use super::PeerAddress;
use crate::prelude::*;

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

/// Capnproto reader options for unauthenticated remote connections accepted on the server
pub static RPC_SERVER_UNAUTHENTICATED_READER_OPTIONS: ReaderOptions = ReaderOptions {
    traversal_limit_in_words: Some(1024),
    nesting_limit: 8,
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

/// The supported ALPN protocol identifiers for this game.
pub static ALPN_GEOSIA: &[&[u8]] = &[b"game-geosia/1"];
/// The supported TLS versions used by this game.
pub static TLS_PROTO_VERSIONS: &[&SupportedProtocolVersion] = &[&TLS13];

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

pub enum PacketSegments<'pkt> {
    Bytes(BufferSegments<&'pkt [u8]>),
    Builder(&'pkt capnp::message::Builder<HeapAllocator>),
}

impl<'pkt> ReaderSegments for PacketSegments<'pkt> {
    fn get_segment(&self, idx: u32) -> Option<&[u8]> {
        match self {
            PacketSegments::Bytes(v) => v.get_segment(idx),
            PacketSegments::Builder(v) => v.get_segment(idx),
        }
    }

    fn len(&self) -> usize {
        match self {
            PacketSegments::Bytes(v) => v.len(),
            PacketSegments::Builder(v) => v.len(),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            PacketSegments::Bytes(v) => v.is_empty(),
            PacketSegments::Builder(v) => v.is_empty(),
        }
    }
}

pub enum PacketWrapper {
    Serialized(Bytes),
    Capnp(CapnpBuilder),
}

impl PacketWrapper {
    // Mutable to trigger lazy compression in the future
    pub fn len(&mut self) -> usize {
        match self {
            Self::Serialized(bytes) => bytes.len(),
            Self::Capnp(builder) => capnp::serialize::compute_serialized_size_in_words(builder),
        }
    }

    pub fn clone_mut(&mut self) -> Self {
        match self {
            Self::Serialized(bytes) => Self::Serialized(bytes.clone()),
            Self::Capnp(builder) => {
                let bytes = capnp::serialize::write_message_to_words(builder);
                let bytes = Bytes::from_owner(bytes);
                *self = Self::Serialized(bytes.clone());
                Self::Serialized(bytes)
            }
        }
    }

    pub fn parse_id(&self, reader_options: ReaderOptions) -> capnp::Result<PacketId> {
        match self {
            Self::Serialized(bytes) => read_packet_id(&bytes, reader_options),
            Self::Capnp(builder) => {
                let reader = builder.get_root_as_reader::<network_packet::Reader<capnp::any_pointer::Owned>>()?;
                Ok(reader.get_id()?)
            }
        }
    }

    pub fn as_segments(&self, reader_options: ReaderOptions) -> capnp::Result<PacketSegments> {
        Ok(match self {
            PacketWrapper::Serialized(bytes) => {
                PacketSegments::Bytes(BufferSegments::new(&bytes as &[u8], reader_options)?)
            }
            PacketWrapper::Capnp(builder) => PacketSegments::Builder(builder),
        })
    }

    pub fn parse_simple(
        &self,
        reader_options: ReaderOptions,
    ) -> capnp::Result<TypedReader<PacketSegments, network_packet::Owned<capnp::any_pointer::Owned>>> {
        self.parse_typed(reader_options)
    }

    pub fn parse_typed<OwnedPayloadType: capnp::traits::Owned>(
        &self,
        reader_options: ReaderOptions,
    ) -> capnp::Result<TypedReader<PacketSegments, network_packet::Owned<OwnedPayloadType>>> {
        let reader = capnp::message::Reader::new(self.as_segments(reader_options)?, reader_options);
        Ok(reader.into_typed())
    }

    pub async fn write_all_quic_bytes(self, tx: &mut SendStream) -> Result<()> {
        match self {
            Self::Serialized(bytes) => {
                tx.write_all(&bytes).await?;
            }
            Self::Capnp(builder) => {
                capnp_futures::serialize::write_message(tx, builder).await?;
            }
        }
        Ok(())
    }
}

impl From<Bytes> for PacketWrapper {
    fn from(value: Bytes) -> Self {
        Self::Serialized(value)
    }
}

impl From<AlignedBytesMut> for PacketWrapper {
    fn from(value: AlignedBytesMut) -> Self {
        Self::Serialized(value.into())
    }
}

impl From<CapnpBuilder> for PacketWrapper {
    fn from(value: CapnpBuilder) -> Self {
        Self::Capnp(value)
    }
}

impl<O: Owned> From<capnp::message::TypedBuilder<O, HeapAllocator>> for PacketWrapper {
    fn from(value: capnp::message::TypedBuilder<O, HeapAllocator>) -> Self {
        Self::Capnp(value.into_inner())
    }
}

pub struct PacketStream {
    initiating_side: GameSide,
    close_request: AsyncWatchSender<bool>,
    closed: AsyncWatchReceiver<bool>,
    tx: AsyncUnboundedSender<PacketWrapper>,
    rx: AsyncMutex<AsyncUnboundedReceiver<PacketWrapper>>,
}

#[derive(Clone, Debug, Error)]
pub enum PacketSendRecvError {
    #[error("Other side of the stream was already closed")]
    StreamClosed,
}

impl PacketStream {
    pub async fn open_internal(connection: &InProcessDuplex, initiating_side: GameSide) -> Result<Self> {
        let (close_channel_tx, close_channel_rx) = async_watch_channel(false);
        let (a_tx, a_rx) = async_unbounded_channel();
        let (b_tx, b_rx) = async_unbounded_channel();
        let stream_a = Self {
            initiating_side,
            close_request: close_channel_tx.clone(),
            closed: close_channel_rx.clone(),
            tx: a_tx,
            rx: AsyncMutex::new(b_rx),
        };
        let stream_b = Self {
            initiating_side,
            close_request: close_channel_tx,
            closed: close_channel_rx,
            tx: b_tx,
            rx: AsyncMutex::new(a_rx),
        };
        connection.outgoing_streams.send(stream_b)?;
        Ok(stream_a)
    }

    pub async fn accept_internal(connection: &InProcessDuplex) -> Result<Self> {
        connection
            .incoming_streams
            .lock()
            .await
            .recv()
            .await
            .ok_or_else(|| anyhow!("internal socket closed"))
    }

    pub async fn open_quic(connection: Connection, initiating_side: GameSide) -> Result<Self> {
        let (raw_tx, raw_rx) = connection.open_bi().await?;
        Self::handle_quic(raw_tx, raw_rx, initiating_side).await
    }

    pub async fn accept_quic(connection: Connection, initiating_side: GameSide) -> Result<Self> {
        let (raw_tx, raw_rx) = connection.accept_bi().await?;
        Self::handle_quic(raw_tx, raw_rx, initiating_side).await
    }

    pub async fn recv_packet(&self) -> Result<PacketWrapper, PacketSendRecvError> {
        if *self.closed.borrow() {
            return Err(PacketSendRecvError::StreamClosed);
        }
        let mut closed = self.closed.clone();
        let mut rx = self.rx.lock().await;
        tokio::select! {
            _ = closed.wait_for(|&v| v) => {
                return Err(PacketSendRecvError::StreamClosed);
            }
            packet = rx.recv() => {
                match packet {
                    Some(packet) => Ok(packet),
                    None => {
                        self.close_request.send_replace(true);
                        return Err(PacketSendRecvError::StreamClosed);
                    }
                }
            }
        }
    }

    pub fn try_recv_packet(&mut self) -> Result<Option<PacketWrapper>, PacketSendRecvError> {
        if *self.closed.borrow() {
            return Err(PacketSendRecvError::StreamClosed);
        }
        match self.rx.get_mut().try_recv() {
            Ok(packet) => Ok(Some(packet)),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                self.close_request.send_replace(true);
                Err(PacketSendRecvError::StreamClosed)
            }
        }
    }

    pub fn send_packet(&self, packet: PacketWrapper) -> Result<(), PacketSendRecvError> {
        if *self.closed.borrow() {
            return Err(PacketSendRecvError::StreamClosed);
        }
        if self.tx.send(packet).is_err() {
            self.close_request.send_replace(true);
            return Err(PacketSendRecvError::StreamClosed);
        }
        Ok(())
    }

    pub fn initiating_side(&self) -> GameSide {
        self.initiating_side
    }

    pub fn close(&self) {
        self.close_request.send_replace(true);
    }

    async fn handle_quic(mut raw_tx: SendStream, mut raw_rx: RecvStream, initiating_side: GameSide) -> Result<Self> {
        let (close_channel_tx, mut close_channel_rx) = async_watch_channel(false);
        let close_channel_rx2 = close_channel_rx.clone();
        let (tx_incoming, rx_incoming) = async_unbounded_channel::<PacketWrapper>();
        let close_from_incoming = close_channel_tx.clone();
        let incoming_handler = spawn_local(async move {
            let _ = async move || -> Result<()> {
                loop {
                    let len = read_leb128(&mut raw_rx).await? as usize;
                    let mut buf = AlignedBytesMut::new(len);
                    raw_rx.read_exact(&mut buf).await?;
                    assert_eq!(len, buf.len());
                    tx_incoming.send(PacketWrapper::from(buf))?
                }
            }()
            .await;
            let _ = close_from_incoming.send(true);
        });
        let (tx_outgoing, mut rx_outgoing) = async_unbounded_channel::<PacketWrapper>();
        let close_from_outgoing = close_channel_tx.clone();

        let outgoing_handler = spawn_local(async move {
            let _ = async move || -> Result<()> {
                while let Some(mut packet) = rx_outgoing.recv().await {
                    let len_bytes = write_leb128(packet.len() as u64);
                    raw_tx.write_all(&len_bytes).await?;
                    packet.write_all_quic_bytes(&mut raw_tx).await?;
                }
                Ok(())
            }()
            .await;
            let _ = close_from_outgoing.send(true);
        });
        // cancellation handler
        let _ = spawn_local(async move {
            let _ = close_channel_rx.wait_for(|&v| v).await;
            incoming_handler.abort();
            outgoing_handler.abort();
            let _ = tokio::join!(incoming_handler, outgoing_handler);
        });

        Ok(Self {
            initiating_side,
            close_request: close_channel_tx,
            closed: close_channel_rx2,
            tx: tx_outgoing,
            rx: AsyncMutex::new(rx_incoming),
        })
    }
}

impl Drop for PacketStream {
    fn drop(&mut self) {
        self.close_request.send_replace(true);
    }
}

/// The bidirectional in-process "socket" used for client-integrated server communication, roughly equivalent to [`Connection`]
pub struct InProcessDuplex {
    /// Stream for accepting new in-process streams.
    pub incoming_streams: AsyncMutex<AsyncUnboundedReceiver<PacketStream>>,
    /// Stream for sending new in-process streams to the other side.
    pub outgoing_streams: AsyncUnboundedSender<PacketStream>,
}

impl InProcessDuplex {
    /// Makes a new pair of connected in-process "sockets".
    pub fn new_pair() -> (Self, Self) {
        let (streams12_tx, streams12_rx) = async_unbounded_channel();
        let (streams21_tx, streams21_rx) = async_unbounded_channel();
        (
            Self {
                incoming_streams: AsyncMutex::new(streams21_rx),
                outgoing_streams: streams12_tx,
            },
            Self {
                incoming_streams: AsyncMutex::new(streams12_rx),
                outgoing_streams: streams21_tx,
            },
        )
    }
}

enum NetworkConnectionSide {
    Local { duplex: InProcessDuplex },
    Remote { connection: Connection },
}

/// Abstraction over local and remote Connections
pub struct NetworkConnection {
    game_side: GameSide,
    address: PeerAddress,
    side: NetworkConnectionSide,
}

impl NetworkConnection {
    pub fn wrap_local(game_side: GameSide, address: PeerAddress, duplex: InProcessDuplex) -> Self {
        Self {
            game_side,
            address,
            side: NetworkConnectionSide::Local { duplex },
        }
    }

    pub fn wrap_remote(game_side: GameSide, address: PeerAddress, connection: Connection) -> Self {
        Self {
            game_side,
            address,
            side: NetworkConnectionSide::Remote { connection },
        }
    }

    pub fn address(&self) -> PeerAddress {
        self.address
    }

    pub fn close(&self) {
        match &self.side {
            NetworkConnectionSide::Local { duplex } => {}
            NetworkConnectionSide::Remote { connection } => {
                connection.close(VarInt::default(), &[]);
            }
        }
    }

    pub async fn open_stream(&self) -> Result<PacketStream> {
        match &self.side {
            NetworkConnectionSide::Local { duplex } => PacketStream::open_internal(duplex, self.game_side).await,
            NetworkConnectionSide::Remote { connection } => {
                PacketStream::open_quic(connection.clone(), self.game_side).await
            }
        }
    }

    pub async fn accept_stream(&self) -> Result<PacketStream> {
        match &self.side {
            NetworkConnectionSide::Local { duplex } => PacketStream::accept_internal(duplex).await,
            NetworkConnectionSide::Remote { connection } => {
                PacketStream::accept_quic(connection.clone(), self.game_side).await
            }
        }
    }
}
