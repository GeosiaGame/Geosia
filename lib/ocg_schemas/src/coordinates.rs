//! A collection of strongly typed newtype wrappers for the various coordinate formats within the game's world and related constants.

use std::ops::{Add, Deref};

use bevy_math::IVec3;
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Length of a side of a block in meters
pub const BLOCK_DIM: f32 = 0.5;

/// Length of a side of a chunk in blocks
pub const CHUNK_DIM: i32 = 32;
/// Length of a side of a chunk in blocks
pub const CHUNK_DIMZ: usize = CHUNK_DIM as usize;
/// Number of blocks on the face of a chunk
pub const CHUNK_DIM2: i32 = CHUNK_DIM * CHUNK_DIM;
/// Number of blocks on the face of a chunk
pub const CHUNK_DIM2Z: usize = (CHUNK_DIM * CHUNK_DIM) as usize;
/// Number of blocks in the volume of the chunk
pub const CHUNK_DIM3: i32 = CHUNK_DIM * CHUNK_DIM * CHUNK_DIM;
/// Number of blocks in the volume of the chunk
pub const CHUNK_DIM3Z: usize = (CHUNK_DIM * CHUNK_DIM * CHUNK_DIM) as usize;
/// Chunk dimensions in blocks as a [IVec3] for convenience
pub const CHUNK_DIM3V: IVec3 = IVec3::splat(CHUNK_DIM);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Error)]
#[error("Given coordinates were outside of chunk boundaries: {0}")]
/// Error when the given coordinates are outside of the chunk boundary.
pub struct InChunkVecError(IVec3);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Error)]
#[error("Given index was outside of chunk boundaries: {0}")]
/// Error when the given block index is outside of the chunk boundary.
pub struct InChunkIndexError(usize);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Pod, Zeroable, Serialize, Deserialize)]
#[repr(transparent)]
/// A block position inside of a chunk, limited to 0..=[CHUNK_DIM]
pub struct InChunkPos(pub(crate) IVec3);

#[derive(Copy, Clone, PartialEq, Hash, Debug, Default, Pod, Zeroable, Serialize, Deserialize)]
#[repr(C)]
/// A range of block positions inside of a chunk, with coordinates limited to 0..[CHUNK_DIM] (min&max are *inclusive*)
pub struct InChunkRange {
    pub(crate) min: InChunkPos,
    pub(crate) max: InChunkPos,
}

impl Eq for InChunkRange {}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Pod, Zeroable, Serialize, Deserialize)]
#[repr(transparent)]
/// An absolute chunk position in a voxel world
pub struct AbsChunkPos(pub(crate) IVec3);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Pod, Zeroable, Serialize, Deserialize)]
#[repr(transparent)]
/// A chunk position relative to another chunk position
pub struct RelChunkPos(pub(crate) IVec3);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Pod, Zeroable, Serialize, Deserialize)]
#[repr(transparent)]
/// An absolute block position in a voxel world
pub struct AbsBlockPos(pub(crate) IVec3);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Pod, Zeroable, Serialize, Deserialize)]
#[repr(transparent)]
/// A block position relative to another block position
pub struct RelBlockPos(pub(crate) IVec3);

