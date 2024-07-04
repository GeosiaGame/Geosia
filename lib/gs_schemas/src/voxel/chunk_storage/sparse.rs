//! Sparse HashMap-backed storage

use hashbrown::HashMap;

use crate::coordinates::{InChunkPos, InChunkRange, CHUNK_DIM3Z};
use crate::voxel::chunk_storage::{ChunkDataType, ChunkStorage};

/// Storage for sparse chunk data, only allocating data for the data that's present at the cost of slower lookups and writes.
/// Filled by the `Default` value by default.
#[derive(Default, Clone, Eq, PartialEq)]
pub struct SparseStorage<DataType: ChunkDataType + Default> {
    data: HashMap<u16, DataType>,
    default_value: DataType, // cached default for the API
}

impl<DataType: ChunkDataType + Default> ChunkStorage<DataType> for SparseStorage<DataType> {
    fn copy_dense(&self, output: &mut [DataType; CHUNK_DIM3Z]) {
        for (i, data) in output.iter_mut().enumerate() {
            *data = self.data.get(&(i as u16)).cloned().unwrap_or_default();
        }
    }

    fn get(&self, position: InChunkPos) -> &DataType {
        self.data
            .get(&(position.as_index() as u16))
            .unwrap_or(&self.default_value)
    }

    fn get_copy(&self, position: InChunkPos) -> DataType
    where
        DataType: Copy,
    {
        self.data
            .get(&(position.as_index() as u16))
            .copied()
            .unwrap_or_default()
    }

    fn put(&mut self, position: InChunkPos, new_value: DataType) -> DataType {
        let idx = position.as_index() as u16;
        self.data.insert(idx, new_value).unwrap_or_default()
    }

    fn fill(&mut self, range: InChunkRange, new_value: DataType) {
        for coord in range.iter_xzy() {
            self.data.insert(coord.as_index() as u16, new_value.clone());
        }
    }
}
