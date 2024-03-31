//! A data structure for keeping track of a stable mapping between: namespaced strings, numerical IDs and objects.
use std::fmt::{Display, Formatter};
use std::hash::Hash;
use std::num::{NonZeroU32, TryFromIntError};
use std::str::Utf8Error;
use std::sync::Arc;

use bytemuck::{PodInOption, TransparentWrapper, ZeroableInOption};
use hashbrown::{Equivalent, HashMap};
use itertools::Itertools;
use kstring::{KString, KStringRef};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Default namespace for the OpenCubeGame's objects (as a `const` for compile-time functions)
pub const OCG_REGISTRY_DOMAIN_CONST: &str = "ocg";
/// Default namespace for the OpenCubeGame's objects
pub static OCG_REGISTRY_DOMAIN: &str = OCG_REGISTRY_DOMAIN_CONST;
/// Default namespace for the OpenCubeGame's objects, as a [`KString`] for convenience
pub static OCG_REGISTRY_DOMAIN_KS: KString = KString::from_static(OCG_REGISTRY_DOMAIN);

/// Checks if the given name is a valid registry name (`[a-z0-9_]+`).
pub const fn is_valid_registry_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let bytes = name.as_bytes();
    // const-fn safe for loop
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        match byte {
            b'0'..=b'9' => {}
            b'a'..=b'z' => {}
            b'_' => {}
            _ => return false,
        }
        i += 1;
    }
    true
}

/// Simple namespaced registry object name
#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Default, Hash, Serialize, Deserialize)]
pub struct RegistryName {
    /// The namespace
    pub ns: KString,
    /// The object name, unique in the namespace
    pub key: KString,
}

/// Reference to a simple namespaced registry object name, see RegistryNamed for the owned variant
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Default, Hash)]
pub struct RegistryNameRef<'n> {
    /// The namespace
    pub ns: KStringRef<'n>,
    /// The object name, unique in the namespace
    pub key: KStringRef<'n>,
}

impl RegistryName {
    /// Constructs a `ocg:`-namespaced name.
    pub fn ocg(key: &str) -> Self {
        Self {
            ns: OCG_REGISTRY_DOMAIN_KS.clone(),
            key: KString::from_ref(key),
        }
    }

    /// A compile time constructor for `ocg:`-namespaced names.
    pub const fn ocg_const(key: &'static str) -> Self {
        Self {
            ns: KString::from_static(OCG_REGISTRY_DOMAIN_CONST),
            key: KString::from_static(key),
        }
    }

    /// Constructs a name out of the given namespace and key.
    pub fn new(ns: &str, key: &str) -> Self {
        Self {
            ns: KString::from_ref(ns),
            key: KString::from_ref(key),
        }
    }

    /// Constructs a name out of the given namespace and key, at compile time.
    pub const fn new_const(ns: &'static str, key: &'static str) -> Self {
        Self {
            ns: KString::from_static(ns),
            key: KString::from_static(key),
        }
    }

    /// Converts the name to a reference struct.
    pub fn as_ref(&self) -> RegistryNameRef {
        self.into()
    }
}

impl<'a> RegistryNameRef<'a> {
    /// Constructs a `gs:`-namespaced name reference
    pub fn ocg(key: impl Into<KStringRef<'a>>) -> Self {
        Self {
            ns: KStringRef::from(&OCG_REGISTRY_DOMAIN_KS),
            key: key.into(),
        }
    }

    /// Converts the name to an owned struct, copying the strings as needed
    pub fn to_owned(&self) -> RegistryName {
        self.into()
    }
}

impl<'a> Equivalent<RegistryName> for RegistryNameRef<'a> {
    /// Enabled heterogeneous lookup in [`HashMap`] and related types.
    fn equivalent(&self, key: &RegistryName) -> bool {
        key.as_ref() == *self
    }
}

impl<'a> Equivalent<RegistryNameRef<'a>> for RegistryName {
    /// Enabled heterogeneous lookup in [`HashMap`] and related types.
    fn equivalent(&self, key: &RegistryNameRef) -> bool {
        *key == self.as_ref()
    }
}

