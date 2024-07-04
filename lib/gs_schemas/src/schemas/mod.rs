//! Generic code for a stable forwards/backwards compatible serialization framework for game objects.
//!
//! Based on capnproto: https://capnproto.org/language.html, https://docs.rs/capnp/latest/capnp/

use std::hash::Hash;

use uuid::Uuid;

use crate::registry::RegistryName;
use crate::schemas::network_capnp::stream_header::WhichReader;

/// Common game object types.
#[allow(missing_docs, clippy::all)] // Auto-generated
pub mod game_types_capnp {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/capnp-generated/game_types_capnp.rs"
    ));
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

/// Helpers for (de)serializing UUIDs.
pub trait SchemaUuidExt {
    /// Serializes a UUID into a capnp message.
    fn write_to_message(self, builder: &mut game_types_capnp::uuid::Builder);
    /// Deserializes a UUID from a capnp message.
    fn read_from_message(reader: &game_types_capnp::uuid::Reader) -> Self;
}

impl SchemaUuidExt for Uuid {
    fn write_to_message(self, builder: &mut game_types_capnp::uuid::Builder) {
        let (high, low) = self.as_u64_pair();
        builder.set_low(low);
        builder.set_high(high);
    }

    fn read_from_message(reader: &game_types_capnp::uuid::Reader) -> Self {
        let (high, low) = (reader.get_high(), reader.get_low());
        Self::from_u64_pair(high, low)
    }
}

/// Helpers for network stream headers.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum NetworkStreamHeader {
    /// A builtin stream type.
    Standard(network_capnp::stream_header::StandardTypes),
    /// A custom (modded) stream type.
    Custom(RegistryName),
}

impl Hash for NetworkStreamHeader {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            Self::Standard(ty) => {
                core::mem::discriminant(ty).hash(state);
            }
            Self::Custom(nm) => {
                nm.hash(state);
            }
        }
    }
}

/// Helpers for (de)serializing stream headers.
pub trait NetworkStreamHeaderExt {
    /// Serializes a stream header into a capnp message.
    fn write_to_message(&self, builder: &mut network_capnp::stream_header::Builder);
    /// Deserializes a stream header from a capnp message.
    fn read_from_message(reader: &network_capnp::stream_header::Reader) -> capnp::Result<Self>
    where
        Self: Sized;
}

impl NetworkStreamHeaderExt for NetworkStreamHeader {
    fn write_to_message(&self, builder: &mut network_capnp::stream_header::Builder) {
        match self {
            Self::Standard(standard) => {
                builder.set_standard_type(*standard);
            }
            Self::Custom(custom) => {
                let mut ser = builder.reborrow().init_custom_type();
                ser.set_ns(&custom.ns);
                ser.set_key(&custom.key);
            }
        }
    }

    fn read_from_message(reader: &network_capnp::stream_header::Reader) -> capnp::Result<Self> {
        match reader.which()? {
            WhichReader::StandardType(standard) => {
                let standard = standard?;
                Ok(Self::Standard(standard))
            }
            WhichReader::CustomType(custom) => {
                let custom = custom?;
                let ns = custom.get_ns()?.to_str()?;
                let key = custom.get_key()?.to_str()?;
                Ok(Self::Custom(RegistryName::new(ns, key)))
            }
        }
    }
}
