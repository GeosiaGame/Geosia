#![warn(missing_docs)]
#![deny(clippy::disallowed_types)]

//! A library crate of the in-memory, on-disk and network representations of the game's core data.

pub mod coordinates;
pub mod physics;
pub mod registry;
pub mod schemas;
pub mod voxel;
