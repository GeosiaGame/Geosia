//! A data structure for keeping track of a stable mapping between: namespaced strings, numerical IDs and objects.
use std::fmt::{Display, Formatter};
use std::num::{NonZeroU32, TryFromIntError};

use bytemuck::{PodInOption, TransparentWrapper, ZeroableInOption};
use hashbrown::{Equivalent, HashMap};
use kstring::{KString, KStringRef};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Default namespace for the OpenCubeGame's objects (as a `const` for compile-time functions)
pub const OCG_REGISTRY_DOMAIN_CONST: &str = "ocg";
/// Default namespace for the OpenCubeGame's objects
pub static OCG_REGISTRY_DOMAIN: &str = OCG_REGISTRY_DOMAIN_CONST;
/// Default namespace for the OpenCubeGame's objects, as a [`KString`] for convenience
pub static OCG_REGISTRY_DOMAIN_KS: KString = KString::from_static(OCG_REGISTRY_DOMAIN);

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
    /// Constructs a `ocg:`-namespaced name
    pub fn ocg(key: impl Into<KString>) -> Self {
        Self {
            ns: OCG_REGISTRY_DOMAIN_KS.clone(),
            key: key.into(),
        }
    }

    /// A compiletime constructor for `ocg:`-namespaced names
    pub const fn ocg_const(key: &'static str) -> Self {
        Self {
            ns: KString::from_static(OCG_REGISTRY_DOMAIN_CONST),
            key: KString::from_static(key),
        }
    }

    /// Converts the name to a reference struct
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
pub trait RegistryObject: PartialEq {
    /// Should be trivial
    fn registry_name(&self) -> RegistryNameRef;
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
#[derive(Debug, Error)]
pub enum RegistryError {
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
    pub fn lookup_object_to_id(&self, object: &Object) -> Option<RegistryId> {
        self.id_to_obj
            .iter()
            .position(|r| r.as_ref().is_some_and(|o| o == object))
            .map(|i| RegistryId(NonZeroU32::new(i as u32).unwrap()))
    }

    /// Gets a `Vec` of all the ID -> Object mappings in this registry.
    pub fn get_objects_ids(&self) -> Vec<(&RegistryId, &Object)> {
        let mut result = Vec::new();
        for id in self.name_to_id.values().into_iter() {
            let obj = self.lookup_id_to_object(*id);
            if let Some(o) = obj {
                result.push((id, o));
            }
        }
        result
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Clone, Eq, PartialEq, Debug, Default)]
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
}
