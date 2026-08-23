use std::borrow::Cow;
use std::collections::VecDeque;

use gs_common::InGameSystemSet;
use gs_common::network::server::QueuedPacket;
use gs_common::network::transport::RPC_CLIENT_READER_OPTIONS;
use gs_common::network::{EntityNetworkIdLookupTable, SharedRegistryHolder};
use gs_schemas::dependencies::itertools::Itertools;
use gs_schemas::schemas::network_capnp::entity_data_stream_packet;
use gs_schemas::schemas::{CapnpExt, capnp_bytes_to_cow};
use uuid::{NonNilUuid, Uuid};

use crate::network::client_packet_handlers::client_packet_handler_system;
use crate::prelude::*;
use crate::states::{ClientAppState, DESPAWN_OUT_OF_GAME, LoadingGameSystemSet};

/// Sets up entity syncing on the client side.
pub fn client_entity_syncer_plugin(app: &mut App) {
    app.insert_resource(NetworkEntityClient::default());
    app.add_systems(
        FixedPreUpdate,
        process_entity_packet_queue
            .before(InGameSystemSet)
            .before(LoadingGameSystemSet)
            .after(client_packet_handler_system)
            .run_if(
                SystemCondition::or_eager(in_state(ClientAppState::LoadingGame), in_state(ClientAppState::InGame))
                    .and_then(resource_exists::<SharedRegistryHolder>),
            ),
    );
}

/// Stores the state required for the entity sync protocol.
#[derive(Resource, Default)]
pub struct NetworkEntityClient {
    /// Stores `PacketId::EntityData` packets
    pub packet_queue: VecDeque<QueuedPacket>,
}

/// Marks the entity as coming from the server.
#[derive(Component, Debug)]
#[require(EntityNetworkId)]
pub struct ClientRemoteEntity {
    registry_id: RegistryId,
}

fn process_entity_packet_queue(world: &mut World) -> BevyResult {
    let registry = Arc::clone(&world.resource::<SharedRegistryHolder>().entity_types);
    let mut queue = std::mem::take(&mut world.resource_mut::<NetworkEntityClient>().packet_queue);
    for packet in queue.drain(..) {
        let root = packet
            .data
            .parse_typed::<entity_data_stream_packet::Owned>(RPC_CLIENT_READER_OPTIONS)?;
        let msg = root.get()?;
        let payload = msg.get_payload()?;
        for new_ent in payload.get_new_entities()? {
            let rid = RegistryId::try_new(new_ent.get_registry_id()).context("invalid new entity registry id")?;
            let nid = Uuid::read_from_message(&new_ent.get_nid()?)?;
            let nid = NonNilUuid::new(nid).context("invalid new entity network id")?;
            let etype = registry
                .lookup_id_to_object(rid)
                .context("missing new entity registry id")?;
            let data = new_ent.get_serialized()?;
            let data = capnp_bytes_to_cow(&data);
            let e = world.spawn((
                DESPAWN_OUT_OF_GAME,
                EntityNetworkId(nid),
                ClientRemoteEntity { registry_id: rid },
            ));
            (etype.deserialize_full)(&data, e)?;
        }
        for upd_ent in payload.get_updated_entities()? {
            let nid = Uuid::read_from_message(&upd_ent.get_nid()?)?;
            let nid = NonNilUuid::new(nid).context("invalid updated entity network id")?;
            let e = world
                .resource::<EntityNetworkIdLookupTable>()
                .get_entity(nid)
                .context("update for non-existent entity ID")?;
            let rid = world
                .entity(e)
                .get::<ClientRemoteEntity>()
                .context("missing client remote entity component on networked entity")?
                .registry_id;
            let etype = registry
                .lookup_id_to_object(rid)
                .context("missing new entity registry id")?;
            let data = upd_ent.get_serialized()?;
            let data = capnp_bytes_to_cow(&data);
            let e = world.entity_mut(e);
            (etype.deserialize_delta)(&data, e)?;
        }
        for del_ent in payload.get_deleted_entities()? {
            let nid = Uuid::read_from_message(&del_ent)?;
            let nid = NonNilUuid::new(nid).context("invalid deleted entity network id")?;
            let e = world
                .resource::<EntityNetworkIdLookupTable>()
                .get_entity(nid)
                .context("update for non-existent entity ID")?;
            world.entity_mut(e).despawn();
        }
    }
    world.resource_mut::<NetworkEntityClient>().packet_queue = queue;
    Ok(())
}
