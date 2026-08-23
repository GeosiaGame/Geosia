//! Entity representing other players connected to the server.

use gs_common::InGameSystemSet;
use gs_schemas::player::{AccountId, CharacterId};

use crate::prelude::*;

pub(super) fn remote_player_avatar_plugin(app: &mut App) {
    app.add_systems(
        PostUpdate,
        (debug_draw_remote_avatars)
            .in_set(InGameSystemSet)
            .after(TransformSystems::Propagate),
    );
}

/// Represents another player connected to the server.
/// Co-exists on the avatar entity with:
/// - [`UniverseTransform`]
#[derive(Debug, Clone, SceneComponent, FromTemplate)]
pub struct RemotePlayerAvatar {
    #[allow(dead_code)]
    /// The ID of the account owning this avatar.
    pub account_id: AccountId,
    /// The ID of the character represented by this avatar.
    pub character_id: CharacterId,
}

#[allow(clippy::new_without_default)] // Conflicts with FromTemplate
impl RemotePlayerAvatar {
    /// Constructs a new avatar with default field values
    pub fn new() -> Self {
        Self {
            account_id: AccountId::default(),
            character_id: CharacterId::default(),
        }
    }

    /// Creates a full [`RemotePlayerAvatar`] with all the necessary components present.
    fn scene() -> impl Scene {
        bsn! {
            #RemotePlayerAvatar
            RemotePlayerAvatar
            UniverseTransform
        }
    }
}

fn debug_draw_remote_avatars(avatars: Populated<(&RemotePlayerAvatar, &GlobalTransform), ()>, mut gizmos: Gizmos) {
    for (avatar, gtf) in avatars {
        let mut iso = gtf.to_isometry();
        gizmos.sphere(iso, 2.0f32, bevy::color::palettes::tailwind::GREEN_500);
        iso.translation += vec3a(0f32, 2.1f32, 0f32);
        iso.rotation *= Quat::from_rotation_y(std::f32::consts::PI);
        let text = format!("{}", avatar.character_id.0.get().as_braced());
        gizmos.text(
            iso,
            &text,
            0.75f32,
            vec2(0f32, -0.5f32),
            bevy::color::palettes::tailwind::GRAY_50,
        );
    }
}
