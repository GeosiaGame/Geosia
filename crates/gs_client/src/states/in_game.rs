//! The state for when the player is in game, with all basic gameplay resources fully loaded.

use bevy::prelude::*;
use gs_common::raycast::{RaycastContext, raycast};
use gs_common::voxel::blocks::STONE_BLOCK_NAME;
use gs_common::voxel::plugin::BlockRegistryHolder;
use gs_schemas::actions::{PositionData, ThrowAction};
use gs_schemas::coordinates::WorldPos;
use gs_schemas::raycast::{RaycastHitMask, RaycastResult, RaycastSpec};
use gs_schemas::voxel::chunk_storage::ChunkStorage;
use gs_schemas::voxel::voxeltypes::{BlockEntry, EMPTY_BLOCK_NAME};

use crate::ClientNetworkThreadHolder;
use crate::states::ClientAppState;
use crate::voxel::ClientVoxelUniverse;

/// The "plugin" implementing the in game state.
pub struct InGamePlugin;

impl Plugin for InGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnExit(ClientAppState::InGame), ingame_cleanup_on_exit);
    }
}

fn ingame_cleanup_on_exit(net_thread: ResMut<ClientNetworkThreadHolder>) {
    net_thread.0.sync_shutdown();
}

/// sends a throw packet over the given net thread and promise holder
pub(crate) fn ingame_send_throw_packet(
    net_thread: &Res<ClientNetworkThreadHolder>,
    voxel_query: &mut Query<&mut ClientVoxelUniverse>,
    block_reg: &Res<BlockRegistryHolder>,
    position: PositionData,
    throw: ThrowAction,
) {
    let _ = net_thread.0.schedule_task(async move |state| {
        let auth_rpc = state.borrow().server_auth_rpc().cloned();
        if let Some(auth_rpc) = auth_rpc {
            let mut rq = auth_rpc.send_throw_action_request();
            position.to_builder(&mut rq.get().init_position());
            throw.to_builder(&mut rq.get().init_throw());
            rq.get().set_tick(0); // TODO send the actual client tick
            let _ = rq.send().promise.await;
        }
        Ok(())
    });

    let limit = 64.0;
    let Ok(voxels) = &mut voxel_query.get_single_mut() else {
        return;
    };
    let ray_ctx = RaycastContext {
        block_registry: Some(block_reg),
        voxel_world: Some(voxels),
    };
    let ray_spec = RaycastSpec {
        start: WorldPos::from_offset_blockpos(position.position, position.offset.into()),
        direction: Dir3::new(position.look).unwrap().into(),
        distance_limit: limit,
        hit_mask: RaycastHitMask::all(),
    };

    let rc = raycast(&ray_ctx, &ray_spec);
    let RaycastResult::BlockHit(rc) = rc else {
        return;
    };
    let pos = if let ThrowAction::ThrowBlock() = throw {
        rc.position.direction_offset(rc.face, 1)
    } else {
        rc.position
    };
    let (i_stone, _) = block_reg.lookup_name_to_object(STONE_BLOCK_NAME.as_ref()).unwrap();
    let (i_empty, _) = block_reg.lookup_name_to_object(EMPTY_BLOCK_NAME.as_ref()).unwrap();

    let (chunk, local) = pos.split_chunk_component();
    if let Some(chunk) = voxels.loaded_chunks_mut().get_chunk_mut(chunk) {
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
