//! Generic code for a stable forwards/backwards compatible serialization framework for game objects.
//!
//! Overview:
//!  - Based on 

pub mod game_types_capnp {
    include!(concat!(env!("OUT_DIR"), "/game_types_capnp.rs"));
}
