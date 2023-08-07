//! Data structures for storage and manipulation of per-block data.
use std::fmt::Debug;
use std::hash::Hash;
use std::iter::{Enumerate, Map, Take};

use crate::coordinates::*;

pub mod array;
pub mod palette;
pub mod sparse;

/// Marker trait for all the requirements for a type to be stored as per-block chunk data.
/// Do not derive yourself, the blanked implementation should cover all types that are valid.
pub trait ChunkDataType: Clone + PartialEq + Hash + Debug {}

/// Blanket implementation for all valid chunk data types.
impl<T> ChunkDataType for T where T: Clone + PartialEq + Hash + Debug {}

/// A container for chunk's data, abstracted from the actual in-memory representation for flexibility.
///
/// The game uses various types of storage, ranging from dense array representations, through palette compression to sparse hash-based storage.
pub trait ChunkStorage<DataType: ChunkDataType> {
    /// Clone all elements of the chunk into a dense XZY-ordered array (with strides of X=1, Z=32, Y=32²).
    fn copy_dense(&self, output: &mut [DataType; CHUNK_DIM3Z]);
    /// Gets the element at the given coordinates, or [`None`] if there is no chunk data at all.
    fn get(&self, position: InChunkPos) -> &DataType;
    /// Gets the element at the given coordinates, for [`Copy`] types. If there is no chunk data, returns the default value.
    fn get_copy(&self, position: InChunkPos) -> DataType
    where
        DataType: Copy;
    /// Puts a single element at the given coordinates.
    ///
    /// Returns the old value.
    fn put(&mut self, position: InChunkPos, new_value: DataType) -> DataType;
    /// Fills a cuboid with the given value.
    fn fill(&mut self, range: InChunkRange, new_value: DataType);
}

pub use array::ArrayStorage;
pub use palette::PaletteStorage;
pub use sparse::SparseStorage;

#[inline]
fn i_to_xzy_itermap<T>((i, val): (usize, T)) -> (InChunkPos, T) {
    (InChunkPos::try_from_index(i).unwrap(), val)
}

type XzyIterator<Iter> =
    Map<Enumerate<Take<Iter>>, fn((usize, <Iter as Iterator>::Item)) -> (InChunkPos, <Iter as Iterator>::Item)>;

/// Extension methods for iterators over chunks
trait ChunkIterator: Iterator {
    #[inline]
    fn enumerate_xzy(self) -> XzyIterator<Self>
    where
        Self: Sized,
    {
        self.take(CHUNK_DIM3Z).enumerate().map(i_to_xzy_itermap)
    }
}
impl<T> ChunkIterator for T where T: Iterator {}
