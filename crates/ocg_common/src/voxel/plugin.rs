//! The Bevy plugin for voxel universe handling.

use std::marker::PhantomData;
use std::sync::Arc;

use bevy::prelude::*;
use ocg_schemas::voxel::voxeltypes::BlockRegistry;
use ocg_schemas::OcgExtraData;

use crate::voxel::persistence::{ChunkLoader, ChunkPersistenceLayer};
use crate::InGameSystemSet;

/// Initializes the settings related to the voxel universe.
#[derive(Default)]
pub struct VoxelUniversePlugin<ExtraData: OcgExtraData> {
    _extra_data: PhantomData<ExtraData>,
}

impl<ExtraData: OcgExtraData> Plugin for VoxelUniversePlugin<ExtraData> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            (system_process_chunk_loading::<ExtraData>).in_set(InGameSystemSet),
        );
    }

    fn name(&self) -> &str {
        "common::VoxelWorldPlugin"
    }

    fn is_unique(&self) -> bool {
        true
    }
}

impl<ExtraData: OcgExtraData> VoxelUniversePlugin<ExtraData> {
    /// Constructor.
    pub fn new() -> Self {
        Self {
            _extra_data: Default::default(),
        }
    }
}

/// The bevy [`Resource`] for voxel data access from systems.
#[derive(Resource)]
pub struct VoxelUniverse<ExtraData: OcgExtraData> {
    /// The types of registered blocks.
    pub block_registry: Arc<BlockRegistry>,
    /// Manages the chunks loaded in memory.
    pub chunk_loader: ChunkLoader<ExtraData>,
}

impl<ExtraData: OcgExtraData> VoxelUniverse<ExtraData> {
    /// Constructor.
    pub fn new(
        registry: Arc<BlockRegistry>,
        persistence_layer: Box<dyn ChunkPersistenceLayer<ExtraData>>,
        group_data: ExtraData::GroupData,
    ) -> Self {
        Self {
            block_registry: registry.clone(),
            chunk_loader: ChunkLoader::new(persistence_layer, group_data),
        }
    }
}

fn system_process_chunk_loading<ExtraData: OcgExtraData>(mut voxels: ResMut<VoxelUniverse<ExtraData>>) {
    let _voxels: &mut VoxelUniverse<_> = &mut voxels;
    // TODO: actually load chunks
}
