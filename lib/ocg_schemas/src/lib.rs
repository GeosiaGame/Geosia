#![warn(missing_docs)]
#![deny(clippy::disallowed_types)]

#![feature(trivial_bounds)]

//! A library crate of the in-memory, on-disk and network representations of the game's core data.

pub mod coordinates;
pub mod direction;
pub mod physics;
pub mod registry;
pub mod schemas;
pub mod voxel;

/// Re-exported dependencies used in API types
pub mod dependencies {
    pub use anyhow;
    pub use bevy_math;
    pub use bitflags;
    pub use bitvec;
    pub use bytemuck;
    pub use capnp;
    pub use either;
    pub use hashbrown;
    pub use itertools;
    pub use kstring;
    pub use once_cell;
    pub use rgb;
    pub use serde;
    pub use smallvec;
    pub use thiserror;
}
