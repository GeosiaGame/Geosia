//! The server-side model of player data and behaviour.
//! See [`gs_schemas::player`] for more information.

use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use gs_schemas::coordinates::{AbsBlockPos, WorldPos};
use gs_schemas::player::{AccountId, CharacterId, PlayerAccount, PlayerCharacter};

use crate::entity::standard_entities::PLAYER_AVATAR_ENTITY_NAME;
use crate::network::server::{ConnectedPlayer, ServerPlayerJoined, ServerPlayerLeft};
use crate::prelude::*;

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
    avatars_by_character: HashMap<CharacterId, Entity>,
}

/// A link between a [`ServerPlayerAvatar`] and a [`ConnectedPlayer`], inserted on the [`ConnectedPlayer`] entity.
#[derive(Clone, Component)]
#[relationship(relationship_target = ServerPlayerAvatar)]
pub struct HasServerPlayerAvatar(Entity);

/// Links an entity inside the game universe to a [`PlayerCharacter`], acting as its in-game avatar.
#[derive(Debug, Component)]
#[component(on_add = Self::on_add, on_discard = Self::on_discard)]
#[require(UniverseTransform, ServerToClientSyncableEntity = ServerToClientSyncableEntity::new(PLAYER_AVATAR_ENTITY_NAME))]
#[relationship_target(relationship = HasServerPlayerAvatar)]
pub struct ServerPlayerAvatar {
    #[allow(dead_code)]
    /// Do not ever modify after construction.
    pub account: AccountId,
    /// Do not ever modify after construction.
    pub character: CharacterId,
    #[relationship]
    connected_player: Entity,
}

impl ServerPlayerAvatar {
    pub fn new(account: AccountId, character: CharacterId) -> Self {
        Self {
            account,
            character,
            connected_player: Entity::PLACEHOLDER,
        }
    }

    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        let Some(avatar) = world.entity(ctx.entity).get::<ServerPlayerAvatar>() else {
            return;
        };
        let character = avatar.character;
        let Some(mut cache) = world.get_resource_mut::<PlayerCache>() else {
            return;
        };
        if let Err(e) = cache.avatars_by_character.try_insert(character, ctx.entity) {
            error!("Could not insert player character {character:?} into the cache as one already exists: {e}");
        }
    }

    fn on_discard(mut world: DeferredWorld, ctx: HookContext) {
        let Some(avatar) = world.entity(ctx.entity).get::<ServerPlayerAvatar>() else {
            return;
        };
        let character = avatar.character;
        let Some(mut cache) = world.get_resource_mut::<PlayerCache>() else {
            return;
        };
        let cached = cache.avatars_by_character.entry(character);
        match cached {
            hashbrown::hash_map::Entry::Occupied(occupied) if *occupied.get() == ctx.entity => {
                occupied.remove();
            }
            hashbrown::hash_map::Entry::Occupied(_) => {
                error!(
                    "Could not remove player character {character:?} from the cache as the mapping points to a different character"
                );
            }
            hashbrown::hash_map::Entry::Vacant(_) => {
                error!("Could not remove player character {character:?} from the cache as it doesn't exist");
            }
        }
    }
}

fn server_create_avatar_for_joining_player(
    event: On<ServerPlayerJoined>,
    player_info: Query<&ConnectedPlayer>,
    mut commands: Commands,
) -> BevyResult {
    let player_info = player_info.get(event.connected_player)?;

    let avatar = commands.spawn((
        ServerPlayerAvatar {
            account: player_info.authenticated_info.player_character.account.id,
            character: player_info.authenticated_info.player_character.id,
            connected_player: event.connected_player,
        },
        UniverseTransform {
            position: WorldPos::from_blockpos(AbsBlockPos::new(0, 6, 12)),
        },
    ));
    let avatar = avatar.id();
    commands
        .get_entity(event.connected_player)?
        .insert(HasServerPlayerAvatar(avatar));

    Ok(())
}

fn server_destroy_avatar_for_leaving_player(
    event: On<ServerPlayerLeft>,
    player_info: Query<&ConnectedPlayer>,
    mut commands: Commands,
) -> BevyResult {
    // TODO

    Ok(())
}
