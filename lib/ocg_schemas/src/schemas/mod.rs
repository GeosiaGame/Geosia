//! Generic code for a stable forwards/backwards compatible serialization framework for game objects.
//!
//! Overview:
//!  - Based on

/// Common game object types
pub mod game_types_capnp {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/capnp-generated/game_types_capnp.rs"
    ));
}
