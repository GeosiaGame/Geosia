//! A collection of strongly typed newtype wrappers for the various coordinate formats within the game's world and related constants.

use std::fmt::{Display, Formatter};
use std::ops::{Add, Deref};

use bevy_math::{IVec3, UVec3};
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
/// Maximum block position allowed, +-2^30 or 1 billion blocks to have a safe margin to avoid integer overflows.
pub const MAX_BLOCK_POS: i32 = 1 << 30;
/// [`MAX_BLOCK_POS`] converted to the unit of chunks.
pub const MAX_CHUNK_POS: i32 = MAX_BLOCK_POS / CHUNK_DIM;

// xxx yyy zzz -> xyzxyzxyz bit pattern
// reference for tests
/// Slower reference implementation of zpack_3d, public for benchmark purposes
pub fn zpack_3d_naive(vec: IVec3) -> u128 {
    let vec = vec.as_uvec3();
    let x = vec.x;
    let y = vec.y;
    let z = vec.z;
    let mut out = 0u128;
    for bit in 0..32 {
        let bit_mask = 1 << bit;
        let z_set = (z & bit_mask) != 0;
        let y_set = (y & bit_mask) != 0;
        let x_set = (x & bit_mask) != 0;
        if z_set {
            out |= 1u128 << (3 * bit);
        }
        if y_set {
            out |= 1u128 << (3 * bit + 1);
        }
        if x_set {
            out |= 1u128 << (3 * bit + 2);
        }
    }
    out
}

const fn bit_repeat(pattern: u128, len: u32) -> u128 {
    if len >= 128 || len == 0 {
        pattern
    } else {
        bit_repeat(pattern | (pattern << len), len * 2)
    }
}

/// Converts a 3d vector of ints to a XYZ Z-order curve packed 128-bit integer by interleaving the bits.
/// Provides spatial locality for sorted coordinates.
/// See [Z-order curves](https://en.wikipedia.org/wiki/Z-order_curve).
#[inline]
pub fn zpack_3d(vec: IVec3) -> u128 {
    // Manual optimization, because using the zcurve crate is too slow in debug mode.
    const BIT96: u128 = (1u128 << 97) - 1;
    let vec = vec.as_uvec3();
    let mut x = vec.x as u128;
    let mut y = vec.y as u128;
    let mut z = vec.z as u128;
    // 0x0000_0000_0000_0000_ABCD_EFGH to
    // 0x0000_0000_ABCD_EFGH_ABCD_EFGH to
    // 0x0000_0000_ABCD_0000_0000_EFGH
    x = (x | x.wrapping_shl(32)) & 0x0000_0000_FFFF_0000_0000_FFFF;
    y = (y | y.wrapping_shl(32)) & 0x0000_0000_FFFF_0000_0000_FFFF;
    z = (z | z.wrapping_shl(32)) & 0x0000_0000_FFFF_0000_0000_FFFF;
    // 0x0000_0000_ABCD_0000_0000_EFGH to
    // 0x0000_ABCD_ABCD_0000_EFGH_EFGH to
    // 0x0000_AB00_00CD_0000_EF00_00GH to
    x = (x | x.wrapping_shl(16)) & const { BIT96 & bit_repeat(0x00_00_FF, 24) };
    y = (y | y.wrapping_shl(16)) & const { BIT96 & bit_repeat(0x00_00_FF, 24) };
    z = (z | z.wrapping_shl(16)) & const { BIT96 & bit_repeat(0x00_00_FF, 24) };
    // 0x00A0_0B00_C00D_00E0_0F00_G00H ...
    x = (x | x.wrapping_shl(8)) & const { BIT96 & bit_repeat(0x00F, 12) };
    y = (y | y.wrapping_shl(8)) & const { BIT96 & bit_repeat(0x00F, 12) };
    z = (z | z.wrapping_shl(8)) & const { BIT96 & bit_repeat(0x00F, 12) };
    x = (x | x.wrapping_shl(4)) & const { BIT96 & bit_repeat(0b00_00_11, 6) };
    y = (y | y.wrapping_shl(4)) & const { BIT96 & bit_repeat(0b00_00_11, 6) };
    z = (z | z.wrapping_shl(4)) & const { BIT96 & bit_repeat(0b00_00_11, 6) };
    x = (x | x.wrapping_shl(2)) & const { BIT96 & bit_repeat(0b001, 3) };
    y = (y | y.wrapping_shl(2)) & const { BIT96 & bit_repeat(0b001, 3) };
    z = (z | z.wrapping_shl(2)) & const { BIT96 & bit_repeat(0b001, 3) };

    x.wrapping_shl(2) | y.wrapping_shl(1) | z
}

