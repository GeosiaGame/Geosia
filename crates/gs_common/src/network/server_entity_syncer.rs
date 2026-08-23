//! Synchronizes entity state over the network.

use bevy::ecs::entity::EntityHashMap;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use gs_schemas::limits::ConnectedPlayersBitset;
use gs_schemas::schemas::network_capnp::PacketId;
use gs_schemas::schemas::{CapnpExt, new_packet_builder};
use smallvec::SmallVec;
use uuid::NonNilUuid;

use crate::entity::NetworkEntityRef;
use crate::network::SharedEntityRegistryIdResolverTemplate;
use crate::network::server::ConnectedPlayer;
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
#[derive(Component, FromTemplate)]
#[require(EntityNetworkId)]
#[component(on_discard = Self::on_discard)]
pub struct ServerToClientSyncableEntity {
    /// The registry name for the entity type to spawn/sync on the other side.
    #[template(SharedEntityRegistryIdResolverTemplate)]
    pub registry_id: RegistryId,
    /// List of players aware of the entity already.
    /// Use acquire-release ordering to access as the discard hook might access the data from a different thread than.
    aware_players: ConnectedPlayersBitset,
}

/// Put on the entity when its network-syncable components have changed.
#[derive(Component)]
#[component(immutable, storage = "SparseSet")]
pub struct ServerToClientEntityDirtyTag;

#[derive(Component, Clone, Debug, Eq, PartialEq)]
#[component(immutable)]
struct DespawnedEntityToSync {
    nid: EntityNetworkId,
    aware_players: ConnectedPlayersBitset,
}

impl ServerToClientSyncableEntity {
    /// Constructs a fresh marker for network sync, must match an entry in the entity registry
    pub fn new(registry_id: RegistryId) -> Self {
        Self {
            registry_id,
            aware_players: 0,
        }
    }

    fn on_discard(mut world: DeferredWorld, ctx: HookContext) {
        let nid = *world.get::<EntityNetworkId>(ctx.entity).unwrap();
        let aware_players = world.get::<Self>(ctx.entity).unwrap().aware_players;
        world.commands().spawn(DespawnedEntityToSync { nid, aware_players });
    }
}

/// Put on connected players who have alrady received the first batch of entity data.
#[derive(Copy, Clone, Component)]
#[component(immutable)]
pub struct PlayerHadFirstEntitySyncTag {
    /// Location in the [`ConnectedPlayersBitset`] corresponding to this player.
    bitset_loc: u32,
}

impl PlayerHadFirstEntitySyncTag {
    fn bitset_mask(&self) -> ConnectedPlayersBitset {
        (1 as ConnectedPlayersBitset) << self.bitset_loc
    }
}

