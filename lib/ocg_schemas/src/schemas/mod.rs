//! Generic code for a stable forwards/backwards compatible serialization framework for game objects.
//!
//! Overview:
//!  - Based on

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
