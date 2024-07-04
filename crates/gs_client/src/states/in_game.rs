//! The state for when the player is in game, with all basic gameplay resources fully loaded.

use bevy::prelude::*;

use crate::states::ClientAppState;
use crate::ClientNetworkThreadHolder;

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
