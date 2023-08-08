//! Array-backed storage

use std::iter::repeat;

use crate::coordinates::{InChunkPos, InChunkRange, CHUNK_DIM3Z};
use crate::voxel::chunk_storage::{ChunkDataType, ChunkStorage};

/// Simple XZY dense array storage for chunk data (with strides of X=1, Z=32, Y=32²).
#[derive(Clone, Eq, PartialEq)]
pub enum ArrayStorage<T: ChunkDataType> {
    /// Single-element case for cases where every single chunk element is identical
    Singleton(T),
    /// Case where at least one element in a chunk is different
    Array(Box<[T; CHUNK_DIM3Z]>),
}

impl<T: ChunkDataType + Default> Default for ArrayStorage<T> {
    fn default() -> Self {
        Self::Singleton(T::default())
    }
}

impl<T: ChunkDataType> ArrayStorage<T> {
    #[cold]
    fn upgrade(&mut self) -> &mut Box<[T; CHUNK_DIM3Z]> {
        match self {
            Self::Array(arr) => arr,
            Self::Singleton(e) => {
                let new_arr: Box<[T; CHUNK_DIM3Z]> = Vec::from_iter(repeat(e.clone()).take(CHUNK_DIM3Z))
                    .into_boxed_slice()
                    .try_into()
                    .unwrap();
                *self = Self::Array(new_arr);
                let Self::Array(arr) = self else { unreachable!() };
                arr
            }
        }
    }
}

impl<T: ChunkDataType> ChunkStorage<T> for ArrayStorage<T> {
    fn copy_dense(&self, output: &mut [T; CHUNK_DIM3Z]) {
        match self {
            Self::Singleton(e) => output.fill(e.clone()),
            Self::Array(arr) => output.clone_from(arr),
        }
    }

    fn get(&self, position: InChunkPos) -> &T {
        match self {
            Self::Singleton(e) => e,
            Self::Array(arr) => &arr[position.as_index()],
        }
    }

    fn get_copy(&self, position: InChunkPos) -> T
    where
        T: Copy,
    {
        match self {
            Self::Singleton(e) => *e,
            Self::Array(arr) => arr[position.as_index()],
        }
    }

    fn put(&mut self, position: InChunkPos, new_value: T) -> T {
        match self {
            Self::Singleton(e) => {
                if e == &new_value {
                    e.clone()
                } else {
                    std::mem::replace(&mut self.upgrade()[position.as_index()], new_value)
                }
            }
            Self::Array(arr) => std::mem::replace(&mut arr[position.as_index()], new_value),
        }
    }

    fn fill(&mut self, range: InChunkRange, new_value: T) {
        if range.is_everything() {
            *self = Self::Singleton(new_value);
        } else {
            let arr = match self {
                Self::Singleton(old_value) => {
                    if old_value == &new_value {
                        // nothing changed
                        return;
                    }
                    self.upgrade()
                }
                Self::Array(arr) => arr,
            };
            for coord in range.iter_xzy() {
                arr[coord.as_index()] = new_value.clone();
            }
        }
    }
}
