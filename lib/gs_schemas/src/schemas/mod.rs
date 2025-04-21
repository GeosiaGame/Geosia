//! Generic code for a stable forwards/backwards compatible serialization framework for game objects.
//!
//! Based on capnproto: https://capnproto.org/language.html, https://docs.rs/capnp/latest/capnp/

use std::convert::Infallible;
use std::fmt::{Debug, Display};
use std::ops::{Deref, DerefMut};

use bevy_math::{I64Vec2, I64Vec3, prelude::*};
use bytes::Bytes;
use capnp::message::{HeapAllocator, ReaderOptions, TypedBuilder, TypedReader};
use capnp::serialize::BufferSegments;
use capnp::{Word, word};
use futures::{AsyncRead, AsyncReadExt};
use smallvec::SmallVec;
use uuid::Uuid;

use crate::registry::RegistryName;

/// Common game object types.
#[allow(missing_docs, clippy::all)] // Auto-generated
pub mod game_types_capnp {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/capnp-generated/game_types_capnp.rs"
    ));

    impl SimpleResult {
        /// Discriminant-based accessor for simple packets.
        pub fn as_i32(self) -> i32 {
            match self {
                Self::Err => 0,
                Self::Ok => 1,
            }
        }

        /// Discriminant-based accessor for simple packets.
        pub fn from_i32(v: i32) -> Self {
            if v == 0 { Self::Err } else { Self::Ok }
        }
    }
}

/// Voxel mesh encoding for resource bundles.
#[allow(missing_docs, clippy::all)] // Auto-generated
pub mod voxel_mesh_capnp {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/capnp-generated/voxel_mesh_capnp.rs"
    ));
}

/// The RPC network protocol.
#[allow(missing_docs, clippy::all)] // Auto-generated
pub mod network_capnp {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/capnp-generated/network_capnp.rs"));
}

impl Display for network_capnp::PacketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let raw_id = *self as i32;
        write!(f, "{self:?} ({raw_id} = 0x{raw_id:02x})")
    }
}

/// Zero-filled capnp [`Word`] for buffer initialization.
pub const CAPNP_ZERO_WORD: Word = word(0, 0, 0, 0, 0, 0, 0, 0);

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

    /// Allocates a mutable byte array object with the given capacity for growth in bytes, and the alignment required by Cap'n proto.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(cap.div_ceil(size_of::<Word>())),
            len: 0,
        }
    }

    /// Shrinks the spare capacity of the inner vector as much as possible.
    pub fn shrink_to_fit(&mut self) {
        self.buffer.shrink_to_fit();
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
    fn from(mut value: AlignedBytesMut) -> Self {
        value.shrink_to_fit();
        Self::from_owner(value)
    }
}

impl capnp::io::Write for AlignedBytesMut {
    fn write_all(&mut self, buf: &[u8]) -> capnp::Result<()> {
        let old_len = self.len;
        let new_len = old_len + buf.len();
        let old_words = old_len.div_ceil(size_of::<Word>());
        let new_words = new_len.div_ceil(size_of::<Word>());
        if new_words > old_words {
            self.buffer
                .extend(std::iter::repeat_n(CAPNP_ZERO_WORD, new_words - old_words));
        }
        self.deref_mut()[old_len..new_len].copy_from_slice(buf);
        Ok(())
    }
}

/// Computes the LEB128 representation of the given input.
pub fn write_leb128(mut value: u64) -> SmallVec<[u8; 10]> {
    let mut v = SmallVec::new();
    while value > 0x7F {
        v.push((0x80 | (value & 0x7F)) as u8);
        value >>= 7;
    }
    v.push((value & 0x7F) as u8);
    v
}

/// Reads a LEB128-encoded number from the given input stream.
pub async fn read_leb128(mut input: impl AsyncRead + Unpin) -> Result<u64, std::io::Error> {
    let mut out = 0u64;
    let mut shift = 0;
    let mut buf = [0u8];
    for _iter in 0..10 {
        input.read_exact(&mut buf).await?;
        out |= ((buf[0] & 0x7F) as u64) << shift;
        let has_more = (buf[0] & 0x80) != 0;
        if has_more {
            shift += 7;
        } else {
            break;
        }
    }
    Ok(out)
}