impl<'a> From<&'a RegistryName> for RegistryNameRef<'a> {
    fn from(value: &'a RegistryName) -> Self {
        RegistryNameRef {
            ns: value.ns.as_ref(),
            key: value.key.as_ref(),
        }
    }
}

impl<'a> From<&RegistryNameRef<'a>> for RegistryName {
    fn from(value: &RegistryNameRef<'a>) -> Self {
        RegistryName {
            ns: value.ns.into(),
            key: value.key.into(),
        }
    }
}

/// Newtype wrapper around a u32 registry ID.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize, TransparentWrapper)]
pub struct RegistryId(pub NonZeroU32);

// SAFETY: transparent NonZeroU32 wrapper, NonZeroU32 implements this trait
unsafe impl ZeroableInOption for RegistryId {}
// SAFETY: transparent NonZeroU32 wrapper, NonZeroU32 implements this trait
unsafe impl PodInOption for RegistryId {}

impl Display for RegistryId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<u32> for RegistryId {
    type Error = TryFromIntError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(Self(NonZeroU32::try_from(value)?))
    }
}

impl Display for RegistryName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.ns, self.key)
    }
}

impl<'a> Display for RegistryNameRef<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.ns, self.key)
    }
}

/// Needs to be implemented on any object that can be a part of a Registry
pub trait RegistryObject: PartialEq + Hash {
    /// Should be trivial
    fn registry_name(&self) -> RegistryNameRef;
}

impl<O: RegistryObject> RegistryObject for Arc<O> {
    fn registry_name(&self) -> RegistryNameRef {
        O::registry_name(self)
    }
}

/// A data structure for keeping track of a stable mapping between: namespaced strings, numerical IDs and objects.
#[derive(Serialize, Deserialize)]
pub struct Registry<Object: RegistryObject> {
    next_free_id: NonZeroU32,
    id_to_obj: Vec<Option<Object>>,
    name_to_id: HashMap<RegistryName, RegistryId>,
}

impl<Object: RegistryObject> Default for Registry<Object> {
    fn default() -> Self {
        Self {
            next_free_id: NonZeroU32::new(1).unwrap(),
            id_to_obj: vec![None],
            name_to_id: HashMap::with_capacity(64),
        }
    }
}

/// Possible errors from Registry operations
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    /// The name given is not a made of legal registry keys.
    #[error("Name {name} is not a legal registry name (made of `[a-z0-9_]+` namespace and key)")]
    IllegalName {
        /// The name that contains an invalid registry key.
        name: RegistryName,
    },
    /// A numeric ID that is already present in the registry was prevented from being overwritten.
    #[error("Id {id} already exists when trying to register {name}")]
    IdAlreadyExists {
        /// The ID already present.
        id: RegistryId,
        /// The name of the object at the ID that was attempted to be overwritten.
        name: RegistryName,
    },
    /// A name that is already present in the registry was prevented from being overwritten.
    #[error("Name {name} already exists in the registry")]
    NameAlreadyExists {
        /// The conflicting name.
        name: RegistryName,
    },
    /// No more unallocated space in the registry. The allocator is a simple bump allocator, so if objects were removed, it might be possible to optimize the registry down to have free space again.
    #[error("No free space in the registry")]
    NoFreeSpace,
}

