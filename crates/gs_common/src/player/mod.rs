//! The server-side model of player data and behaviour.
//! See [`gs_schemas::player`] for more information.

use bevy::ecs::component::{Immutable, StorageType};
use bevy::ecs::lifecycle::{ComponentHook, HookContext};
use bevy::ecs::world::DeferredWorld;
use gs_schemas::player::{AccountId, CharacterId, PlayerAccount, PlayerCharacter};

use crate::prelude::*;

/// A bevy plugin for registering the shared player information resources with the engine.
pub fn player_data_common_plugin(app: &mut App) {
    app.insert_resource(PlayerCache::default());
}

/// A bevy plugin for registering the server-side player information resources with the engine.
/// Automatically includes [`player_data_common_plugin`].
pub fn player_data_server_plugin(app: &mut App) {
    app.add_plugins(player_data_common_plugin);
}

/// Caches [`PlayerAccount`]s and [`PlayerCharacter`]s.
#[derive(Resource, Default, Debug)]
pub struct PlayerCache {
    accounts_by_uuid: HashMap<AccountId, Arc<PlayerAccount>>,
    characters_by_uuid: HashMap<CharacterId, Arc<PlayerCharacter>>,
    avatars_by_character: HashMap<CharacterId, Entity>,
}

/// Links an entity inside the game universe to a [`PlayerCharacter`].
/// The entity persists even if the character is disconnected, laying dormant until it reconnects.
#[derive(Debug)]
pub struct PlayerCharacterComponent {
    account: AccountId,
    character: CharacterId,
}

impl Component for PlayerCharacterComponent {
    const STORAGE_TYPE: StorageType = StorageType::Table;
    type Mutability = Immutable;

    fn on_add() -> Option<ComponentHook> {
        Some(|mut world: DeferredWorld, ctx: HookContext| {
            let Some(avatar) = world.entity(ctx.entity).get::<PlayerCharacterComponent>() else {
                return;
            };
            let character = avatar.character;
            let Some(mut cache) = world.get_resource_mut::<PlayerCache>() else {
                return;
            };
            if let Err(e) = cache.avatars_by_character.try_insert(character, ctx.entity) {
                error!("Could not insert player character {character:?} into the cache as one already exists: {e}");
            }
        })
    }

    fn on_remove() -> Option<ComponentHook> {
        Some(|mut world: DeferredWorld, ctx: HookContext| {
            let Some(avatar) = world.entity(ctx.entity).get::<PlayerCharacterComponent>() else {
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
        })
    }
}
