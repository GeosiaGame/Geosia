//! The model of player data and behaviour.
//!
//! Players are split into a few parts:
//! - [`PlayerAccount`] - determines the actual person playing the game
//! - [`PlayerCharacter`] - one person can have multiple characters, sometimes used simultaneously e.g. to isolate admin access or record gameplay from a different perspective
//! - `PlayerAvatar` (in the game crates) - the in-game entity representing the character, has an inventory, health, etc.

use std::hash::{Hash, Hasher};
use std::sync::{Arc, LazyLock};
use kstring::KString;
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
pub static NONREGISTERED_PLAYER_URL_BASE: LazyLock<Url> = LazyLock::new(|| Url::parse("https://geosia.localhost").unwrap());

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
    
}

impl PlayerCharacter {
    
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
