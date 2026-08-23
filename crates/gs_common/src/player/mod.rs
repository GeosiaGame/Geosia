//! The server-side model of player data and behaviour.
//! See [`gs_schemas::player`] for more information.

use gs_schemas::coordinates::{AbsBlockPos, WorldPos};
use gs_schemas::player::{AccountId, CharacterId, PlayerAccount, PlayerCharacter};

use crate::entity::standard_entities::PLAYER_AVATAR_ENTITY_NAME;
use crate::network::server::{ConnectedPlayer, ServerPlayerJoined, ServerPlayerLeft};
use crate::prelude::*;
use crate::voxel::plugin::ChunkLoader;

/// A bevy plugin for registering the shared player information resources with the engine.
pub fn player_data_common_plugin(app: &mut App) {
    app.insert_resource(PlayerCache::default());
    app.add_observer(server_create_avatar_for_joining_player);
    app.add_observer(server_destroy_avatar_for_leaving_player);
}

/// A bevy plugin for registering the server-side player information resources with the engine.
/// Automatically includes [`player_data_common_plugin`].
pub fn player_data_server_plugin(app: &mut App) {
    app.add_plugins(player_data_common_plugin);
}

/// Caches [`PlayerAccount`]s and [`PlayerCharacter`]s.
#[derive(Resource, Default, Debug)]
pub struct PlayerCache {
    #[allow(dead_code)]
    accounts_by_uuid: HashMap<AccountId, Arc<PlayerAccount>>,
    #[allow(dead_code)]
    characters_by_uuid: HashMap<CharacterId, Arc<PlayerCharacter>>,
}

/// A link between a [`ServerPlayerAvatar`] and a [`ConnectedPlayer`], inserted on the [`ConnectedPlayer`] entity.
#[derive(Clone, Component, FromTemplate)]
#[relationship(relationship_target = ServerPlayerAvatarControlled)]
pub struct ServerPlayerAvatarController(Entity);

impl ServerPlayerAvatarController {
    /// Returns the ID of the player avatar controlled by this entity.
    pub fn server_player_avatar_id(&self) -> Entity {
        self.0
    }
}

/// A link between a [`ServerPlayerAvatar`] and a [`ConnectedPlayer`], inserted on the [`ServerPlayerAvatar`] entity.
#[derive(Clone, Component, FromTemplate)]
#[relationship_target(relationship = ServerPlayerAvatarController)]
pub struct ServerPlayerAvatarControlled(Entity);

impl ServerPlayerAvatarControlled {
    /// Returns the ID of the player controlling this entity.
    pub fn connected_player_id(&self) -> Entity {
        self.0
    }
}

/// Links an entity inside the game universe to a [`PlayerCharacter`], acting as its in-game avatar.
/// Co-exists on the player entity with:
/// - [`UniverseTransform`]
/// - [`ServerToClientSyncableEntity`]
#[derive(Debug, Clone, SceneComponent, FromTemplate)]
pub struct ServerPlayerAvatar {
    #[allow(dead_code)]
    /// The ID of the account owning this avatar.
    pub account_id: AccountId,
    /// The ID of the character represented by this avatar.
    pub character_id: CharacterId,
}

#[allow(clippy::new_without_default)] // Conflicts with FromTemplate
impl ServerPlayerAvatar {
    /// Constructs a new avatar with default field values
    pub fn new() -> Self {
        Self {
            account_id: AccountId::default(),
            character_id: CharacterId::default(),
        }
    }

    /// Creates a full [`ServerPlayerAvatar`] with all the necessary components present.
    fn scene() -> impl Scene {
        bsn! {
            #ServerPlayerAvatar
            ServerPlayerAvatar
            UniverseTransform
            ServerToClientSyncableEntity {
                registry_id: PLAYER_AVATAR_ENTITY_NAME
            }
            ChunkLoader
        }
    }
}

fn server_create_avatar_for_joining_player(
    event: On<ServerPlayerJoined>,
    player_info: Query<&ConnectedPlayer>,
    mut commands: Commands,
) -> BevyResult {
    let player_info = player_info.get(event.connected_player)?;
    let account_id = player_info.authenticated_info.player_character.account.id;
    let character_id = player_info.authenticated_info.player_character.id;

    let avatar = commands
        .spawn_scene(bsn! {
            @ServerPlayerAvatar {
                account_id: account_id,
                character_id: character_id,
            }
            UniverseTransform {
                position: {WorldPos::from_blockpos(AbsBlockPos::new(0, 6, 12))},
            }
        })
        .id();

    commands
        .entity(event.connected_player)
        .insert(ServerPlayerAvatarController(avatar));

    info!(
        "Spawned avatar {:?} for joining player {:?} {:?}",
        avatar, event.connected_player, player_info.authenticated_info.address
    );

    Ok(())
}

fn server_destroy_avatar_for_leaving_player(
    event: On<ServerPlayerLeft>,
    avatars: Query<(Entity, Option<&ServerPlayerAvatarControlled>)>,
    mut commands: Commands,
) -> BevyResult {
    for (avatar_id, controlled) in avatars {
        if controlled.map(ServerPlayerAvatarControlled::connected_player_id) == Some(event.connected_player) {
            commands.entity(avatar_id).try_despawn();
        }
    }

    Ok(())
}
