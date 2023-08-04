//! Data structures for storage and manipulation of per-block data.
use std::fmt::Debug;
use std::hash::Hash;
use std::iter::{Enumerate, Map, Take};

use bitvec::prelude::*;
use either::Either;
use hashbrown::HashMap;
use itertools::{iproduct, Itertools};
use smallvec::{smallvec, SmallVec};

use crate::coordinates::*;

/// Marker trait for all the requirements for a type to be stored as per-block chunk data.
/// Do not derive yourself, the blanked implementation should cover all types that are valid.
pub trait ChunkDataType: Clone + Default + PartialEq + Hash + Debug {}

/// Blanket implementation for all valid chunk data types.
impl<T> ChunkDataType for T where T: Clone + Default + PartialEq + Hash + Debug {}

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

/// Chunk data compressed by storing a list of used values in a `palette` array and indices into that array for every chunk element.
/// A special case for all data being of the same type has a very small memory footprint.
#[derive(Clone, Eq, PartialEq)]
pub struct PaletteStorage<DataType: ChunkDataType> {
    palette: SmallVec<[DataType; 16]>,
    /// Invariant: The length is 1, CHUNK_DIM3Z / 2 (u8 indices) or CHUNK_DIM3Z (u16 indices)
    data_storage: SmallVec<[u16; 1]>,
    /// Length of [`palette`] at the last palette GC call
    last_gc_palette_len: usize,
}

/// Simple XZY dense array storage for chunk data (with strides of X=1, Z=32, Y=32²).
#[derive(Clone, Eq, PartialEq)]
pub enum ArrayStorage<T> {
    /// Single-element case for cases where every single chunk element is identical
    Singleton(T),
    /// Case where at least one element in a chunk is different
    Array(Box<[T; CHUNK_DIM3Z]>),
}

/// Storage for sparse chunk data, only allocating data for the data that's present at the cost of slower lookups and writes.
pub struct SparseStorage<DataType: ChunkDataType> {
    data: HashMap<u16, DataType>,
}

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

// Implementations

