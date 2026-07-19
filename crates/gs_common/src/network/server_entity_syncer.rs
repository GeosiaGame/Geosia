//! Synchronizes entity state over the network.

use bevy::ecs::world::EntityRefExcept;
use gs_schemas::schemas::network_capnp::PacketId;
use gs_schemas::schemas::{CapnpExt, new_packet_builder};

use crate::network::server::{ConnectedPlayer, ServerPlayerJoined};
use crate::network::server_packet_handler::BootstrappedGameDataTag;
use crate::network::transport::PacketWrapper;
use crate::prelude::*;
use crate::{GameServerResource, InGameSystemSet};

/// A bevy plugin that registers the systems for syncing entities.
pub fn server_entity_syncer_plugin(app: &mut App) {
    app.add_systems(
        FixedPostUpdate,
        (dirty_entity_sync, first_entity_sync).chain().in_set(InGameSystemSet),
    );
}

/// Sends the entity to nearby (or all) clients and keeps all networked components in sync automatically.
#[derive(Clone, Component)]
#[require(EntityNetworkId)]
pub struct ServerToClientSyncableEntity {
    /// The registry name for the entity type to spawn/sync on the other side.
    pub registry_name: RegistryName,
}

impl ServerToClientSyncableEntity {
    /// Constructs a fresh marker for network sync, must match a name in the entity registry
    pub fn new(registry_name: RegistryName) -> Self {
        Self { registry_name }
    }
}

/// Put on connected players who have alrady received the first batch of entity data.
#[derive(Copy, Clone, Component)]
pub struct PlayerHadFirstEntitySyncTag;

/// Sends all entity data to players who have not received any yet.
fn first_entity_sync(
    engine: Res<GameServerResource>,
    players: Populated<
        (Entity, &ConnectedPlayer),
        (Without<PlayerHadFirstEntitySyncTag>, With<BootstrappedGameDataTag>),
    >,
    to_sync: Query<(EntityRef, &ServerToClientSyncableEntity, &EntityNetworkId), Without<ConnectedPlayer>>,
    mut commands: Commands,
) -> BevyResult {
    let engine = &*engine.0;
    let entity_registry = &*engine.shared_registries.entity_types;

    let mut packet = new_packet_builder::<gs_schemas::schemas::network_capnp::entity_data_stream_packet::Owned>();
    let mut root = packet.init_root();
    root.set_id(PacketId::EntityData);
    root.set_timestamp_ms(engine.network_thread.packet_timestamp());
    let mut payload = root.init_payload();
    let entity_count = to_sync.count();
    let mut new_entities = payload.init_new_entities(entity_count.try_into()?);
    let mut buffer: Vec<u8> = Vec::new();
    for (i, (entity, syncable, nid)) in to_sync.iter().enumerate() {
        let mut new_entity = new_entities.reborrow().get(i as u32);
        let (rid, schema) = entity_registry
            .lookup_name_to_object(syncable.registry_name.as_ref())
            .context("entity_registry.lookup_name_to_object")?;
        new_entity.set_registry_id(rid.0.get());
        nid.0.get().write_to_message(&mut new_entity.reborrow().init_nid());
        buffer.clear();
        (schema.serialize_full)(entity, &mut buffer)?;
        new_entity.set_serialized(&buffer[..])?;
    }
    let mut packet = PacketWrapper::from(packet);

    players.iter().for_each(|(player_entity, player)| {
        let Ok(mut e) = commands.get_entity(player_entity) else {
            return;
        };
        // TODO: dedicated stream
        let _ = player.main_s2c_stream.send_packet(packet.clone_mut());
        e.insert(PlayerHadFirstEntitySyncTag);
    });
    Ok(())
}

fn dirty_entity_sync(
    engine: Res<GameServerResource>,
    players: Populated<(Entity, &ConnectedPlayer), (With<PlayerHadFirstEntitySyncTag>, With<BootstrappedGameDataTag>)>,
    to_sync: Query<(EntityRef, &ServerToClientSyncableEntity, &EntityNetworkId), Without<ConnectedPlayer>>,
    mut commands: Commands,
) -> BevyResult {
    Ok(())
}
