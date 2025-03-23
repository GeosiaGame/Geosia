//! Bevy [`Plugin`] for the client voxel universe functionality.
use bevy::prelude::*;
use gs_common::voxel::plugin::VoxelUniversePlugin;

use crate::voxel::meshgen::ChunkMeshMaterial;
use crate::ClientData;

/// Initializes the required plugins for client-side voxel universe support.
#[derive(Default)]
pub struct VoxelUniverseClientPlugin;

impl Plugin for VoxelUniverseClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(VoxelUniversePlugin::<ClientData>::new())
            .add_plugins(MaterialPlugin::<ChunkMeshMaterial>::default());
    }
}