/// Helpers for (de)serializing a simple type to/from capnp messages.
pub trait CapnpExt: Sized {
    /// The corresponding capnp-generated `Builder` type
    type Builder<'a>;
    /// The corresponding capnp-generated `Reader` type
    type Reader<'a>;
    /// The Result error type when reading a capnp message.
    type ReaderError;

    /// Serializes a UUID into a capnp message.
    fn write_to_message(&self, builder: &mut Self::Builder<'_>);
    /// Deserializes a UUID from a capnp message.
    fn read_from_message(reader: &Self::Reader<'_>) -> Result<Self, Self::ReaderError>;
}

impl CapnpExt for Uuid {
    type Builder<'a> = game_types_capnp::uuid::Builder<'a>;
    type Reader<'a> = game_types_capnp::uuid::Reader<'a>;
    type ReaderError = Infallible;

    fn write_to_message(&self, builder: &mut Self::Builder<'_>) {
        let (high, low) = self.as_u64_pair();
        builder.set_low(low);
        builder.set_high(high);
    }

    fn read_from_message(reader: &Self::Reader<'_>) -> Result<Self, Self::ReaderError> {
        let (high, low) = (reader.get_high(), reader.get_low());
        Ok(Self::from_u64_pair(high, low))
    }
}

impl CapnpExt for IVec2 {
    type Builder<'a> = game_types_capnp::i_vec2::Builder<'a>;
    type Reader<'a> = game_types_capnp::i_vec2::Reader<'a>;
    type ReaderError = Infallible;

    fn write_to_message(&self, builder: &mut Self::Builder<'_>) {
        builder.set_x(self.x);
        builder.set_y(self.y);
    }

    fn read_from_message(reader: &Self::Reader<'_>) -> Result<Self, Self::ReaderError> {
        Ok(Self::new(reader.get_x(), reader.get_y()))
    }
}

impl CapnpExt for IVec3 {
    type Builder<'a> = game_types_capnp::i_vec3::Builder<'a>;
    type Reader<'a> = game_types_capnp::i_vec3::Reader<'a>;
    type ReaderError = Infallible;

    fn write_to_message(&self, builder: &mut Self::Builder<'_>) {
        builder.set_x(self.x);
        builder.set_y(self.y);
        builder.set_z(self.z);
    }

    fn read_from_message(reader: &Self::Reader<'_>) -> Result<Self, Self::ReaderError> {
        Ok(Self::new(reader.get_x(), reader.get_y(), reader.get_z()))
    }
}

impl CapnpExt for I64Vec2 {
    type Builder<'a> = game_types_capnp::i64_vec2::Builder<'a>;
    type Reader<'a> = game_types_capnp::i64_vec2::Reader<'a>;
    type ReaderError = Infallible;

    fn write_to_message(&self, builder: &mut Self::Builder<'_>) {
        builder.set_x(self.x);
        builder.set_y(self.y);
    }

    fn read_from_message(reader: &Self::Reader<'_>) -> Result<Self, Self::ReaderError> {
        Ok(Self::new(reader.get_x(), reader.get_y()))
    }
}

impl CapnpExt for I64Vec3 {
    type Builder<'a> = game_types_capnp::i64_vec3::Builder<'a>;
    type Reader<'a> = game_types_capnp::i64_vec3::Reader<'a>;
    type ReaderError = Infallible;

    fn write_to_message(&self, builder: &mut Self::Builder<'_>) {
        builder.set_x(self.x);
        builder.set_y(self.y);
        builder.set_z(self.z);
    }

    fn read_from_message(reader: &Self::Reader<'_>) -> Result<Self, Self::ReaderError> {
        Ok(Self::new(reader.get_x(), reader.get_y(), reader.get_z()))
    }
}

impl CapnpExt for Vec2 {
    type Builder<'a> = game_types_capnp::vec2::Builder<'a>;
    type Reader<'a> = game_types_capnp::vec2::Reader<'a>;
    type ReaderError = Infallible;

    fn write_to_message(&self, builder: &mut Self::Builder<'_>) {
        builder.set_x(self.x);
        builder.set_y(self.y);
    }

    fn read_from_message(reader: &Self::Reader<'_>) -> Result<Self, Self::ReaderError> {
        Ok(Self::new(reader.get_x(), reader.get_y()))
    }
}

impl CapnpExt for Vec3 {
    type Builder<'a> = game_types_capnp::vec3::Builder<'a>;
    type Reader<'a> = game_types_capnp::vec3::Reader<'a>;
    type ReaderError = Infallible;

    fn write_to_message(&self, builder: &mut Self::Builder<'_>) {
        builder.set_x(self.x);
        builder.set_y(self.y);
        builder.set_z(self.z);
    }

    fn read_from_message(reader: &Self::Reader<'_>) -> Result<Self, Self::ReaderError> {
        Ok(Self::new(reader.get_x(), reader.get_y(), reader.get_z()))
    }
}

impl CapnpExt for Quat {
    type Builder<'a> = game_types_capnp::quat::Builder<'a>;
    type Reader<'a> = game_types_capnp::quat::Reader<'a>;
    type ReaderError = Infallible;

    fn write_to_message(&self, builder: &mut Self::Builder<'_>) {
        builder.set_x(self.x);
        builder.set_y(self.y);
        builder.set_z(self.z);
        builder.set_w(self.w);
    }

    fn read_from_message(reader: &Self::Reader<'_>) -> Result<Self, Self::ReaderError> {
        Ok(Self::from_xyzw(reader.get_x(), reader.get_y(), reader.get_z(), reader.get_w()).normalize())
    }
}

impl CapnpExt for RegistryName {
    type Builder<'a> = game_types_capnp::registry_name::Builder<'a>;
    type Reader<'a> = game_types_capnp::registry_name::Reader<'a>;
    type ReaderError = capnp::Error;

    fn write_to_message(&self, builder: &mut Self::Builder<'_>) {
        builder.set_ns(self.ns.as_str());
        builder.set_key(self.key.as_str());
    }

    fn read_from_message(reader: &Self::Reader<'_>) -> Result<Self, Self::ReaderError> {
        let ns = reader.get_ns()?.to_str()?;
        let key = reader.get_key()?.to_str()?;
        Ok(RegistryName::new(ns, key))
    }
}

/// Decodes a packet type ID from the given packet bytes.
pub fn read_packet_id(packet_bytes: &[u8], reader_options: ReaderOptions) -> capnp::Result<network_capnp::PacketId> {
    let mut packet_bytes_ref = packet_bytes;
    let msg = capnp::serialize::read_message_from_flat_slice_no_alloc(&mut packet_bytes_ref, reader_options)?;
    let reader = TypedReader::<_, network_capnp::network_packet::Owned<capnp::any_pointer::Owned>>::new(msg);
    Ok(reader.get()?.get_id()?)
}

/// Helper to get a typed reader from a packet of a known simple type.
pub fn read_packet_simple(
    packet_bytes: &[u8],
    reader_options: ReaderOptions,
) -> capnp::Result<TypedReader<BufferSegments<&[u8]>, network_capnp::network_packet::Owned<capnp::any_pointer::Owned>>>
{
    let segments = capnp::serialize::BufferSegments::new(packet_bytes, reader_options)?;
    let reader = capnp::message::Reader::new(segments, reader_options);
    Ok(TypedReader::new(reader))
}

/// Helper to get a typed reader from a packet of a known pointer type.
pub fn read_packet<OwnedPayloadType: capnp::traits::Owned>(
    packet_bytes: &[u8],
    reader_options: ReaderOptions,
) -> capnp::Result<TypedReader<BufferSegments<&[u8]>, network_capnp::network_packet::Owned<OwnedPayloadType>>> {
    let segments = capnp::serialize::BufferSegments::new(packet_bytes, reader_options)?;
    let reader = capnp::message::Reader::new(segments, reader_options);
    Ok(TypedReader::new(reader))
}

/// Helper to create a typed writer for a packet of a known type
pub fn new_packet_builder<OwnedPayloadType: capnp::traits::Owned>()
-> TypedBuilder<network_capnp::network_packet::Owned<OwnedPayloadType>, HeapAllocator> {
    capnp::message::TypedBuilder::new_default()
}

/// Helper to create a typed writer for a packet using the int32 simple payload field.
pub fn new_simple_packet_builder()
-> TypedBuilder<network_capnp::network_packet::Owned<capnp::any_pointer::Owned>, HeapAllocator> {
    capnp::message::TypedBuilder::new_default()
}

/// Alias for the capnp builder type used for passing raw packet data around.
pub type CapnpBuilder = capnp::message::Builder<HeapAllocator>;