/// Error type describing what went wrong during registry deserialization.
#[derive(Error, Debug)]
pub enum RegistryDeserializationError {
    /// Error from the capnp message readers.
    #[error("Serialized data could not be read: {0}")]
    CapnpReadError(#[from] capnp::Error),
    /// Invalid UTF8 in message.
    #[error("Invalid UTF8 found: {0}")]
    InvalidUtf8(#[from] Utf8Error),
    /// One or more of the array lenghts is different from the rest.
    #[error("The ID/NS/Key arrays are of unequal lengths")]
    MismatchedArrayLengths,
    /// Found an illegal value for a registry ID.
    #[error("An illegal ID was present in the registry bundle")]
    IllegalID,
    /// Some entries from the remote registry are missing in our registry.
    #[error("The local registry was missing the following names: {0:?}")]
    MissingEntries(Vec<RegistryName>),
    /// Error when inserting entries to the new registry.
    #[error("Error during object insertion: {0}")]
    InsertionError(#[from] RegistryError),
}

/// A registry of up to 2^32-2 named objects.
impl<Object: RegistryObject> Registry<Object> {
    /// Low-level: Allocate the next free ID in the registry
    pub fn allocate_id(&mut self) -> Result<RegistryId, RegistryError> {
        let id = self.next_free_id;
        self.next_free_id = self.next_free_id.checked_add(1).ok_or(RegistryError::NoFreeSpace)?;
        Ok(RegistryId(id))
    }

    /// Try to put the object in the registry, allocating it a new ID.
    /// On failure, no ID is allocated and a precise error is returned.
    pub fn push_object(&mut self, object: Object) -> Result<RegistryId, RegistryError> {
        let name = object.registry_name().to_owned();
        if !is_valid_registry_name(&name.ns) || !is_valid_registry_name(&name.key) {
            return Err(RegistryError::IllegalName { name });
        }
        if self.name_to_id.contains_key(&name) {
            return Err(RegistryError::NameAlreadyExists { name });
        }
        let id = self.allocate_id()?;
        let raw_id = id.0.get() as usize;
        if self.id_to_obj.len() <= raw_id {
            self.id_to_obj.resize_with(raw_id + 32, || None);
        } else if self.id_to_obj[raw_id].is_some() {
            panic!(
                "Freshly allocated ID {:?} already used when trying to allocate for {}",
                id,
                object.registry_name()
            );
        }
        self.id_to_obj[raw_id] = Some(object);
        self.name_to_id.insert(name, id);
        Ok(id)
    }

    /// Low-level: Attempt to insert an object-id pair into the registry directly, useful for deserialization or manually tweaking registry contents.
    pub fn insert_object_with_id(&mut self, id: RegistryId, object: Object) -> Result<(), RegistryError> {
        let raw_id = id.0.get() as usize;
        if id.0 == NonZeroU32::MAX {
            return Err(RegistryError::NoFreeSpace);
        }
        if self.id_to_obj.len() <= raw_id {
            self.id_to_obj.resize_with(raw_id + 32, || None);
        } else if let Some(obj) = self.id_to_obj[raw_id].as_ref() {
            return Err(RegistryError::IdAlreadyExists {
                id,
                name: obj.registry_name().to_owned(),
            });
        }
        let name = object.registry_name().to_owned();
        if !is_valid_registry_name(&name.ns) || !is_valid_registry_name(&name.key) {
            return Err(RegistryError::IllegalName { name });
        }
        if self.name_to_id.contains_key(&name) {
            return Err(RegistryError::NameAlreadyExists { name });
        }
        if id.0 >= self.next_free_id {
            self.next_free_id = id.0.checked_add(1).unwrap();
        }
        self.id_to_obj[raw_id] = Some(object);
        self.name_to_id.insert(name, id);
        Ok(())
    }

    /// Given a namespaced name, look up an object and its ID, or return `None` if it's not found.
    pub fn lookup_name_to_object(&self, name: RegistryNameRef) -> Option<(RegistryId, &Object)> {
        let id = *self.name_to_id.get(&name)?;
        let obj = self.id_to_obj.get(id.0.get() as usize)?.as_ref()?;
        Some((id, obj))
    }

    /// Given a registry object ID, look up an object, or return `None` if it's not found.
    pub fn lookup_id_to_object(&self, id: RegistryId) -> Option<&Object> {
        self.id_to_obj.get(id.0.get() as usize)?.as_ref()
    }

    /// Given a registry object, find look up its ID, or return `None` if it's not found.
    pub fn search_object_to_id(&self, object: &Object) -> Option<RegistryId> {
        self.id_to_obj
            .iter()
            .position(|r| r.as_ref().is_some_and(|o| o == object))
            .map(|i| RegistryId(NonZeroU32::new(i as u32).unwrap()))
    }

    /// Iterates over all the registry objects.
    pub fn iter(&self) -> impl Iterator<Item = (RegistryId, RegistryNameRef, &Object)> {
        self.name_to_id.iter().filter_map(|(name, &id)| {
            self.id_to_obj
                .get(id.0.get() as usize)
                .and_then(Option::as_ref)
                .map(|obj| (id, name.as_ref(), obj))
        })
    }

    /// Serializes the ID-name mappings to a schema bundle message.
    pub fn serialize_ids(&self, builder: &mut crate::schemas::game_types_capnp::registry_id_mapping_bundle::Builder) {
        let mut mappings = self.iter().map(|(id, name, _obj)| (id, name)).collect_vec();
        mappings.sort_by_key(|(id, _name)| *id);
        let len_u32: u32 = mappings.len().try_into().unwrap();
        {
            let mut ids = builder.reborrow().init_ids(len_u32);
            let ids = ids.as_slice().unwrap();
            debug_assert_eq!(mappings.len(), ids.len());
            for (idx, (id, _name)) in mappings.iter().enumerate() {
                ids[idx] = id.0.get();
            }
        }
        {
            let mut namespaces = builder.reborrow().init_nss(len_u32);
            for (idx, (_id, name)) in mappings.iter().enumerate() {
                namespaces.set(idx as u32, name.ns);
            }
        }
        {
            let mut keys = builder.reborrow().init_keys(len_u32);
            for (idx, (_id, name)) in mappings.iter().enumerate() {
                keys.set(idx as u32, name.key);
            }
        }
    }

    /// Constructs a new registry by cloning the entries from this registry that have ID mappings available in the given mapping bundle.
    pub fn clone_with_serialized_ids(
        &self,
        bundle: &crate::schemas::game_types_capnp::registry_id_mapping_bundle::Reader,
    ) -> Result<Self, RegistryDeserializationError>
    where
        Object: Clone,
    {
        let mut out = Self::default();

        let ids = bundle.reborrow().get_ids()?;
        let nss = bundle.reborrow().get_nss()?;
        let keys = bundle.reborrow().get_keys()?;

        if ids.len() != nss.len() || keys.len() != nss.len() {
            return Err(RegistryDeserializationError::MismatchedArrayLengths);
        }

        let mut missing_entries = Vec::new();
        out.name_to_id.reserve(ids.len() as usize);
        out.id_to_obj.reserve(ids.len() as usize);
        for idx in 0..ids.len() {
            let new_id = ids.get(idx);
            let new_id = RegistryId::try_from(new_id).or(Err(RegistryDeserializationError::IllegalID))?;
            let ns = nss.get(idx)?.to_str()?;
            let key = keys.get(idx)?.to_str()?;
            let name = RegistryName::new(ns, key);

            let old_obj = self.lookup_name_to_object(name.as_ref());
            if let Some((_old_id, old_obj)) = old_obj {
                out.insert_object_with_id(new_id, old_obj.clone())?;
            } else {
                missing_entries.push(name);
            }
        }

        if !missing_entries.is_empty() {
            return Err(RegistryDeserializationError::MissingEntries(missing_entries));
        }

        Ok(out)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Clone, Eq, PartialEq, Debug, Default, Hash)]
    struct DummyObject(RegistryName);

    impl RegistryObject for DummyObject {
        fn registry_name(&self) -> RegistryNameRef {
            self.0.as_ref()
        }
    }

    #[test]
    pub fn simple_registry() {
        let mut reg: Registry<DummyObject> = Registry::default();
        let a_id = reg.push_object(DummyObject(RegistryName::ocg("a"))).unwrap();
        assert_eq!(a_id.0.get(), 1);
        let b_id = RegistryId::try_from(2).unwrap();
        let c_id = RegistryId::try_from(3).unwrap(); // non-existent
        reg.insert_object_with_id(b_id, DummyObject(RegistryName::ocg("b")))
            .unwrap();
        assert!(reg.push_object(DummyObject(RegistryName::ocg("a"))).is_err());
        assert!(reg.push_object(DummyObject(RegistryName::ocg("b"))).is_err());
        assert!(reg
            .insert_object_with_id(b_id, DummyObject(RegistryName::ocg("new")))
            .is_err());
        assert!(reg
            .insert_object_with_id(c_id, DummyObject(RegistryName::ocg("b")))
            .is_err());

        assert_eq!(reg.lookup_id_to_object(a_id).map(|o| o.0.key.as_str()), Some("a"));
        assert_eq!(reg.lookup_id_to_object(b_id).map(|o| o.0.key.as_str()), Some("b"));
        assert_eq!(reg.lookup_id_to_object(c_id).map(|o| o.0.key.as_str()), None);

        let dyn_a = KString::from_string(String::from("a"));
        let dyn_b = KString::from_string(String::from("b"));
        let dyn_c = KString::from_string(String::from("c"));

        assert_eq!(
            reg.lookup_name_to_object(RegistryNameRef::ocg(&dyn_a))
                .map(|(id, o)| (id, o.0.key.as_str())),
            Some((a_id, "a"))
        );
        assert_eq!(
            reg.lookup_name_to_object(RegistryNameRef::ocg(&dyn_b))
                .map(|(id, o)| (id, o.0.key.as_str())),
            Some((b_id, "b"))
        );
        assert_eq!(
            reg.lookup_name_to_object(RegistryNameRef::ocg(&dyn_c))
                .map(|(id, o)| (id, o.0.key.as_str())),
            None
        );
    }

    #[test]
    pub fn serialize_registry() {
        let mut original: Registry<DummyObject> = Registry::default();

        let o_a = original.push_object(DummyObject(RegistryName::ocg_const("a"))).unwrap();
        let o_b = original.push_object(DummyObject(RegistryName::ocg_const("b"))).unwrap();
        let o_c = original.push_object(DummyObject(RegistryName::ocg_const("c"))).unwrap();

        let mut original_rev: Registry<DummyObject> = Registry::default();

        let _or_c = original_rev
            .push_object(DummyObject(RegistryName::ocg_const("c")))
            .unwrap();
        let _or_b = original_rev
            .push_object(DummyObject(RegistryName::ocg_const("b")))
            .unwrap();
        let _or_a = original_rev
            .push_object(DummyObject(RegistryName::ocg_const("a")))
            .unwrap();

        let mut missing: Registry<DummyObject> = Registry::default();

        let _m_a = missing.push_object(DummyObject(RegistryName::ocg_const("a"))).unwrap();
        let _m_b = missing.push_object(DummyObject(RegistryName::ocg_const("b"))).unwrap();

        let mut original_message = capnp::message::Builder::default();
        let mut original_bundle =
            original_message.init_root::<crate::schemas::game_types_capnp::registry_id_mapping_bundle::Builder>();
        original.serialize_ids(&mut original_bundle);
        let original_bytes = capnp::serialize::write_message_to_words(&original_message);

        let reader = capnp::serialize::read_message_from_flat_slice(
            &mut &*original_bytes,
            capnp::message::DEFAULT_READER_OPTIONS,
        )
        .unwrap();
        let bundle_reader: crate::schemas::game_types_capnp::registry_id_mapping_bundle::Reader =
            reader.get_root().unwrap();

        let missing_result = missing.clone_with_serialized_ids(&bundle_reader);
        match missing_result {
            Err(RegistryDeserializationError::MissingEntries(me)) => {
                assert_eq!(me, vec![RegistryName::ocg_const("c")]);
            }
            Err(e) => {
                panic!("Registry did not return missing entries error: {e}");
            }
            Ok(_) => {
                panic!("Registry did not return error");
            }
        }

        let original_result = original.clone_with_serialized_ids(&bundle_reader).unwrap();
        assert_eq!(
            original_result
                .lookup_name_to_object(RegistryNameRef::ocg("a"))
                .unwrap()
                .0,
            o_a
        );
        assert_eq!(
            original_result
                .lookup_name_to_object(RegistryNameRef::ocg("b"))
                .unwrap()
                .0,
            o_b
        );
        assert_eq!(
            original_result
                .lookup_name_to_object(RegistryNameRef::ocg("c"))
                .unwrap()
                .0,
            o_c
        );

        let original_rev_result = original_rev.clone_with_serialized_ids(&bundle_reader).unwrap();
        assert_eq!(
            original_rev_result
                .lookup_name_to_object(RegistryNameRef::ocg("a"))
                .unwrap()
                .0,
            o_a
        );
        assert_eq!(
            original_rev_result
                .lookup_name_to_object(RegistryNameRef::ocg("b"))
                .unwrap()
                .0,
            o_b
        );
        assert_eq!(
            original_rev_result
                .lookup_name_to_object(RegistryNameRef::ocg("c"))
                .unwrap()
                .0,
            o_c
        );
    }
}