#[test]
fn test_bit_repeat() {
    fn check(line: u32, a: u128, b: u128) {
        assert_eq!(a, b, "[line {line}] \n{a:032x} != \n{b:032x}");
    }
    check(line!(), bit_repeat(0x0, 1), 0);
    check(line!(), bit_repeat(0x1, 1), u128::MAX);
    check(line!(), bit_repeat(0x0, 32), 0);
    check(line!(), bit_repeat(0x01, 32), 0x00000001_00000001_00000001_00000001);
    check(line!(), bit_repeat(0x10, 32), 0x00000010_00000010_00000010_00000010);
    check(line!(), bit_repeat(0x1000, 32), 0x00001000_00001000_00001000_00001000);
    check(
        line!(),
        bit_repeat(0x10000000, 32),
        0x10000000_10000000_10000000_10000000,
    );
}

#[test]
fn test_zpack_3d() {
    use itertools::iproduct;
    let list = [
        0,
        1,
        2,
        4,
        8,
        16,
        32,
        64,
        128,
        256,
        512,
        1024,
        65536,
        1 << 30,
        1 << 31,
        -1,
        -2,
        -4,
        -8,
        -16,
        -32,
        -64,
        -128,
        7,
        321,
        -127,
        i32::MIN,
        i32::MAX,
    ];
    for (x, y, z) in iproduct!(list, list, list) {
        let v = IVec3::new(x, y, z);
        let naive = zpack_3d_naive(v);
        let fast = zpack_3d(v);
        assert_eq!(
            naive, fast,
            "zpack of {v} is not valid.\n    x: {x:032b}\n    y: {y:032b}\n    z: {z:032b}\nnaive: {naive:0128b} ({nones} ones)\n fast: {fast:0128b} ({fones} ones)\n",
            x = v.x as u32,
            y = v.y as u32,
            z = v.z as u32,
            nones = naive.count_ones(),
            fones = fast.count_ones(),
        );
    }
}

/// Restores a 3d vector of ints from a XYZ Z-order curve packed 128-bit integer by interleaving the bits.
/// See [`zpack_3d`].
#[inline]
pub fn zunpack_3d(idx: u128) -> IVec3 {
    let [y, z, x] = zorder::coord_of(idx);
    UVec3::new(x, y, z).as_ivec3()
}

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

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Pod, Zeroable, Serialize, Deserialize)]
#[repr(C)]
/// A range of block positions inside of a chunk, with coordinates limited to 0..[CHUNK_DIM] (min&max are *inclusive*)
pub struct InChunkRange {
    pub(crate) min: InChunkPos,
    pub(crate) max: InChunkPos,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, Pod, Zeroable, Serialize, Deserialize)]
#[repr(C)]
/// A range of chunk positions (min&max are *inclusive*)
pub struct AbsChunkRange {
    pub(crate) min: AbsChunkPos,
    pub(crate) max: AbsChunkPos,
}

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

            /// Const-friendly `from<IVec3>`
            #[inline]
            pub const fn from_ivec3(value: IVec3) -> Self {
                Self(value)
            }

            /// Const-friendly `into<IVec3>`
            #[inline]
            pub const fn into_ivec3(self) -> IVec3 {
                self.0
            }

            /// Constructs a new [`Self`] from the given coordinates.
            #[inline]
            pub const fn new(x: i32, y: i32, z: i32) -> Self {
                Self(IVec3::new(x, y, z))
            }

            /// Constructs a new [`Self`] from a given coordinate copied to all dimensions.
            #[inline]
            pub const fn splat(v: i32) -> Self {
                Self(IVec3::splat(v))
            }
        }

        impl From<IVec3> for $T {
            #[inline]
            fn from(value: IVec3) -> Self {
                Self::from_ivec3(value)
            }
        }
        impl From<$T> for IVec3 {
            #[inline]
            fn from(value: $T) -> IVec3 {
                value.into_ivec3()
            }
        }
        impl std::ops::Deref for $T {
            type Target = IVec3;

            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
}

