//! The networking layer of the game.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;

use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::template::TemplateContext;
use bevy::ecs::world::DeferredWorld;
use smallvec::SmallVec;
use thiserror::Error;
use uuid::{NonNilUuid, Uuid};

use crate::prelude::*;
use crate::registries::GameRegistries;

pub mod server;
pub mod server_entity_syncer;
pub mod server_packet_handler;
pub mod thread;
pub mod transport;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
/// Address uniquely identifying a connected network or local peer (client or server on the other side of the connection).
pub enum PeerAddress {
    /// A local, in-process connection with a given ID to distinguish multiple local connections used in tests.
    Local(i32),
    /// A remote, over-the-network connection to a given peer at the specified IP address and port, connected to a local IP and port.
    Network {
        /// The local network interface address and port bound for this peer
        local: SocketAddr,
        /// The peer's address and port
        remote: SocketAddr,
    },
}

impl PeerAddress {
    /// Obtains the underlying socket address for a remote address, or None for other types.
    pub fn remote_addr(self) -> Option<SocketAddr> {
        match self {
            PeerAddress::Network { remote, .. } => Some(remote),
            _ => None,
        }
    }
}

impl Display for PeerAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(loc) => write!(f, "Local:{loc}"),
            Self::Network { local, remote } => write!(f, "Remote:({local} -> {remote})"),
        }
    }
}

/// A [`Resource`] holding the shared network registries copy for the currently running game.
#[derive(Clone, Resource, Deref, DerefMut)]
pub struct SharedRegistryHolder(pub GameRegistries);

macro_rules! impl_shared_registry_resolver {
    ($tmpl_name: ident, $registry: ident) => {
        #[doc = concat!("A [`Template`] implementation for [`RegistryId`] resolution in the current `", stringify!($registry), "` registry.")]
        #[derive(Default)]
        pub struct $tmpl_name {
            /// The name to resolve.
            pub registry_name: RegistryName,
        }

        impl Template for $tmpl_name {
            type Output = RegistryId;

            fn build_template(&self, context: &mut TemplateContext) -> BevyResult<Self::Output> {
                let world = context.entity.world();
                let holder = world.get_resource::<SharedRegistryHolder>().context("get_resource::<SharedRegistryHolder>")?;
                let registry = &*holder.$registry;
                let (id, _) =
                    registry.lookup_name_to_object(self.registry_name.as_ref()).with_context(|| format!("SharedRegistryHolder.{}.lookup_name_to_object({})", stringify!($registry), self.registry_name))?;
                Ok(id)
            }

            fn clone_template(&self) -> Self {
                Self { registry_name: self.registry_name.clone() }
            }
        }

        impl From<RegistryName> for $tmpl_name {
            fn from(value: RegistryName) -> Self {
                Self { registry_name: value }
            }
        }

        impl From<&RegistryName> for $tmpl_name {
            fn from(value: &RegistryName) -> Self {
                Self { registry_name: value.clone() }
            }
        }

        impl From<RegistryNameRef<'_>> for $tmpl_name {
            fn from(value: RegistryNameRef<'_>) -> Self {
                Self { registry_name: value.to_owned() }
            }
        }

        impl From<&RegistryNameRef<'_>> for $tmpl_name {
            fn from(value: &RegistryNameRef<'_>) -> Self {
                Self { registry_name: value.to_owned() }
            }
        }
    };
}

impl_shared_registry_resolver!(SharedBlockRegistryIdResolverTemplate, block_types);
impl_shared_registry_resolver!(SharedBiomeRegistryIdResolverTemplate, biome_types);
impl_shared_registry_resolver!(SharedEntityRegistryIdResolverTemplate, entity_types);

/// Stores backwards mappings from a network ID to the entity that has it.
#[derive(Clone, Debug, Default, Resource)]
pub struct EntityNetworkIdLookupTable {
    network_ids: BTreeMap<NonNilUuid, SmallVec<[Entity; 1]>>,
}

/// Error type for [`EntityNetworkIdLookupTable::get_entity`].
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Error)]
pub enum EntityNetworkIdLookupError {
    /// Zero entities match the query.
    #[error("No entity found with network ID {0}")]
    NoSuchEntity(NonNilUuid),
    /// More than one entity matches the query.
    #[error("Multiple entity found with network ID {0}")]
    MultipleSuchEntities(NonNilUuid),
}

impl EntityNetworkIdLookupTable {
    /// Looks up an entity by the given network ID.
    pub fn get_entity(&self, nid: NonNilUuid) -> Result<Entity, EntityNetworkIdLookupError> {
        match self.network_ids.get(&nid).map(SmallVec::as_slice) {
            None | Some([]) => Err(EntityNetworkIdLookupError::NoSuchEntity(nid)),
            Some([e]) => Ok(*e),
            Some([_, ..]) => Err(EntityNetworkIdLookupError::MultipleSuchEntities(nid)),
        }
    }
}

/// Sets up the state for [`EntityNetworkId`] tracking
pub fn networked_entities_plugin(app: &mut App) {
    app.init_resource::<EntityNetworkIdLookupTable>();
}

/// A unique identifier used for disambiguating entities over the network.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Component)]
#[component(immutable)]
#[component(on_add, on_discard)]
pub struct EntityNetworkId(pub NonNilUuid);

impl Default for EntityNetworkId {
    /// Generates a random identifier
    fn default() -> Self {
        Self(NonNilUuid::new(Uuid::new_v4()).unwrap())
    }
}

impl EntityNetworkId {
    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        let nid = world.get::<Self>(ctx.entity).unwrap().0;
        world
            .resource_mut::<EntityNetworkIdLookupTable>()
            .network_ids
            .entry(nid)
            .or_default()
            .push(ctx.entity);
    }

    fn on_discard(mut world: DeferredWorld, ctx: HookContext) {
        let nid = world.get::<Self>(ctx.entity).unwrap().0;
        let mut table = world.resource_mut::<EntityNetworkIdLookupTable>();
        let Entry::Occupied(mut entry) = table.network_ids.entry(nid) else {
            return;
        };
        entry.get_mut().retain(|e| *e != ctx.entity);
        if entry.get().is_empty() {
            entry.remove();
        }
    }
}