enum SafePaletteIndices<'d> {
    Singleton,
    U8(&'d [u8; CHUNK_DIM3Z]),
    U16(&'d [u16; CHUNK_DIM3Z]),
}

enum SafePaletteIndicesMut<'d> {
    Singleton,
    U8(&'d mut [u8; CHUNK_DIM3Z]),
    U16(&'d mut [u16; CHUNK_DIM3Z]),
}

impl<'d> SafePaletteIndices<'d> {
    fn new(data_storage: &SmallVec<[u16; 1]>) -> SafePaletteIndices {
        match data_storage.len() {
            0 | 1 => SafePaletteIndices::Singleton,
            PAL_DATA_ARRAY_U8_LEN => {
                let byte_arr: Result<&[u8; CHUNK_DIM3Z], _> =
                    bytemuck::cast_slice::<u16, u8>(&data_storage[..]).try_into();
                SafePaletteIndices::U8(byte_arr.expect("Wrong internal palette array size"))
            }
            PAL_DATA_ARRAY_U16_LEN => {
                let arr: Result<&[u16; CHUNK_DIM3Z], _> = data_storage[..].try_into();
                SafePaletteIndices::U16(arr.expect("Wrong internal palette array size"))
            }
            len => panic!("Invalid data array size of {} items", len),
        }
    }

    fn iter_wide(&self) -> impl Iterator<Item = u16> + '_ {
        match self {
            SafePaletteIndices::Singleton => Either::Left(std::iter::repeat(0).take(CHUNK_DIM3Z)),
            SafePaletteIndices::U8(indices) => Either::Right(Either::Left(indices.iter().map(|&v| v as u16))),
            SafePaletteIndices::U16(indices) => Either::Right(Either::Right(indices.iter().copied())),
        }
    }
}

impl<'d> SafePaletteIndicesMut<'d> {
    fn new(data_storage: &mut SmallVec<[u16; 1]>) -> SafePaletteIndicesMut {
        match data_storage.len() {
            0 | 1 => SafePaletteIndicesMut::Singleton,
            PAL_DATA_ARRAY_U8_LEN => {
                let byte_arr: Result<&mut [u8; CHUNK_DIM3Z], _> =
                    bytemuck::cast_slice_mut::<u16, u8>(&mut data_storage[..]).try_into();
                SafePaletteIndicesMut::U8(byte_arr.expect("Wrong internal palette array size"))
            }
            PAL_DATA_ARRAY_U16_LEN => {
                let arr: Result<&mut [u16; CHUNK_DIM3Z], _> = (&mut data_storage[..]).try_into();
                SafePaletteIndicesMut::U16(arr.expect("Wrong internal palette array size"))
            }
            len => panic!("Invalid data array size of {} items", len),
        }
    }
}

/// Maximum number of elements in the palette that uses [`u8`] storage
const PAL_BYTE_CUTOFF: usize = 255;
/// Length of the data array when using u8-typed data
const PAL_DATA_ARRAY_U8_LEN: usize = CHUNK_DIM3Z / 2;
/// Length of the data array when using u16-typed data
const PAL_DATA_ARRAY_U16_LEN: usize = CHUNK_DIM3Z;

impl<DataType: ChunkDataType + Copy> PaletteStorage<DataType> {
    fn data(&self) -> SafePaletteIndices {
        SafePaletteIndices::new(&self.data_storage)
    }

    fn data_mut(&mut self) -> SafePaletteIndicesMut {
        SafePaletteIndicesMut::new(&mut self.data_storage)
    }

    /// Iterates over all the data in XZY order (with strides of X=1, Z=32, Y=32²).
    pub fn iter(&self) -> impl Iterator<Item = &DataType> {
        // Use Either to wrap the iterators to allow varying return types.
        match self.data() {
            SafePaletteIndices::Singleton => Either::Left(std::iter::repeat(&self.palette[0]).take(CHUNK_DIM3Z)),
            SafePaletteIndices::U8(indices) => {
                Either::Right(Either::Left(indices.iter().map(|&idx| &self.palette[idx as usize])))
            }
            SafePaletteIndices::U16(indices) => {
                Either::Right(Either::Right(indices.iter().map(|&idx| &self.palette[idx as usize])))
            }
        }
    }

    /// Iterates over all the data paired with the block coordinates inside the chunk, in XZY order.
    pub fn iter_with_coords(&self) -> impl Iterator<Item = (InChunkPos, &DataType)> {
        self.iter().enumerate_xzy()
    }

    /// Garbage collect unused palette entries, compacting the chunk data.
    #[cold]
    fn palette_gc(&mut self, ignored_coord: Option<InChunkPos>) {
        self.last_gc_palette_len = self.palette.len();
        let mut pal_entry_used = bitarr!(0; CHUNK_DIM3Z); // 4 kiB
        let ignored_idx = ignored_coord.map(InChunkPos::as_index).unwrap_or(CHUNK_DIM3Z);
        fn mark_used_entries<T: Into<usize> + Copy>(
            ignored_idx: usize,
            indices: &[T],
            pal_entry_used: &mut BitArr!(for CHUNK_DIM3Z),
        ) {
            indices[..ignored_idx]
                .iter()
                .for_each(|&idx| pal_entry_used.set(idx.into(), true));
            if ignored_idx + 1 < indices.len() {
                indices[ignored_idx + 1..]
                    .iter()
                    .for_each(|&idx| pal_entry_used.set(idx.into(), true));
            }
        }
        match self.data() {
            SafePaletteIndices::Singleton => {
                return;
            }
            SafePaletteIndices::U8(indices) => {
                mark_used_entries(ignored_idx, indices, &mut pal_entry_used);
            }
            SafePaletteIndices::U16(indices) => {
                mark_used_entries(ignored_idx, indices, &mut pal_entry_used);
            }
        }
        let pal_entry_used = &pal_entry_used[0..self.palette.len()];
        let entries_used = pal_entry_used.count_ones();
        match entries_used {
            _ if entries_used == self.palette.len() => {
                // No unused entries
            }
            1 => {
                // Convert to singleton, freeing all the data
                self.data_storage = smallvec![];
                self.palette = smallvec![self.palette[pal_entry_used.first_one().unwrap()]];
                self.last_gc_palette_len = 1;
            }
            2..=CHUNK_DIM3Z => {
                let old_palette = std::mem::take(&mut self.palette);
                let mut pal_remap = vec![0u16; old_palette.len()];
                // Compacts the palette array by removing all unused indices, and creating a remapping as pal_remap[old] == new
                for used_idx in pal_entry_used.iter_ones() {
                    pal_remap[used_idx] = self.palette.len() as u16;
                    self.palette.push(old_palette[used_idx]);
                }
                let old_data = std::mem::take(&mut self.data_storage);
                let old_view = SafePaletteIndices::new(&old_data);
                if entries_used <= PAL_BYTE_CUTOFF {
                    self.data_storage.resize(PAL_DATA_ARRAY_U8_LEN, 0);
                } else {
                    self.data_storage.resize(PAL_DATA_ARRAY_U16_LEN, 0);
                }
                let new_view = SafePaletteIndicesMut::new(&mut self.data_storage);
                match new_view {
                    SafePaletteIndicesMut::U8(new_view) => {
                        for (old_idx, new_idx) in old_view.iter_wide().zip_eq(new_view.iter_mut()) {
                            *new_idx = pal_remap[old_idx as usize] as u8;
                        }
                    }
                    SafePaletteIndicesMut::U16(new_view) => {
                        for (old_idx, new_idx) in old_view.iter_wide().zip_eq(new_view.iter_mut()) {
                            *new_idx = pal_remap[old_idx as usize];
                        }
                    }
                    SafePaletteIndicesMut::Singleton => unreachable!(),
                }
            }
            len => panic!("Invalid palette size of {} items", len),
        }
    }

    /// Returns the palette index of the given data element. Needs a coordinate to ignore when modifying palette entries in case of a 100% full palette.
    #[inline]
    fn palette_get_or_insert(&mut self, dt: DataType, ignored_coord: InChunkPos) -> u16 {
        if let Some(palpos) = self.palette.iter().position(|pel| pel == &dt) {
            return palpos as u16;
        }
        if self.palette.len() >= CHUNK_DIM3Z {
            // Slow path: let's assume chunks with a unique paletted item in every single blockspace are incredibly rare
            self.palette_gc(Some(ignored_coord));
        }
        let idx = self.palette.len();
        assert!(idx < CHUNK_DIM3Z);
        self.palette.push(dt);
        idx as u16
    }

    /// Upgrade the internal paletted data array by 1 tier (1 -> 256 -> 65k -> no-op).
    #[cold]
    fn upgrade_storage(&mut self) {
        fn upgrade(storage: &mut SmallVec<[u16; 1]>) {
            match storage.len() {
                0 | 1 => {
                    storage.resize(PAL_DATA_ARRAY_U8_LEN, 0);
                    storage.fill(0);
                }
                PAL_DATA_ARRAY_U8_LEN => {
                    storage.resize(PAL_DATA_ARRAY_U16_LEN, 0);
                    let data_arr: &mut [u16; PAL_DATA_ARRAY_U16_LEN] = (&mut storage[..])
                        .try_into()
                        .expect("Wrong internal palette array size");
                    // Converts:
                    // | u8  u8| u8  u8|
                    // |   2b  |   2b  |
                    // Into:
                    // |u16    |u16    |u16    |u16    |
                    // | 2b    | 2b    | 2b    | 2b    |
                    // Walking in reverse ensures that data doesn't get overwritten before it gets read,
                    // because 2*idx is greater than idx for all positive idx, and at 0 the read is done first.
                    for data_pair_idx in (0..PAL_DATA_ARRAY_U8_LEN).rev() {
                        let u8s_packed = data_arr[data_pair_idx];
                        let u8_pair: [u8; 2] = u8s_packed.to_ne_bytes();
                        data_arr[data_pair_idx * 2] = u8_pair[0] as u16;
                        data_arr[data_pair_idx * 2 + 1] = u8_pair[1] as u16;
                    }
                }
                PAL_DATA_ARRAY_U16_LEN => {
                    // no-op
                }
                len => panic!("Invalid data array size of {} items", len),
            }
        }
        upgrade(&mut self.data_storage)
    }
}
impl<DataType: ChunkDataType + Copy> Default for PaletteStorage<DataType> {
    fn default() -> Self {
        Self {
            palette: smallvec![DataType::default()],
            data_storage: smallvec![0],
            last_gc_palette_len: 0,
        }
    }
}
impl<DataType: ChunkDataType + Copy> ChunkStorage<DataType> for PaletteStorage<DataType> {
    fn copy_dense(&self, output: &mut [DataType; CHUNK_DIM3Z]) {
        for (input, output) in self.iter().zip_eq(output.iter_mut()) {
            *output = *input;
        }
    }

    #[inline]
    fn get(&self, position: InChunkPos) -> &DataType {
        match self.data() {
            SafePaletteIndices::Singleton => &self.palette[0],
            SafePaletteIndices::U8(indices) => &self.palette[indices[position.as_index()] as usize],
            SafePaletteIndices::U16(indices) => &self.palette[indices[position.as_index()] as usize],
        }
    }

    #[inline]
    fn get_copy(&self, position: InChunkPos) -> DataType {
        match self.data() {
            SafePaletteIndices::Singleton => self.palette.first().copied().unwrap_or_default(),
            SafePaletteIndices::U8(indices) => self.palette[indices[position.as_index()] as usize],
            SafePaletteIndices::U16(indices) => self.palette[indices[position.as_index()] as usize],
        }
    }

    #[inline]
    fn put(&mut self, position: InChunkPos, new_value: DataType) -> DataType {
        let palette_pos = self.palette_get_or_insert(new_value, position);
        match self.data_mut() {
            SafePaletteIndicesMut::Singleton => {
                if palette_pos == 0 {
                    return self.palette.get(0).copied().unwrap_or_default();
                }
            }
            SafePaletteIndicesMut::U8(indices) => {
                if palette_pos <= u8::MAX as u16 {
                    let old_idx = std::mem::replace(&mut indices[position.as_index()], palette_pos as u8);
                    return self.palette.get(old_idx as usize).copied().unwrap_or_default();
                }
            }
            SafePaletteIndicesMut::U16(indices) => {
                let old_idx = std::mem::replace(&mut indices[position.as_index()], palette_pos);
                return self.palette.get(old_idx as usize).copied().unwrap_or_default();
            }
        }
        // Needs upgrade, otherwise an early return is used above
        self.upgrade_storage();
        match self.data_mut() {
            SafePaletteIndicesMut::Singleton => unreachable!(),
            SafePaletteIndicesMut::U8(indices) => {
                if palette_pos <= u8::MAX as u16 {
                    let old_idx = std::mem::replace(&mut indices[position.as_index()], palette_pos as u8);
                    return self.palette.get(old_idx as usize).copied().unwrap_or_default();
                } else {
                    unreachable!();
                }
            }
            SafePaletteIndicesMut::U16(indices) => {
                let old_idx = std::mem::replace(&mut indices[position.as_index()], palette_pos);
                return self.palette.get(old_idx as usize).copied().unwrap_or_default();
            }
        }
    }

    fn fill(&mut self, range: InChunkRange, new_value: DataType) {
        if range.is_empty() {
            return;
        }
        let palette_pos = self.palette_get_or_insert(new_value, range.min());

        let min = range.min();
        let max = range.max();

        match self.data_mut() {
            SafePaletteIndicesMut::Singleton => {
                if palette_pos == 0 {
                    return;
                }
            }
            SafePaletteIndicesMut::U8(indices) => {
                if palette_pos <= u8::MAX as u16 {
                    for (y, z) in iproduct!(min.y..=max.y, min.z..=max.z) {
                        let start_idx = (y * CHUNK_DIM2 + z * CHUNK_DIM + min.x) as usize;
                        let end_idx = (y * CHUNK_DIM2 + z * CHUNK_DIM + max.x) as usize;
                        indices[start_idx..=end_idx].fill(palette_pos as u8);
                    }
                    return;
                }
            }
            SafePaletteIndicesMut::U16(indices) => {
                for (y, z) in iproduct!(min.y..=max.y, min.z..=max.z) {
                    let start_idx = (y * CHUNK_DIM2 + z * CHUNK_DIM + min.x) as usize;
                    let end_idx = (y * CHUNK_DIM2 + z * CHUNK_DIM + max.x) as usize;
                    indices[start_idx..=end_idx].fill(palette_pos);
                }
                return;
            }
        }
        // Needs upgrade, otherwise an early return is used above. This should only recurse at most once.
        self.upgrade_storage();
        self.fill(range, new_value);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn palette_set() {
        let mut chunk: PaletteStorage<u64> = PaletteStorage::default();
        let zero_arr: Box<[u64; CHUNK_DIM3Z]> = bytemuck::zeroed_box();
        let mut one_arr: Box<[u64; CHUNK_DIM3Z]> = bytemuck::zeroed_box();
        one_arr.fill(1);
        let mut out_arr: Box<[u64; CHUNK_DIM3Z]> = bytemuck::zeroed_box();

        for idx in 0..CHUNK_DIM3Z {
            assert_eq!(0, chunk.get_copy(InChunkPos::try_from_index(idx).unwrap()));
        }
        chunk.copy_dense(&mut out_arr);
        assert_eq!(&zero_arr[..], &out_arr[..]);

        chunk.fill(InChunkRange::WHOLE_CHUNK, 1);
        for idx in 0..CHUNK_DIM3Z {
            assert_eq!(1, chunk.get_copy(InChunkPos::try_from_index(idx).unwrap()));
        }
        chunk.copy_dense(&mut out_arr);
        assert_eq!(&one_arr[..], &out_arr[..]);

        for idx in 0..CHUNK_DIM3Z {
            chunk.put(InChunkPos::try_from_index(idx).unwrap(), idx as u64);
        }
        for idx in 0..CHUNK_DIM3Z {
            assert_eq!(chunk.get_copy(InChunkPos::try_from_index(idx).unwrap()), idx as u64);
        }

        chunk.fill(InChunkRange::WHOLE_CHUNK, 1_000_000);
        chunk.fill(
            InChunkRange::from_corners(
                InChunkPos::ZERO,
                InChunkPos::try_new(CHUNK_DIM - 1, 8, CHUNK_DIM - 1).unwrap(),
            ),
            2_000_000,
        );

        for (pos, val) in chunk.iter_with_coords() {
            if pos.y <= 8 {
                assert_eq!(*val, 2_000_000);
            } else {
                assert_eq!(*val, 1_000_000);
            }
        }
    }
}