macro_rules! impl_rel_abs_pair {
    ($Rel:ident, $Abs:ident) => {
        impl std::ops::Add<$Rel> for $Rel {
            type Output = $Rel;
            #[inline]
            fn add(self, rhs: Self) -> Self::Output {
                $Rel(self.0 + rhs.0)
            }
        }
        impl std::ops::Add<$Abs> for $Rel {
            type Output = $Abs;
            #[inline]
            fn add(self, rhs: $Abs) -> Self::Output {
                $Abs(self.0 + rhs.0)
            }
        }
        impl std::ops::Add<$Rel> for $Abs {
            type Output = $Abs;
            #[inline]
            fn add(self, rhs: $Rel) -> Self::Output {
                $Abs(self.0 + rhs.0)
            }
        }

        impl std::ops::Sub<$Rel> for $Rel {
            type Output = $Rel;
            #[inline]
            fn sub(self, rhs: Self) -> Self::Output {
                $Rel(self.0 - rhs.0)
            }
        }
        impl std::ops::Sub<$Abs> for $Rel {
            type Output = $Abs;
            #[inline]
            fn sub(self, rhs: $Abs) -> Self::Output {
                $Abs(self.0 - rhs.0)
            }
        }
        impl std::ops::Sub<$Rel> for $Abs {
            type Output = $Abs;
            #[inline]
            fn sub(self, rhs: $Rel) -> Self::Output {
                $Abs(self.0 - rhs.0)
            }
        }
        impl std::ops::Sub<$Abs> for $Abs {
            type Output = $Rel;
            #[inline]
            fn sub(self, rhs: $Abs) -> Self::Output {
                $Rel(self.0 - rhs.0)
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

    /// Const-friendly `try_from<IVec3>`
    #[inline]
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
    #[inline]
    pub const fn try_new(x: i32, y: i32, z: i32) -> Result<Self, InChunkVecError> {
        Self::try_from_ivec3(IVec3::new(x, y, z))
    }

    /// Same as `try_new(v, v, v)`
    #[inline]
    pub const fn try_splat(v: i32) -> Result<Self, InChunkVecError> {
        Self::try_from_ivec3(IVec3::splat(v))
    }

    /// Convert a XZY-strided index into a chunk storage array into the coordinates
    #[inline]
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
    #[inline]
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
    /// One block range containing the block at (0,0,0).
    pub const BLOCK_AT_ZERO: Self = Self::from_corners(InChunkPos::ZERO, InChunkPos::ZERO);
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

    /// Checks if the range covers the entire chunk
    #[inline]
    pub const fn is_everything(self) -> bool {
        self.min.0.x == 0
            && self.min.0.y == 0
            && self.min.0.z == 0
            && self.max.0.x == InChunkPos::MAX.0.x
            && self.max.0.y == InChunkPos::MAX.0.y
            && self.max.0.z == InChunkPos::MAX.0.z
    }

    /// Returns the corner with the smallest coordinates.
    #[inline]
    pub const fn min(self) -> InChunkPos {
        self.min
    }

    /// Returns the corner with the largest coordinates.
    #[inline]
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

impl AbsChunkRange {
    /// A single chunk at (0, 0, 0).
    pub const BLOCK_AT_ZERO: Self = Self::from_corners(AbsChunkPos::ZERO, AbsChunkPos::ZERO);

    /// Constructs a new range from two (inclusive) corner positions.
    pub const fn from_corners(a: AbsChunkPos, b: AbsChunkPos) -> Self {
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
        let min = AbsChunkPos(IVec3::new(min_x, min_y, min_z));
        let max = AbsChunkPos(IVec3::new(max_x, max_y, max_z));
        Self { min, max }
    }

    /// Returns the corner with the smallest coordinates.
    pub const fn min(self) -> AbsChunkPos {
        self.min
    }

    /// Returns the corner with the largest coordinates.
    pub const fn max(self) -> AbsChunkPos {
        self.max
    }

    /// Returns an iterator over all the coordinates inside this range, in XZY order.
    pub fn iter_xzy(self) -> impl Iterator<Item = AbsChunkPos> {
        itertools::iproduct!(
            self.min.y..=self.max.y,
            self.min.z..=self.max.z,
            self.min.x..=self.max.x
        )
        .map(|(y, z, x)| AbsChunkPos(IVec3::new(y, z, x)))
    }
}

// === AbsChunkPos
impl_simple_ivec3_newtype!(AbsChunkPos);

impl From<AbsBlockPos> for AbsChunkPos {
    fn from(value: AbsBlockPos) -> Self {
        Self::new(
            value.x.div_euclid(CHUNK_DIM),
            value.y.div_euclid(CHUNK_DIM),
            value.z.div_euclid(CHUNK_DIM),
        )
    }
}

impl AbsChunkPos {
    /// Converts the chunk position to a Z-curve index. See [`zpack_3d`].
    #[inline]
    pub fn as_zpack(self) -> u128 {
        zpack_3d(self.0)
    }

    /// Converts the chunk position from a Z-curve index. See [`zunpack_3d`].
    #[inline]
    pub fn from_zpack(idx: u128) -> Self {
        Self(zunpack_3d(idx))
    }
}

impl Display for AbsChunkPos {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Chunk(x={}, y={}, z={})", self.x, self.y, self.z)
    }
}

impl PartialOrd for AbsChunkPos {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }

    fn lt(&self, other: &Self) -> bool {
        self.as_zpack() < other.as_zpack()
    }

    fn le(&self, other: &Self) -> bool {
        self.as_zpack() <= other.as_zpack()
    }

    fn gt(&self, other: &Self) -> bool {
        self.as_zpack() > other.as_zpack()
    }

    fn ge(&self, other: &Self) -> bool {
        self.as_zpack() >= other.as_zpack()
    }
}

impl Ord for AbsChunkPos {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_zpack().cmp(&other.as_zpack())
    }
}

// === RelChunkPos
impl_simple_ivec3_newtype!(RelChunkPos);
impl_rel_abs_pair!(RelChunkPos, AbsChunkPos);

impl Display for RelChunkPos {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Chunk Difference(x={}, y={}, z={})", self.x, self.y, self.z)
    }
}

