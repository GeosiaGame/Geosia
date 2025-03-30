//! The state for when the player is in game, with all basic gameplay resources fully loaded.

use bevy::prelude::*;
use gs_common::prelude::GenericAsyncResult;
use gs_common::raycast::{raycast, RaycastContext};
use gs_common::ServerData;
use gs_common::voxel::blocks::STONE_BLOCK_NAME;
use gs_common::voxel::plugin::{BlockRegistryHolder, VoxelUniverse};
use gs_schemas::actions::{PositionData, ThrowAction};
use gs_schemas::coordinates::WorldPos;
use gs_schemas::raycast::{RaycastHitMask, RaycastResult, RaycastSpec};
use gs_schemas::schemas::game_types_capnp::position_data;
use gs_schemas::voxel::chunk_storage::ChunkStorage;
use gs_schemas::voxel::voxeltypes::{BlockEntry, EMPTY_BLOCK_NAME};
use crate::ClientNetworkThreadHolder;
use crate::states::ClientAppState;
use crate::voxel::ClientVoxelUniverse;

/// The "plugin" implementing the in game state.
pub struct InGamePlugin;

impl Plugin for InGamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InGamePromiseHolder>();
        app.add_systems(OnExit(ClientAppState::InGame), ingame_cleanup_on_exit);
    }
}

#[derive(Resource, Default)]
pub(crate) struct InGamePromiseHolder {
    promises: Vec<Box<dyn GenericAsyncResult + Send + Sync>>,
}

fn ingame_cleanup_on_exit(net_thread: ResMut<ClientNetworkThreadHolder>) {
    net_thread.0.sync_shutdown();
}

/// sends a throw packet over the given net thread and promise holder
pub(crate) fn ingame_send_throw_packet(net_thread: &Res<ClientNetworkThreadHolder>,  promises: &mut ResMut<InGamePromiseHolder>, voxel_query: &mut Query<&mut ClientVoxelUniverse>, bregistry: &Res<BlockRegistryHolder>, position: PositionData, throw: ThrowAction) {
    promises
        .promises
        .push(Box::new(net_thread.0.schedule_task(async move |state| {
            let auth_rpc = state.borrow().server_auth_rpc().cloned();
            if let Some(auth_rpc) = auth_rpc {
                let mut rq = auth_rpc.send_throw_action_request();
                position.to_builder(&mut rq.get().init_position());
                throw.to_builder(&mut rq.get().init_throw());
                let _ = rq.send().promise.await;
            }
            Ok(())
        })));
    
    let limit = 64.0;
    let Ok(voxels) = &mut voxel_query.get_single_mut() else {
        return;
    };
    let rcctx = RaycastContext {
        block_registry: Some(&bregistry),
        voxel_world: Some(voxels),
    };
    let rcspec = RaycastSpec {
        start: WorldPos::from_offset(position.position, position.offset.into()),
        direction: Dir3::new(position.look).unwrap().into(),
        distance_limit: limit,
        hit_mask: RaycastHitMask::all(),
    };

    let rc = raycast(&rcctx, &rcspec);
    let RaycastResult::BlockHit(rc) = rc else {
        return;
    };
    let pos = if let ThrowAction::ThrowBlock() = throw {
        rc.face.offset(&rc.position, 1)
    } else {
        rc.position
    };
    let (i_stone, _) = bregistry.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
    let (i_empty, _) = bregistry.lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref()).unwrap();

    let (chunk, local) = pos.split_chunk_component();
    if let Some(mut chunk) = voxels.loaded_chunks_mut().get_chunk_mut(chunk) {
        match throw {
            ThrowAction::ThrowBlock() => {
                chunk.mutate_predicted().blocks.put(local, BlockEntry::new(i_stone, 0));
            }
            ThrowAction::ThrowItem() => {
                chunk.mutate_predicted().blocks.put(local, BlockEntry::new(i_empty, 0));
            }
        }
    }
}