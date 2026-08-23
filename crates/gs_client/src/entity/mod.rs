pub mod remote_player_avatar;
pub mod standard_entities;

use crate::prelude::*;

/// Registers everything related to client entities with Bevy.
pub fn client_entities_plugin(app: &mut App) {
    app.add_plugins(remote_player_avatar::remote_player_avatar_plugin);
}