// === AbsBlockPos
impl_simple_ivec3_newtype!(AbsBlockPos);

impl From<AbsChunkPos> for AbsBlockPos {
    fn from(value: AbsChunkPos) -> Self {
        Self(value.0 * IVec3::splat(CHUNK_DIM))
    }
}

impl AbsBlockPos {
    /// Splits the block position into the coordinate of the chunk and coordinate of the block within that chunk
    pub fn split_chunk_component(self) -> (AbsChunkPos, InChunkPos) {
        (
            AbsChunkPos::new(
                self.x.div_euclid(CHUNK_DIM),
                self.y.div_euclid(CHUNK_DIM),
                self.z.div_euclid(CHUNK_DIM),
            ),
            InChunkPos(IVec3::new(
                self.x.rem_euclid(CHUNK_DIM),
                self.y.rem_euclid(CHUNK_DIM),
                self.z.rem_euclid(CHUNK_DIM),
            )),
        )
    }

    /// Converts the block position to a Z-curve index. See [`zpack_3d`].
    #[inline]
    pub fn as_zpack(self) -> u128 {
        zpack_3d(self.0)
    }

    /// Converts the block position from a Z-curve index. See [`zunpack_3d`].
    #[inline]
    pub fn from_zpack(idx: u128) -> Self {
        Self(zunpack_3d(idx))
    }
}

impl Display for AbsBlockPos {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Block(x={}, y={}, z={})", self.x, self.y, self.z)
    }
}

impl PartialOrd for AbsBlockPos {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }

    fn lt(&self, other: &Self) -> bool {
        self.as_zpack() < other.as_zpack()
    }

    fn le(&self, other: &Self) -> bool {
        self.as_zpack() <= other.as_zpack()
    }

    fn gt(&self, other: &Self) -> bool {
        self.as_zpack() > other.as_zpack()
    }

    fn ge(&self, other: &Self) -> bool {
        self.as_zpack() >= other.as_zpack()
    }
}

impl Ord for AbsBlockPos {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_zpack().cmp(&other.as_zpack())
    }
}

// === RelBlockPos
impl_simple_ivec3_newtype!(RelBlockPos);
impl_rel_abs_pair!(RelBlockPos, AbsBlockPos);

impl From<RelChunkPos> for RelBlockPos {
    fn from(value: RelChunkPos) -> Self {
        Self(value.0 * IVec3::splat(CHUNK_DIM))
    }
}

impl Display for RelBlockPos {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Block Difference(x={}, y={}, z={})", self.x, self.y, self.z)
    }
}
