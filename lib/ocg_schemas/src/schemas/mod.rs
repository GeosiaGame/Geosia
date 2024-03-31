//! Generic code for a stable forwards/backwards compatible serialization framework for game objects.
//!
//! Based on capnproto: https://capnproto.org/language.html, https://docs.rs/capnp/latest/capnp/

use uuid::Uuid;

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
