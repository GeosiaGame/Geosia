//! The model of player data and behaviour.
//!
//! Players are split into a few parts:
//! - [`PlayerAccount`] - determines the actual person playing the game
//! - [`PlayerCharacter`] - one person can have multiple characters, sometimes used simultaneously e.g. to isolate admin access or record gameplay from a different perspective
//! - `PlayerAvatar` (in the game crates) - the in-game entity representing the character, has an inventory, health, etc.
//!
//! A character's profile is identified by the URL

use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, LazyLock};

use kstring::KString;
use regex::Regex;
use url::Url;
use uuid::{NonNilUuid, Uuid};

/// Simple newtype to avoid mixing up account and character UUIDs.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct AccountId(pub NonNilUuid);
/// Simple newtype to avoid mixing up account and character UUIDs.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct CharacterId(pub NonNilUuid);

/// The special domain host name for unregistered players.
pub static NONREGISTERED_PLAYER_DOMAIN: KString = KString::from_static("geosia.localhost");
/// The special domain host for unregistered players.
pub static NONREGISTERED_PLAYER_URL_BASE: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://geosia.localhost").unwrap());

static URL_UUID_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("(?-u)\\A[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}\\z").unwrap()
});

/// Sample UUID for testing defined as `uuid.uuid5(uuid.NAMESPACE_DNS, 'alice.geosia.org')`
pub static TEST_ALICE_PLAYER_UUID: AccountId =
    AccountId(NonNilUuid::new(Uuid::from_u128(0xf57eb054325c51dda5d41922706f34eb)).unwrap());
/// Sample UUID for testing defined as `uuid.uuid5(uuid.NAMESPACE_DNS, 'bob.geosia.org')`
pub static TEST_BOB_PLAYER_UUID: AccountId =
    AccountId(NonNilUuid::new(Uuid::from_u128(0xbdb682a17df559bd8bd4f35f2202b73b)).unwrap());

/// Sample UUID for testing defined as `uuid.uuid5(uuid.NAMESPACE_DNS, 'character.alice.geosia.org')`
pub static TEST_ALICE_CHARACTER_UUID: CharacterId =
    CharacterId(NonNilUuid::new(Uuid::from_u128(0xf23079e29ebd5702ad4c4be0d595367f)).unwrap());
/// Sample UUID for testing defined as `uuid.uuid5(uuid.NAMESPACE_DNS, 'character.bob.geosia.org')`
pub static TEST_BOB_CHARACTER_UUID: CharacterId =
    CharacterId(NonNilUuid::new(Uuid::from_u128(0x36a69d6963eb5fc4ba1c2bd166bf8b1d)).unwrap());

/// Computes the registration URL of a nonregistered player with the given [`AccountId`].
pub fn nonregistered_player_url(id: AccountId) -> Url {
    let mut url = (*NONREGISTERED_PLAYER_URL_BASE).clone();
    url.set_path(id.0.get().as_hyphenated().encode_lower(&mut Uuid::encode_buffer()));
    url
}

/// Stores information about a player who can log into the game and play one or more characters.
#[derive(Clone, Debug)]
pub struct PlayerAccount {
    /// Where the account is registered, can be [`NONREGISTERED_PLAYER_URL_BASE`] followed by `/UUID-HERE` for non-registered accounts.
    /// E.g. `https://geosia.localhost/ac7cf770-40cd-467c-8d06-82f3fe7cd9c4`
    pub url: Url,
    /// Uniquely identifies this account.
    pub id: AccountId,
    /// Convenience display name, do not rely on this staying the same over time.
    pub display_name: KString,
}

/// Stores default display name, UUID and skin information about a specific character.
#[derive(Clone, Debug)]
pub struct PlayerCharacter {
    /// The account this character belongs to.
    pub account: Arc<PlayerAccount>,
    /// Uniquely identifies this character.
    pub id: CharacterId,
    /// In-game name of the character, do not rely on this staying the same over time.
    pub display_name: KString,
}

impl PlayerAccount {
    /// Parses a player account definition from untrusted inputs.
    pub fn try_parse(url: &str, display_name: &str) -> Result<Self, anyhow::Error> {
        let url = Url::parse(url)?;
        let uuid = url
            .path_segments()
            .ok_or_else(|| anyhow::anyhow!("Player URL must have path components"))?
            .next_back()
            .ok_or_else(|| anyhow::anyhow!("Player URL must have a UUID path component"))?;
        if !URL_UUID_PATTERN.is_match(uuid) {
            return Err(anyhow::anyhow!("Player URL must end in a valid UUID path component"));
        }
        let uuid = Uuid::try_parse(uuid)?;
        let Ok(uuid) = NonNilUuid::try_from(uuid) else {
            return Err(anyhow::anyhow!("Player UUID must not be the nil UUID"));
        };
        if display_name.trim() != display_name {
            return Err(anyhow::anyhow!(
                "Player display name must not start/end with whitespace"
            ));
        }
        if display_name.is_empty() || display_name.chars().all(char::is_whitespace) {
            return Err(anyhow::anyhow!("Player display name cannot be empty"));
        }
        let display_name = KString::from_ref(display_name);
        Ok(Self {
            url,
            id: AccountId(uuid),
            display_name,
        })
    }
}

impl PlayerCharacter {
    /// Parses a player character definition from untrusted inputs.
    pub fn try_parse(account: Arc<PlayerAccount>, uuid: Uuid, display_name: &str) -> Result<Self, anyhow::Error> {
        let Ok(uuid) = NonNilUuid::try_from(uuid) else {
            return Err(anyhow::anyhow!("Character UUID must not be the nil UUID"));
        };
        if display_name.trim() != display_name {
            return Err(anyhow::anyhow!(
                "Character display name must not start/end with whitespace"
            ));
        }
        if display_name.is_empty() || display_name.chars().all(char::is_whitespace) {
            return Err(anyhow::anyhow!("Character display name cannot be empty"));
        }
        let display_name = KString::from_ref(display_name);
        Ok(Self {
            account,
            id: CharacterId(uuid),
            display_name,
        })
    }
}

/// Only compares the `server_url` and `id` fields.
impl PartialEq for PlayerAccount {
    /// Only compares the `server_url` and `id` fields.
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.url == other.url
    }
}
/// Only compares the `server_url` and `id` fields.
impl Eq for PlayerAccount {}
/// Only hashes the `server_url` and `id` fields.
impl Hash for PlayerAccount {
    /// Only hashes the `server_url` and `id` fields.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.url.hash(state);
        self.id.hash(state);
    }
}

impl Display for PlayerAccount {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Player {} ({})", self.display_name, self.url)
    }
}

/// Only compares the `account` and `id` fields.
impl PartialEq for PlayerCharacter {
    /// Only compares the `account` and `id` fields.
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.account == other.account
    }
}
/// Only compares the `account` and `id` fields.
impl Eq for PlayerCharacter {}
/// Only hashes the `account` and `id` fields.
impl Hash for PlayerCharacter {
    /// Only hashes the `account` and `id` fields.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.account.hash(state);
        self.id.hash(state);
    }
}

impl Display for PlayerCharacter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Character {} ({}) of {}", self.display_name, self.id.0, self.account)
    }
}