/// Sends all entity data to players who have not received any yet.
fn first_entity_sync(
    engine: Res<GameServerResource>,
    existing_players: Query<&PlayerHadFirstEntitySyncTag, With<ConnectedPlayer>>,
    players: Populated<
        (Entity, &ConnectedPlayer),
        (Without<PlayerHadFirstEntitySyncTag>, With<BootstrappedGameDataTag>),
    >,
    mut to_sync: ParamSet<(
        Query<(NetworkEntityRef, &ServerToClientSyncableEntity, &EntityNetworkId), Without<ConnectedPlayer>>,
        // Mutable access to write the connected players set conflicts with EntityRef for dynamic serialization
        Query<&mut ServerToClientSyncableEntity, (With<EntityNetworkId>, Without<ConnectedPlayer>)>,
    )>,
    mut commands: Commands,
) -> BevyResult {
    let engine = &*engine.0;
    let entity_registry = &*engine.shared_registries.entity_types;

    let new_player_count = players.count();
    let existing_player_bits = existing_players
        .iter()
        .fold(0 as ConnectedPlayersBitset, |v, p| v | p.bitset_mask());
    let remaining_player_bits = existing_player_bits.count_zeros();
    assert!(
        remaining_player_bits as usize > new_player_count,
        "Max connected players constraint violated: remaining_bits={remaining_player_bits} new_count={new_player_count}"
    );
    let (new_player_bits, new_player_bitlocs) = {
        let mut new_player_bitlocs: SmallVec<[u32; 4]> = SmallVec::with_capacity(new_player_count);
        let mut new_player_bits = 0 as ConnectedPlayersBitset;

        for _ in 0..new_player_count {
            let all_bits = new_player_bits | existing_player_bits;
            let bitloc = (!all_bits)
                .lowest_one()
                .expect("Max connected players constraint violated");
            let bitmask = (1 as ConnectedPlayersBitset) << bitloc;
            new_player_bitlocs.push(bitloc);
            new_player_bits |= bitmask;
        }

        (new_player_bits, new_player_bitlocs)
    };

    let mut packet = new_packet_builder::<gs_schemas::schemas::network_capnp::entity_data_stream_packet::Owned>();
    let mut root = packet.init_root();
    root.set_id(PacketId::EntityData);
    root.set_timestamp_ms(engine.network_thread.packet_timestamp());
    let payload = root.init_payload();
    let entity_count = to_sync.p0().count();
    let mut new_entities = payload.init_new_entities(entity_count.try_into()?);
    let mut buffer: Vec<u8> = Vec::new();
    for (i, (entity, syncable, nid)) in to_sync.p0().iter().enumerate() {
        let mut new_entity = new_entities.reborrow().get(i as u32);
        let schema = entity_registry
            .lookup_id_to_object(syncable.registry_id)
            .context("entity_registry.lookup_name_to_object")?;
        new_entity.set_registry_id(syncable.registry_id.0.get());
        nid.0.get().write_to_message(&mut new_entity.reborrow().init_nid());
        buffer.clear();
        (schema.serialize_full)(entity, &mut buffer)?;
        new_entity.set_serialized(&buffer[..])?;
    }
    let mut packet = PacketWrapper::from(packet);
    for mut syncable in to_sync.p1().iter_mut() {
        syncable.aware_players = (syncable.aware_players & existing_player_bits) | new_player_bits;
    }

    for (i, (player_entity, player)) in players.iter().enumerate() {
        let Ok(mut e) = commands.get_entity(player_entity) else {
            continue;
        };
        // TODO: dedicated stream
        let _ = player.main_s2c_stream.send_packet(packet.clone_mut());
        e.insert(PlayerHadFirstEntitySyncTag {
            bitset_loc: new_player_bitlocs[i],
        });
    }
    Ok(())
}

