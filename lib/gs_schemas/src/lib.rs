//! A library crate of the in-memory, on-disk and network representations of the game's core data.

extern crate core;

use anyhow::Context;
use smallvec::{Array, SmallVec};

pub mod actions;
pub mod coordinates;
pub mod direction;
pub mod math;
pub mod mutwatcher;
pub mod range;
pub mod raycast;
pub mod registries;
pub mod registry;
pub mod schemas;
pub mod voxel;

/// A trait implemented by the game server and client, specifying the concrete types to attach as extra metadata for every chunk, chunk group, entity, etc.
/// Used to inject side-specific data into common data structures.
pub trait GsExtraData: Send + Sync + 'static {
    /// Per-chunk data
    type ChunkData: Default + Clone + Send + Sync + 'static;
    /// Per-chunk group data
    type GroupData: Default + Clone + Send + Sync + 'static;

    /// The associated game side.
    const SIDE: GameSide;
}

/// The side of a network connection a game instance resides on (server or client).
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum GameSide {
    /// The host for zero or more clients.
    Server,
    /// The client connecting to exactly one server.
    Client,
}

impl GameSide {
    /// Returns the opposite side
    #[must_use]
    pub fn opposite(self) -> GameSide {
        match self {
            Self::Client => Self::Server,
            Self::Server => Self::Client,
        }
    }
}

/// Re-exported dependencies used in API types
pub mod dependencies {
    pub use anyhow;
    pub use bevy_color;
    pub use bevy_math;
    pub use bitflags;
    pub use bitvec;
    pub use bytemuck;
    pub use bytes;
    pub use capnp;
    pub use either;
    pub use hashbrown;
    pub use itertools;
    pub use kstring;
    pub use noise;
    pub use rand;
    pub use rand_xoshiro;
    pub use rusqlite;
    pub use serde;
    pub use smallvec;
    pub use thiserror;
    pub use uuid;
    pub use zorder;
}

/// A simple wrapper type that's either a slice borrow, or an owned [`SmallVec`].
pub enum SmallCowVec<'b, A: Array> {
    /// The slice variant.
    Borrowed(&'b [A::Item]),
    /// The [`SmallVec`] variant.
    Owned(SmallVec<A>),
}

impl<'b, Item, const N: usize> From<&'b [Item]> for SmallCowVec<'b, [Item; N]> {
    fn from(value: &'b [Item]) -> Self {
        Self::Borrowed(value)
    }
}

impl<A: Array> From<SmallVec<A>> for SmallCowVec<'static, A> {
    fn from(value: SmallVec<A>) -> Self {
        Self::Owned(value)
    }
}

impl<A: Array> From<Vec<A::Item>> for SmallCowVec<'static, A> {
    fn from(value: Vec<A::Item>) -> Self {
        Self::Owned(SmallVec::from_vec(value))
    }
}

impl<'b, A: Array> From<SmallCowVec<'b, A>> for SmallVec<A>
where
    A::Item: Clone,
{
    fn from(val: SmallCowVec<'b, A>) -> Self {
        match val {
            SmallCowVec::Owned(v) => v,
            SmallCowVec::Borrowed(b) => SmallVec::from(b),
        }
    }
}

impl<'b, A: Array> From<SmallCowVec<'b, A>> for Vec<A::Item>
where
    A::Item: Clone,
{
    fn from(val: SmallCowVec<'b, A>) -> Self {
        match val {
            SmallCowVec::Owned(v) => v.into_vec(),
            SmallCowVec::Borrowed(b) => Vec::from(b),
        }
    }
}

/// A simple list of errors used for accumulating errors from a loop if it's desired to handle them all at the end instead of breaking the loop.
#[derive(Debug)]
pub struct ErrorList(anyhow::Result<()>);

impl Default for ErrorList {
    fn default() -> Self {
        Self(Ok(()))
    }
}

impl ErrorList {
    /// Constructs a new empty [`ErrorList`] in an `Ok` state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Processes the given [`Result`], attaching to the error list if it's an error
    pub fn attach_if_err<T, E: Into<anyhow::Error>>(&mut self, result: Result<T, E>) {
        if let Err(e) = result.map_err(Into::into) {
            match self.0 {
                Ok(_) => self.0 = Err(e),
                Err(_) => self.0 = std::mem::replace(&mut self.0, Ok(())).context(e),
            }
        }
    }

    /// Converts the entire list into a single [`Result`]
    pub fn into_result(self) -> anyhow::Result<()> {
        self.0
    }
}