// === Utils
macro_rules! impl_simple_ivec3_newtype {
    ($T:ident) => {
        impl $T {
            /// (0, 0, 0)
            pub const ZERO: Self = Self(IVec3::ZERO);
            /// (1, 1, 1)
            pub const ONE: Self = Self(IVec3::ONE);
            /// (1, 0, 0)
            pub const X: Self = Self(IVec3::X);
            /// (0, 1, 0)
            pub const Y: Self = Self(IVec3::Y);
            /// (0, 0, 1)
            pub const Z: Self = Self(IVec3::Z);

            /// Const-friendly from<IVec3>
            pub const fn from_ivec3(value: IVec3) -> Self {
                Self(value)
            }

            /// Const-friendly into<IVec3>
            pub const fn into_ivec3(self) -> IVec3 {
                self.0
            }

            /// Constructs a new [`Self`] from the given coordinates.
            pub const fn new(x: i32, y: i32, z: i32) -> Self {
                Self(IVec3::new(x, y, z))
            }
        }

        impl From<IVec3> for $T {
            fn from(value: IVec3) -> Self {
                Self::from_ivec3(value)
            }
        }
        impl From<$T> for IVec3 {
            fn from(value: $T) -> IVec3 {
                value.into_ivec3()
            }
        }
        impl Deref for $T {
            type Target = IVec3;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
}

// === InChunkPos

impl TryFrom<IVec3> for InChunkPos {
    type Error = InChunkVecError;

    #[inline]
    fn try_from(value: IVec3) -> Result<Self, Self::Error> {
        Self::try_from_ivec3(value)
    }
}

impl From<InChunkPos> for IVec3 {
    #[inline]
    fn from(value: InChunkPos) -> IVec3 {
        value.0
    }
}

impl Deref for InChunkPos {
    type Target = IVec3;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl InChunkPos {
    /// (0, 0, 0)
    pub const ZERO: Self = Self(IVec3::ZERO);
    /// (1, 1, 1)
    pub const ONE: Self = Self(IVec3::ONE);
    /// (1, 0, 0)
    pub const X: Self = Self(IVec3::X);
    /// (0, 1, 0)
    pub const Y: Self = Self(IVec3::Y);
    /// (0, 0, 1)
    pub const Z: Self = Self(IVec3::Z);
    /// (31, 31, 31)
    pub const MAX: Self = Self(IVec3::splat(CHUNK_DIM - 1));

    /// Const-friendly try_from<IVec3>
    pub const fn try_from_ivec3(v: IVec3) -> Result<Self, InChunkVecError> {
        let IVec3 { x, y, z } = v;
        if (x < 0) || (x >= CHUNK_DIM) || (y < 0) || (y >= CHUNK_DIM) || (z < 0) || (z >= CHUNK_DIM) {
            Err(InChunkVecError(v))
        } else {
            Ok(Self(v))
        }
    }

    /// Constructs a new in-chunk position from the given coordinates, or returns an error if it's
    /// outside of chunk bounds.
    pub const fn try_new(x: i32, y: i32, z: i32) -> Result<Self, InChunkVecError> {
        Self::try_from_ivec3(IVec3::new(x, y, z))
    }

    /// Same as `try_new(v, v, v)`
    pub const fn try_splat(v: i32) -> Result<Self, InChunkVecError> {
        Self::try_from_ivec3(IVec3::splat(v))
    }

    /// Convert a XZY-strided index into a chunk storage array into the coordinates
    pub const fn try_from_index(idx: usize) -> Result<Self, InChunkIndexError> {
        if idx >= CHUNK_DIM3Z {
            return Err(InChunkIndexError(idx));
        }
        let i: i32 = idx as i32;
        Ok(InChunkPos(IVec3::new(
            i % CHUNK_DIM,
            (i / CHUNK_DIM2) % CHUNK_DIM,
            (i / CHUNK_DIM) % CHUNK_DIM,
        )))
    }

    /// Converts the coordinates into an XZY-strided index into the chunk storage array
    pub const fn as_index(self) -> usize {
        (self.0.x + (CHUNK_DIM * self.0.z) + (CHUNK_DIM2 * self.0.y)) as usize
    }
}

impl Add<InChunkPos> for InChunkPos {
    type Output = RelBlockPos;
    #[inline]
    fn add(self, rhs: InChunkPos) -> Self::Output {
        RelBlockPos(self.0 + rhs.0)
    }
}

// === InChunkRange
impl InChunkRange {
    /// Empty range containing no blocks, originating at (0, 0, 0).
    pub const ZERO: Self = Self::from_corners(InChunkPos::ZERO, InChunkPos::ZERO);
    /// A single block at (0, 0, 0).
    pub const BLOCK_AT_ZERO: Self = Self::from_corners(InChunkPos::ZERO, InChunkPos::ONE);
    /// The whole chunk `[(0, 0, 0), (31, 31, 31)]`.
    pub const WHOLE_CHUNK: Self = Self::from_corners(InChunkPos::ZERO, InChunkPos::MAX);

    /// Constructs a new range from two (inclusive) corner positions.
    pub const fn from_corners(a: InChunkPos, b: InChunkPos) -> Self {
        // Min/max manually implemented to allow for `const` calls
        let (min_x, max_x) = if a.0.x < b.0.x {
            (a.0.x, b.0.x)
        } else {
            (b.0.x, (a.0.x))
        };
        let (min_y, max_y) = if a.0.y < b.0.y {
            (a.0.y, b.0.y)
        } else {
            (b.0.y, (a.0.y))
        };
        let (min_z, max_z) = if a.0.z < b.0.z {
            (a.0.z, b.0.z)
        } else {
            (b.0.z, (a.0.z))
        };
        let min = InChunkPos(IVec3::new(min_x, min_y, min_z));
        let max = InChunkPos(IVec3::new(max_x, max_y, max_z));
        Self { min, max }
    }

    /// Checks if the range has any blocks, false if one or more of the dimensions are zero.
    pub const fn is_empty(self) -> bool {
        (self.min.0.x == self.max.0.x) || (self.min.0.y == self.max.0.y) || (self.min.0.z == self.max.0.z)
    }

    /// Returns the corner with the smallest coordinates.
    pub const fn min(self) -> InChunkPos {
        self.min
    }

    /// Returns the corner with the largest coordinates.
    pub const fn max(self) -> InChunkPos {
        self.max
    }

    /// Returns an iterator over all the coordinates inside this range, in XZY order.
    pub fn iter_xzy(self) -> impl Iterator<Item = InChunkPos> {
        itertools::iproduct!(
            self.min.y..=self.max.y,
            self.min.z..=self.max.z,
            self.min.x..=self.max.x
        )
        .map(|(y, z, x)| InChunkPos(IVec3::new(y, z, x)))
    }
}

// === AbsChunkPos
impl_simple_ivec3_newtype!(AbsChunkPos);
// === RelChunkPos
impl_simple_ivec3_newtype!(RelChunkPos);
// === AbsBlockPos
impl_simple_ivec3_newtype!(AbsBlockPos);
// === RelBlockPos
impl_simple_ivec3_newtype!(RelBlockPos);