fn dirty_entity_sync(
    engine: Res<GameServerResource>,
    players: Populated<(&ConnectedPlayer, &PlayerHadFirstEntitySyncTag), With<BootstrappedGameDataTag>>,
    mut to_sync: Query<
        (
            NetworkEntityRef,
            &mut ServerToClientSyncableEntity,
            &EntityNetworkId,
            Has<ServerToClientEntityDirtyTag>,
        ),
        Without<ConnectedPlayer>,
    >,
    to_delete: Query<
        (Entity, &DespawnedEntityToSync),
        (Without<ConnectedPlayer>, Without<ServerToClientSyncableEntity>),
    >,
    mut commands: Commands,
) -> BevyResult {
    let engine = &*engine.0;
    let entity_registry = &*engine.shared_registries.entity_types;

    let valid_player_bits = players
        .iter()
        .fold(0 as ConnectedPlayersBitset, |v, (_, p)| v | p.bitset_mask());

    let mut globally_deleted_entities: SmallVec<[_; 32]> = SmallVec::with_capacity(to_delete.count());
    for (e, to_sync) in to_delete {
        globally_deleted_entities.push(to_sync.clone());
        commands.entity(e).despawn();
    }

    let mut full_entity_data: EntityHashMap<(NonNilUuid, RegistryId, Rc<[u8]>)> = EntityHashMap::new();
    let mut delta_entity_data: EntityHashMap<(NonNilUuid, Rc<[u8]>)> = EntityHashMap::new();
    let mut buffer: Vec<u8> = Vec::new();

    let mut get_full_entity_data = |buffer: &mut Vec<u8>,
                                    e_ref: NetworkEntityRef,
                                    syncable: &ServerToClientSyncableEntity,
                                    nid: &EntityNetworkId|
     -> Result<_> {
        let entry = full_entity_data.entry(e_ref.entity());
        use bevy::platform::collections::hash_map::Entry;
        Ok(match entry {
            Entry::Occupied(entry) => entry.get().clone(),
            Entry::Vacant(entry) => {
                let schema = entity_registry
                    .lookup_id_to_object(syncable.registry_id)
                    .context("entity_registry.lookup_id_to_object")?;
                buffer.clear();
                (schema.serialize_full)(e_ref, buffer)?;
                entry
                    .insert((nid.0, syncable.registry_id, buffer.clone().into()))
                    .clone()
            }
        })
    };

    let mut get_entity_delta_data = |buffer: &mut Vec<u8>,
                                     e_ref: NetworkEntityRef,
                                     syncable: &ServerToClientSyncableEntity,
                                     nid: &EntityNetworkId|
     -> Result<_> {
        let entry = delta_entity_data.entry(e_ref.entity());
        use bevy::platform::collections::hash_map::Entry;
        Ok(match entry {
            Entry::Occupied(entry) => entry.get().clone(),
            Entry::Vacant(entry) => {
                let schema = entity_registry
                    .lookup_id_to_object(syncable.registry_id)
                    .context("entity_registry.lookup_id_to_object")?;
                buffer.clear();
                (schema.serialize_delta)(e_ref, buffer)?;
                entry.insert((nid.0, buffer.clone().into())).clone()
            }
        })
    };

    let packet_timestamp = engine.network_thread.packet_timestamp();
    let mut deletions: SmallVec<[NonNilUuid; 32]> = SmallVec::with_capacity(globally_deleted_entities.len());
    let mut insertions: SmallVec<[(NonNilUuid, RegistryId, Rc<[u8]>); 32]> = SmallVec::new();
    let mut deltas: SmallVec<[(NonNilUuid, Rc<[u8]>); 32]> = SmallVec::new();
    for (player, player_sync_tag) in players {
        let player_mask = player_sync_tag.bitset_mask();
        deletions.clear();
        for despawned in globally_deleted_entities.iter() {
            if despawned.aware_players & player_mask == player_mask {
                deletions.push(despawned.nid.0);
            }
        }

        for (e_ref, mut e_sync_info, nid, is_dirty) in to_sync.iter_mut() {
            let e_old_mask = e_sync_info.aware_players;
            let e_was_seen = (e_old_mask & player_mask) != 0;
            let e_should_be_seen = true;
            let e_new_mask = (e_old_mask & (!player_mask)) | if e_should_be_seen { player_mask } else { 0 };
            match (e_was_seen, e_should_be_seen, is_dirty) {
                (false, false, _) => {
                    // no-op, remains unseen
                }
                (false, true, _) => {
                    // needs to become seen, send full data
                    let full_data = get_full_entity_data(&mut buffer, e_ref, &e_sync_info, nid)?;
                    insertions.push(full_data);
                }
                (true, true, false) => {
                    // no-op, still seen but no changes
                }
                (true, true, true) => {
                    // still seen with changes
                    let changes = get_entity_delta_data(&mut buffer, e_ref, &e_sync_info, nid)?;
                    deltas.push(changes);
                }
                (true, false, _) => {
                    // was seen but becomes unseen, send removal
                    deletions.push(nid.0);
                }
            }
            e_sync_info.aware_players = e_new_mask & valid_player_bits;
        }

        let mut packet = new_packet_builder::<gs_schemas::schemas::network_capnp::entity_data_stream_packet::Owned>();
        let mut root = packet.init_root();
        root.set_id(PacketId::EntityData);
        root.set_timestamp_ms(packet_timestamp);
        let mut payload = root.init_payload();

        let mut msg_insertions = payload.reborrow().init_new_entities(insertions.len().try_into()?);
        for (i, (nid, rid, data)) in insertions.drain(..).enumerate() {
            let mut entry = msg_insertions.reborrow().get(i.try_into()?);
            nid.get().write_to_message(&mut entry.reborrow().init_nid());
            entry.set_registry_id(rid.0.get());
            entry.set_serialized(&data[..])?;
        }

        let mut msg_deltas = payload.reborrow().init_updated_entities(deltas.len().try_into()?);
        for (i, (nid, data)) in deltas.drain(..).enumerate() {
            let mut entry = msg_deltas.reborrow().get(i.try_into()?);
            nid.get().write_to_message(&mut entry.reborrow().init_nid());
            entry.set_serialized(&data[..])?;
        }

        let mut msg_deletions = payload.reborrow().init_deleted_entities(deletions.len().try_into()?);
        for (i, v) in deletions.drain(..).enumerate() {
            let mut entry = msg_deletions.reborrow().get(i.try_into()?);
            v.get().write_to_message(&mut entry);
        }

        let packet = PacketWrapper::from(packet);
        // TODO: dedicated stream
        let _ = player.main_s2c_stream.send_packet(packet);
    }

    for (e_ref, _e_sync_info, _nid, is_dirty) in to_sync.iter_mut() {
        if is_dirty {
            commands.entity(e_ref.id()).remove::<ServerToClientEntityDirtyTag>();
        }
    }

    Ok(())
}
