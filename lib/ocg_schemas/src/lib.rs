#![warn(missing_docs)]
#![deny(
    clippy::disallowed_types,
    clippy::await_holding_refcell_ref,
    clippy::await_holding_lock
)]

//! A library crate of the in-memory, on-disk and network representations of the game's core data.

pub mod coordinates;
pub mod direction;
pub mod physics;
pub mod range;
pub mod registries;
pub mod registry;
pub mod schemas;
pub mod voxel;

/// A trait implemented by the game server and client, specifying the concrete types to attach as extra metadata for every chunk, chunk group, entity, etc.
/// Used to inject side-specific data into common data structures.
pub trait OcgExtraData {
    /// Per-chunk data
    type ChunkData: Clone;
    /// Per-chunk group data
    type GroupData: Clone;
}

/// The side of a network connection a game instance resides on (server or client).
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum GameSide {
    /// The host for zero or more clients.
    Server,
    /// The client connecting to exactly one server.
    Client,
}

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
    pub use uuid;
}
